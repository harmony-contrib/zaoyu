//! Native paths and persistent picker grants. Bridge work runs on the OHOS worker.

use openharmony_ability::OpenHarmonyApp;
use std::{
    collections::BTreeMap,
    io::{Read as _, Write as _},
    os::unix::fs::OpenOptionsExt as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};
use winit::platform::ohos::{
    drag_drop,
    file_access::{self, permissions},
    plugins::files::{FileDialogOptions, FilesExt as _},
};

pub(super) struct Grants {
    directory: PathBuf,
    uris: Mutex<BTreeMap<String, u32>>,
}

impl Grants {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self { directory, uris: Mutex::new(BTreeMap::new()) }
    }

    /// Restore on a worker before any editor code reads its recent paths.
    pub(super) fn restore(self: &Arc<Self>) -> Result<(), String> {
        let grants = Arc::clone(self);
        std::thread::Builder::new()
            .name("file-grants".into())
            .spawn(move || {
                let Ok(bytes) = read_bounded(&grants.directory.join("file-grants.json"), 1024 * 1024) else { return };
                let Ok(saved) = serde_json::from_slice::<BTreeMap<String, u32>>(&bytes) else { return };
                for (uri, mode) in saved.into_iter().take(4096) {
                    if uri.len() > 16 * 1024 || !matches!(mode, 1 | 3) {
                        continue;
                    }
                    let policy = permissions::PolicyInfo { uri: uri.clone(), operation_mode: mode };
                    match permissions::activate_permission(&[policy]) {
                        Ok(failed) if failed.is_empty() => {
                            grants.uris.lock().unwrap_or_else(PoisonError::into_inner).insert(uri, mode);
                        }
                        _ => log::warn!("A saved file grant is unavailable; select the file again"),
                    }
                }
            })
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|_| "The file permission worker stopped".to_owned())
    }

    fn persist(&self, uris: &[String], writable: bool) -> Result<Vec<PathBuf>, String> {
        if uris.is_empty() {
            return Ok(Vec::new());
        }
        if uris.len() > 256 || uris.iter().any(|uri| uri.len() > 16 * 1024) {
            return Err("Too many selected files or an invalid file URI".into());
        }
        let mut saved = self.uris.lock().unwrap_or_else(PoisonError::into_inner);
        if saved.len().saturating_add(uris.len()) > 4096 {
            return Err("Too many saved grants; select a containing folder instead".into());
        }
        let paths = file_access::persist_paths(uris, writable)?;
        for uri in uris {
            let mode = if writable { 3 } else { 1 };
            saved.entry(uri.clone()).and_modify(|old| *old |= mode).or_insert(mode);
        }
        let bytes = serde_json::to_vec(&*saved).map_err(|error| error.to_string())?;
        write_atomic(&self.directory.join("file-grants.json"), &bytes)?;
        Ok(paths)
    }

    fn can_replace(&self, path: &Path) -> bool {
        if path.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
            return false;
        }
        let Some(parent) = path.parent() else { return false };
        parent.starts_with(&self.directory)
            || self.uris.lock().unwrap_or_else(PoisonError::into_inner).iter().any(|(uri, mode)| {
                *mode == 3 && drag_drop::file_path(uri).is_some_and(|folder| folder.is_dir() && parent.starts_with(folder))
            })
    }

    async fn authorize_save(&self, ability: &OpenHarmonyApp, path: &Path) -> Result<bool, String> {
        if self.can_replace(path) {
            return Ok(true);
        }
        // Upstream writers replace through a sibling temporary file, which requires a folder grant.
        let parent = path.parent().ok_or("The save path has no containing folder")?;
        let uri = drag_drop::file_uri(parent).ok_or("Cannot resolve the save folder URI")?;
        let response = ability
            .show_file_dialog(FileDialogOptions::new("open-folder").default_location(uri))
            .await
            .map_err(|error| error.to_string())?;
        let Some(selected) = response.files.first() else { return Ok(false) };
        if drag_drop::file_path(selected).as_deref() != Some(parent) {
            return Err(format!("Select {} to save this file, or choose another destination", parent.display()));
        }
        self.persist(&response.files, true)?;
        Ok(true)
    }
}

/// Run from the Ability worker; an empty result is a cancelled dialog.
pub(super) async fn pick(ability: &OpenHarmonyApp, grants: &Grants, options: FileDialogOptions) -> Result<Vec<PathBuf>, String> {
    let save = options.dialog_type == "save-file";
    let folder = options.dialog_type == "open-folder";
    let response = ability.show_file_dialog(options).await.map_err(|error| error.to_string())?;
    if save {
        let Some(uri) = response.files.first() else { return Ok(Vec::new()) };
        let path = drag_drop::file_path(uri).filter(|path| path.is_absolute()).ok_or("Cannot resolve the save URI")?;
        return Ok(if grants.authorize_save(ability, &path).await? { vec![path] } else { Vec::new() });
    }
    grants.persist(&response.files, folder)
}

pub(super) fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > max {
        return Err("The file is too large or is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1)).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max {
        return Err("The file is too large".into());
    }
    Ok(bytes)
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().ok_or("The destination has no file name")?.to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut file =
        std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|error| error.to_string())
}

//! File pickers and durable URI grants, modeled on PhotoCraft and PdfCraft.
//! A writable containing folder is required for atomic project saves and exports.

use openharmony_ability::OpenHarmonyApp;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError, mpsc},
};
use winit::platform::ohos::{
    file_access::{self, permissions},
    plugins::files::{FileDialogFilter, FileDialogOptions, FilesExt as _},
};

use filmcraft_ui_egui::native_dialogs::FileDialog;

use super::grant_store::{MAX_BYTES as MAX_GRANTS_BYTES, MAX_ENTRIES as MAX_GRANTS};

const SAVE_FOLDER_MODE: u32 = 1 | 2;

pub struct Grants {
    directory: PathBuf,
    update: Mutex<()>,
    uris: Mutex<BTreeMap<String, u32>>,
    writable: Mutex<Vec<PathBuf>>,
}

impl Grants {
    pub fn new(directory: PathBuf) -> Self {
        Self { writable: Mutex::new(vec![directory.clone()]), directory, update: Mutex::new(()), uris: Mutex::new(BTreeMap::new()) }
    }

    pub fn restore(&self) -> Result<(), String> {
        let file = match std::fs::File::open(self.directory.join("file-grants.json")) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Cannot read saved file grants: {error}")),
        };
        let mut bytes = Vec::new();
        file.take(MAX_GRANTS_BYTES + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
        let saved = super::grant_store::parse(&bytes)?;
        for (uri, mode) in saved {
            let policy = permissions::PolicyInfo { uri: uri.clone(), operation_mode: mode };
            match permissions::activate_permission(&[policy]) {
                Ok(failed) if failed.is_empty() => self.track(&uri, mode),
                _ => log::warn!("A saved file grant is no longer available; select the file again"),
            }
        }
        Ok(())
    }

    fn track(&self, uri: &str, mode: u32) {
        self.uris.lock().unwrap_or_else(PoisonError::into_inner).entry(uri.to_owned()).and_modify(|old| *old |= mode).or_insert(mode);
        if mode == SAVE_FOLDER_MODE
            && let Some(path) = winit::platform::ohos::drag_drop::file_path(uri).filter(|path| path.is_dir())
        {
            let mut writable = self.writable.lock().unwrap_or_else(PoisonError::into_inner);
            if !writable.contains(&path) {
                writable.push(path);
            }
        }
    }

    pub fn persist(&self, selected: &[String], write: bool) -> Result<Vec<PathBuf>, String> {
        if selected.is_empty() {
            return Ok(Vec::new());
        }
        let _update = self.update.lock().unwrap_or_else(PoisonError::into_inner);
        let total_bytes =
            selected.iter().try_fold(0u64, |size, uri| size.checked_add(uri.len() as u64)).ok_or("Selected file URIs are too large")?;
        if total_bytes > MAX_GRANTS_BYTES {
            return Err("Selected file URIs exceed the 1 MiB limit".into());
        }
        if selected.len() > MAX_GRANTS {
            return Err("Too many selected files".into());
        }
        let paths = selected
            .iter()
            .map(|uri| winit::platform::ohos::drag_drop::file_path(uri).ok_or_else(|| "Cannot resolve selected URI".to_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        let individual: Vec<_> = selected.iter().zip(&paths).filter(|(_, path)| !self.can_save(path)).map(|(uri, _)| uri.clone()).collect();
        {
            let uris = self.uris.lock().unwrap_or_else(PoisonError::into_inner);
            if uris.len().saturating_add(individual.iter().filter(|uri| !uris.contains_key(*uri)).count()) > MAX_GRANTS {
                return Err("Too many saved file grants".into());
            }
        }
        file_access::persist_paths(&individual, write)?;
        for uri in individual {
            self.track(&uri, if write { SAVE_FOLDER_MODE } else { 1 });
        }
        let uris = self.uris.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes = serde_json::to_vec(&*uris).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_GRANTS_BYTES {
            return Err("Saved file grants exceed the 1 MiB limit".into());
        }
        filmcraft_format::atomic_write(&self.directory.join("file-grants.json"), &bytes)
            .map_err(|error| format!("Cannot save file grants: {error}"))?;
        Ok(paths)
    }

    pub fn can_save(&self, path: &Path) -> bool {
        // Reject lexical traversal before checking a directory prefix.
        if !path.is_absolute() || path.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
            return false;
        }
        path.parent()
            .is_some_and(|parent| self.writable.lock().unwrap_or_else(PoisonError::into_inner).iter().any(|dir| parent.starts_with(dir)))
    }

    async fn authorize_save(&self, app: &OpenHarmonyApp, path: &Path) -> Result<bool, String> {
        if self.can_save(path) {
            return Ok(true);
        }
        let parent = path.parent().ok_or("The save path has no containing folder")?;
        let uri = winit::platform::ohos::drag_drop::file_uri(parent).ok_or("Cannot resolve the save folder URI")?;
        let response =
            app.show_file_dialog(FileDialogOptions::new("open-folder").default_location(uri)).await.map_err(|error| error.to_string())?;
        let Some(selected) = response.files.first() else { return Ok(false) };
        if winit::platform::ohos::drag_drop::file_path(selected).as_deref() != Some(parent) {
            return Err(format!("Select {} to save this file safely, or use Save As to choose another location", parent.display()));
        }
        self.persist(&response.files, true)?;
        Ok(true)
    }

    pub fn resolve_want(&self, uri: &str) -> Result<PathBuf, String> {
        if uri.is_empty() || uri.len() as u64 > MAX_GRANTS_BYTES {
            return Err("Opened file URI is empty or exceeds the 1 MiB limit".into());
        }
        match self.persist(&[uri.to_owned()], false) {
            Ok(paths) => paths.into_iter().next().ok_or_else(|| "Cannot resolve opened file URI".into()),
            Err(error) => {
                // A Want may carry a temporary read grant even when persistence is unavailable.
                log::warn!("Cannot retain the opened file permission: {error}; select the file again after restarting");
                winit::platform::ohos::drag_drop::file_path(uri).ok_or_else(|| "Cannot resolve opened file URI".into())
            }
        }
    }
}

fn filters(name: &str, exts: &[String]) -> Vec<FileDialogFilter> {
    let clean: Vec<_> = exts.iter().map(|ext| ext.trim().trim_start_matches('.')).filter(|ext| !ext.is_empty()).collect();
    if clean.is_empty() { Vec::new() } else { vec![FileDialogFilter::new().name(name).pattern(clean.join(";"))] }
}

async fn choose(grants: &Grants, app: &OpenHarmonyApp, request: FileDialog) -> Result<Vec<String>, String> {
    let save_filters = match &request {
        FileDialog::SaveProject { .. } => filters("FilmCraft Project", &["fcproj".into()]),
        FileDialog::SaveAs { filter, exts, .. } => filters(filter, exts),
        _ => Vec::new(),
    };
    let mut directory = None;
    let mut save = false;
    let mut folder = false;
    let options = match request {
        FileDialog::OpenFiles { exts } => FileDialogOptions::new("open-file").allow_many(true).filters(filters("Media", &exts)),
        FileDialog::OpenFile { filter, exts } => FileDialogOptions::new("open-file").filters(filters(&filter, &exts)),
        FileDialog::Relink { exts, .. } => FileDialogOptions::new("open-file").filters(filters("Media", &exts)),
        FileDialog::OpenProject => FileDialogOptions::new("open-file").filters(vec![
            FileDialogFilter::new().name("FilmCraft Project").pattern("fcproj"),
            FileDialogFilter::new().name("All files").pattern("*"),
        ]),
        FileDialog::Folder { at } => {
            folder = true;
            directory = at.map(PathBuf::from);
            FileDialogOptions::new("open-folder")
        }
        FileDialog::SaveProject { name } | FileDialog::SaveAs { name, .. } => {
            save = true;
            let path = Path::new(&name);
            directory = path.parent().filter(|parent| parent.is_absolute()).map(Path::to_path_buf);
            FileDialogOptions::new("save-file")
                .filters(save_filters)
                .suggested_name(path.file_name().map_or_else(|| "Untitled.fcproj".into(), |name| name.to_string_lossy().into_owned()))
        }
    };
    let options = match directory.as_deref().and_then(winit::platform::ohos::drag_drop::file_uri) {
        Some(uri) => options.default_location(uri),
        None => options,
    };
    let response = app.show_file_dialog(options).await.map_err(|error| error.to_string())?;
    let paths = if save {
        let Some(uri) = response.files.first() else { return Ok(Vec::new()) };
        let path = winit::platform::ohos::drag_drop::file_path(uri).ok_or("Cannot resolve selected save URI")?;
        if !grants.authorize_save(app, &path).await? {
            return Ok(Vec::new());
        }
        vec![path]
    } else {
        grants.persist(&response.files, folder)?
    };
    Ok(paths.into_iter().map(|path| path.to_string_lossy().into_owned()).collect())
}

pub fn dialog_runner(
    app: OpenHarmonyApp,
    grants: Arc<Grants>,
    ctx: egui::Context,
    errors: mpsc::Sender<String>,
) -> Box<dyn FnMut(FileDialog) -> mpsc::Receiver<Vec<String>>> {
    Box::new(move |request| {
        let (app, grants) = (app.clone(), Arc::clone(&grants));
        super::jobs::spawn("files", async move { choose(&grants, &app, request).await }, ctx.clone(), errors.clone())
    })
}

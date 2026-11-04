//! OpenHarmony file picker, save filters, and persistent URI grants (`file-grants.json`).
//!
//! Atomic replacement needs the containing directory: a single-file grant cannot create a sibling
//! temporary file.

use openharmony_ability::OpenHarmonyApp;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};
use vectorcraft_ui_egui::{FilePick, picks::PickRequest};
use winit::platform::ohos::{
    file_access::{self, permissions},
    plugins::files::{FileDialogFilter, FileDialogOptions, FilesExt as _},
    task,
};

// A directory read/write grant covers creating and replacing its children.
const SAVE_FOLDER_MODE: u32 = 1 | 2;

pub struct Grants {
    directory: PathBuf,
    uris: Mutex<BTreeMap<String, u32>>,
    writable: Mutex<Vec<PathBuf>>,
}

impl Grants {
    pub fn new(directory: PathBuf) -> Self {
        Self { writable: Mutex::new(vec![directory.clone()]), directory, uris: Mutex::new(BTreeMap::new()) }
    }

    /// Called on the startup worker, before recent files can be opened.
    pub fn restore(&self) {
        let saved = read_limited(&self.directory.join("file-grants.json"), 1024 * 1024);
        let saved: BTreeMap<String, u32> = match saved {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            _ => return,
        };
        for (uri, mode) in saved {
            if !matches!(mode, 1 | 3) {
                continue;
            }
            let policy = permissions::PolicyInfo { uri: uri.clone(), operation_mode: mode };
            match permissions::activate_permission(&[policy]) {
                Ok(failed) if failed.is_empty() => self.track(&uri, mode),
                _ => log::warn!("A saved file grant is no longer available; select the file again"),
            }
        }
    }

    fn track(&self, uri: &str, mode: u32) {
        self.uris.lock().unwrap_or_else(PoisonError::into_inner).entry(uri.to_owned()).and_modify(|old| *old |= mode).or_insert(mode);
        if mode == SAVE_FOLDER_MODE
            && let Some(path) = winit::platform::ohos::drag_drop::file_path(uri).filter(|p| p.is_dir())
        {
            let mut writable = self.writable.lock().unwrap_or_else(PoisonError::into_inner);
            if !writable.contains(&path) {
                writable.push(path);
            }
        }
    }

    pub fn persist(&self, uris: &[String], write: bool) -> Result<Vec<PathBuf>, String> {
        if uris.len() > 256 || uris.iter().any(|uri| uri.len() > 16 * 1024) {
            return Err("The file selection exceeds the URI limit".into());
        }
        let paths = uris
            .iter()
            .map(|uri| winit::platform::ohos::drag_drop::file_path(uri).ok_or_else(|| "Cannot resolve selected URI".to_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        // Avoid redundant file policies inside an explicitly selected writable folder.
        let individual: Vec<String> =
            uris.iter().zip(&paths).filter(|(_, path)| !self.can_save(path)).map(|(uri, _)| uri.clone()).collect();
        file_access::persist_paths(&individual, write)?;
        for uri in &individual {
            let mode = if write { SAVE_FOLDER_MODE } else { 1 };
            self.track(uri, mode);
        }
        let uris = self.uris.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes = serde_json::to_vec(&*uris).map_err(|e| e.to_string())?;
        vectorcraft_format::write_atomic(&self.directory.join("file-grants.json"), &bytes).map_err(|e| e.to_string())?;
        Ok(paths)
    }

    pub fn can_save(&self, path: &Path) -> bool {
        super::policy::can_save(path, &self.writable.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// The OS picker is the user's explicit authorization for this directory.
    /// Cancellation leaves the original document and destination untouched.
    pub async fn authorize_save(&self, app: &OpenHarmonyApp, path: &Path) -> Result<bool, String> {
        if self.can_save(path) {
            return Ok(true);
        }
        let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let parent = target.parent().ok_or("The save path has no containing folder")?;
        let uri = winit::platform::ohos::drag_drop::file_uri(parent).ok_or("Cannot resolve the save folder URI")?;
        let response =
            app.show_file_dialog(FileDialogOptions::new("open-folder").default_location(uri)).await.map_err(|e| e.to_string())?;
        let Some(selected) = response.files.first() else { return Ok(false) };
        if winit::platform::ohos::drag_drop::file_path(selected).and_then(|path| std::fs::canonicalize(path).ok()).as_deref()
            != std::fs::canonicalize(parent).ok().as_deref()
        {
            return Err(format!("Select {} to save this file safely, or use Save As to choose another location", parent.display()));
        }
        self.persist(&response.files, true)?;
        Ok(self.can_save(path))
    }
}

/// Bounded reads also cover files received from another application in a Want.
pub fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err(format!("{} exceeds the file size limit", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the file size limit", path.display()));
    }
    Ok(bytes)
}

pub fn read(path: &str) -> Result<Vec<u8>, String> {
    read_limited(Path::new(path), 512 * 1024 * 1024)
}

/// As in PhotoCraft, a bridge future can be awaited by a save worker without a runtime.
pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    struct Unpark(std::thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::park();
    }
}

fn filters(pick: &FilePick) -> Vec<FileDialogFilter> {
    pick.filters
        .iter()
        .filter(|(_, extensions)| !extensions.is_empty())
        .map(|(name, extensions)| FileDialogFilter::new().name(name.clone()).pattern(extensions.join(";")))
        .collect()
}

pub async fn pick(ability: &OpenHarmonyApp, grants: &Grants, request: PickRequest) -> Result<Vec<String>, String> {
    let save = matches!(request, PickRequest::Save(_));
    let folder = matches!(request, PickRequest::Folder);
    let options = match request {
        PickRequest::Open(pick) | PickRequest::Save(pick) => {
            let mut options = FileDialogOptions::new(if save { "save-file" } else { "open-file" }).filters(filters(&pick));
            if !pick.name.is_empty() {
                let name = Path::new(&pick.name).file_name().unwrap_or_default().to_string_lossy().into_owned();
                options = options.suggested_name(name);
            }
            if let Some(uri) = pick.folder.as_deref().and_then(|path| winit::platform::ohos::drag_drop::file_uri(Path::new(path))) {
                options = options.default_location(uri);
            }
            options
        }
        PickRequest::OpenMany => FileDialogOptions::new("open-file").allow_many(true).filters(
            vectorcraft_engine::cmd::fileio::place_filters()
                .map(|(name, extensions)| FileDialogFilter::new().name(name).pattern(extensions.join(";")))
                .collect(),
        ),
        PickRequest::Folder => FileDialogOptions::new("open-folder"),
    };
    let response = ability.show_file_dialog(options).await.map_err(|e| e.to_string())?;
    if save {
        let Some(uri) = response.files.first() else { return Ok(Vec::new()) };
        let path = winit::platform::ohos::drag_drop::file_path(uri).ok_or("Cannot resolve selected save URI")?;
        return Ok(if grants.authorize_save(ability, &path).await? { vec![path.to_string_lossy().into_owned()] } else { Vec::new() });
    }
    Ok(grants.persist(&response.files, folder)?.into_iter().map(|p| p.to_string_lossy().into_owned()).collect())
}

pub type Errors = Arc<Mutex<Vec<String>>>;

pub fn report(errors: &Errors, error: String) {
    log::error!("{error}");
    let mut errors = errors.lock().unwrap_or_else(PoisonError::into_inner);
    // Keep a burst of failed external requests from growing the host's queue without bound.
    if errors.len() < 32 {
        errors.push(error);
    }
}

pub fn start_pick(
    ability: OpenHarmonyApp,
    grants: Arc<Grants>,
    ctx: egui::Context,
    errors: Errors,
) -> vectorcraft_ui_egui::picks::StartPick {
    Box::new(move |request| {
        let (tx, rx) = std::sync::mpsc::channel();
        let (ability, grants, ctx, errors) = (ability.clone(), grants.clone(), ctx.clone(), errors.clone());
        task::spawn(async move {
            let paths = match pick(&ability, &grants, request).await {
                Ok(paths) => paths,
                Err(error) => {
                    report(&errors, error);
                    Vec::new()
                }
            };
            // A closed app has no use for its dialog's answer.
            let _ = tx.send(paths);
            ctx.request_repaint();
        });
        Some(rx)
    })
}

pub fn authorize(ability: &OpenHarmonyApp, grants: &Grants, path: &str) -> Result<(), String> {
    if block_on(grants.authorize_save(ability, Path::new(path)))? { Ok(()) } else { Err("Save cancelled".into()) }
}

pub fn resolve_want(grants: &Grants, uri: &str) -> Option<PathBuf> {
    match grants.persist(&[uri.to_owned()], false) {
        Ok(paths) => paths.into_iter().next(),
        Err(error) => {
            // A launch Want can carry a temporary grant even when persistence is unavailable.
            log::warn!("Could not persist the incoming file grant: {error}");
            winit::platform::ohos::drag_drop::file_path(uri)
        }
    }
}

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
use winit::platform::ohos::{
    file_access::{self, permissions},
    plugins::files::{FileDialogFilter, FileDialogOptions, FilesExt as _},
    task,
};

const OPEN_EXTS: &[&str] = &["xlsx", "xlsm", "xls", "xlsb", "ods", "csv", "tsv", "txt", "json"];

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

    /// Reactivate grants before the first workbook can be opened.
    pub fn restore(&self) {
        let saved = read_bounded(&self.directory.join("file-grants.json"), 1024 * 1024);
        let saved: BTreeMap<String, u32> = match saved {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes).unwrap_or_default(),
            _ => return,
        };
        for (uri, mode) in saved.into_iter().take(4096) {
            if uri.len() > 16 * 1024 || !matches!(mode, 1 | 3) {
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
        if uris.len() > 256
            || uris.iter().any(|uri| uri.len() > 16 * 1024)
            || self.uris.lock().unwrap_or_else(PoisonError::into_inner).len().saturating_add(uris.len()) > 4096
        {
            return Err("Too many file grants; select a containing folder instead".into());
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
        write_atomic(&self.directory.join("file-grants.json"), &bytes)?;
        Ok(paths)
    }

    pub fn can_save(&self, path: &Path) -> bool {
        if path.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
            return false;
        }
        path.parent()
            .is_some_and(|parent| self.writable.lock().unwrap_or_else(PoisonError::into_inner).iter().any(|dir| parent.starts_with(dir)))
    }

    /// The OS picker is the user's explicit authorization for this directory.
    /// Cancellation leaves the original document and destination untouched.
    pub async fn authorize_save(&self, app: &OpenHarmonyApp, path: &Path) -> Result<bool, String> {
        if self.can_save(path) {
            return Ok(true);
        }
        let parent = path.parent().ok_or("The save path has no containing folder")?;
        let uri = winit::platform::ohos::drag_drop::file_uri(parent).ok_or("Cannot resolve the save folder URI")?;
        let response =
            app.show_file_dialog(FileDialogOptions::new("open-folder").default_location(uri)).await.map_err(|e| e.to_string())?;
        let Some(selected) = response.files.first() else { return Ok(false) };
        if winit::platform::ohos::drag_drop::file_path(selected).as_deref() != Some(parent) {
            return Err(format!("Select {} to save this file safely, or use Save As to choose another location", parent.display()));
        }
        self.persist(&response.files, true)?;
        Ok(true)
    }

    pub fn resolve_want(&self, uri: &str) -> Option<PathBuf> {
        if uri.is_empty() || uri.len() > 16 * 1024 {
            return None;
        }
        if let Ok(paths) = self.persist(&[uri.to_owned()], false)
            && let Some(path) = paths.into_iter().next()
        {
            return Some(path);
        }
        winit::platform::ohos::drag_drop::file_path(uri)
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    gridcraft_engine::io::write_file(&path.to_string_lossy(), bytes).map_err(|error| error.to_string())
}

pub fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > max {
        return Err("The selected file is too large or is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1)).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max {
        return Err("The selected file is too large".into());
    }
    Ok(bytes)
}

pub fn open_async(ability: OpenHarmonyApp, grants: Arc<Grants>, inbox: gridcraft_ui_egui::Inbox, ctx: egui::Context, picture: bool) {
    task::spawn(async move {
        let result: Result<(), String> = async {
            let extensions = if picture { &["png", "jpg", "jpeg"][..] } else { OPEN_EXTS };
            let options = FileDialogOptions::new("open-file").allow_many(false).filters(vec![
                FileDialogFilter::new().name(if picture { "Pictures" } else { "Spreadsheets" }).pattern(extensions.join(";")),
            ]);
            let response = ability.show_file_dialog(options).await.map_err(|error| error.to_string())?;
            let paths = grants.persist(&response.files, false)?;
            if let Some(path) = paths.first() {
                enqueue_file(path, &inbox)?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            log::warn!("GridCraft OpenHarmony file open failed: {error}");
        }
        ctx.request_repaint();
    });
}

/// Wants have read grants; use the same import flow as picked workbooks so Save asks for an
/// authorized destination instead of trying to replace a read-only external document.
pub fn open_uri(grants: Arc<Grants>, uri: String, inbox: gridcraft_ui_egui::Inbox, ctx: egui::Context) {
    task::spawn(async move {
        let result = grants
            .resolve_want(&uri)
            .ok_or_else(|| "Cannot resolve opened file URI".to_owned())
            .and_then(|path| enqueue_file(&path, &inbox));
        if let Err(error) = result {
            log::warn!("GridCraft OpenHarmony Want file open failed: {error}");
        }
        ctx.request_repaint();
    });
}

fn enqueue_file(path: &Path, inbox: &gridcraft_ui_egui::Inbox) -> Result<(), String> {
    let bytes = read_bounded(path, 256 * 1024 * 1024)?;
    let name = path.file_name().ok_or("The selected file has no name")?.to_string_lossy().into_owned();
    let mut queue = inbox.lock().unwrap_or_else(PoisonError::into_inner);
    if queue.len() >= 8 {
        return Err("Too many pending files; wait for the previous files to open".into());
    }
    queue.push((name, bytes));
    Ok(())
}

const SAVE_FILTERS: &[(&str, &str)] = &[
    ("Excel Workbook", "xlsx"),
    ("Macro-enabled Workbook", "xlsm"),
    ("CSV UTF-8", "csv"),
    ("Tab-separated Text", "tsv"),
    ("Web Page", "html"),
    ("PDF", "pdf"),
];

pub fn pick_save(ability: &OpenHarmonyApp, grants: &Arc<Grants>, suggested: &str) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ability = ability.clone();
    let grants = Arc::clone(grants);
    let suggested = Path::new(suggested);
    let extension = suggested
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|ext| SAVE_FILTERS.iter().any(|(_, candidate)| candidate == ext))
        .unwrap_or_else(|| "xlsx".into());
    let stem = suggested.file_stem().and_then(|stem| stem.to_str()).unwrap_or("Book1");
    let name = format!("{stem}.{extension}");
    task::spawn(async move {
        let result: Result<Option<String>, String> = async {
            let filters = SAVE_FILTERS
                .iter()
                .filter(|(_, ext)| *ext == extension)
                .chain(SAVE_FILTERS.iter().filter(|(_, ext)| *ext != extension))
                .map(|(name, ext)| FileDialogFilter::new().name(*name).pattern(*ext))
                .collect();
            let options = FileDialogOptions::new("save-file").filters(filters).suggested_name(name);
            let response = ability.show_file_dialog(options).await.map_err(|error| error.to_string())?;
            let Some(uri) = response.files.first() else { return Ok(None) };
            let path = winit::platform::ohos::drag_drop::file_path(uri).ok_or("Cannot resolve selected save URI")?;
            Ok(grants.authorize_save(&ability, &path).await?.then(|| path.to_string_lossy().into_owned()))
        }
        .await;
        match result {
            Ok(path) => {
                let _ = tx.send(path);
            }
            Err(error) => log::warn!("GridCraft OpenHarmony save dialog failed: {error}"),
        }
    });
    // Ability bridge calls run on its worker and ArkTS thread, independently of egui's thread.
    rx.recv().ok().flatten()
}

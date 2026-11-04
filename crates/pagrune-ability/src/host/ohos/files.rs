//! OpenHarmony file picker, save filters, and persistent URI grants (`file-grants.json`).
//!
//! Atomic replacement needs the containing directory: a single-file grant cannot create a sibling
//! temporary file.

use openharmony_ability::OpenHarmonyApp;
use pdfcraft_ui_egui::{OhosDialogKind, OhosDialogRequest};
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

const OPEN_EXTS: &[&str] = &["pdf", "png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "jp2", "j2k", "jpx", "txt"];

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
        let saved = std::fs::read(self.directory.join("file-grants.json"));
        let saved: BTreeMap<String, u32> = match saved {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes).unwrap_or_default(),
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
        let _ = std::fs::write(self.directory.join("file-grants.json"), &bytes);
        Ok(paths)
    }

    pub fn can_save(&self, path: &Path) -> bool {
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
        if uri.is_empty() {
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

fn dialog_filters(filters: &[(String, Vec<String>)]) -> Vec<FileDialogFilter> {
    let mut out: Vec<FileDialogFilter> = filters
        .iter()
        .filter_map(|(name, exts)| {
            let clean: Vec<String> =
                exts.iter().map(|ext| ext.trim().trim_start_matches('.').to_owned()).filter(|ext| !ext.is_empty()).collect();
            (!clean.is_empty()).then(|| FileDialogFilter::new().name(name.clone()).pattern(clean.join(";")))
        })
        .collect();
    if out.is_empty() {
        out.push(FileDialogFilter::new().name("Supported Files").pattern(OPEN_EXTS.join(";")));
    }
    out
}

pub fn handle_dialog(grants: &Arc<Grants>, ability: &OpenHarmonyApp, req: OhosDialogRequest) -> Vec<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel();
    let app = ability.clone();
    let grants = Arc::clone(grants);
    task::spawn(async move {
        let result: Result<Vec<PathBuf>, String> = async {
            let with_dir = |options: FileDialogOptions, dir: Option<&Path>| match dir.and_then(winit::platform::ohos::drag_drop::file_uri) {
                Some(uri) => options.default_location(uri),
                None => options,
            };
            match req.kind {
                OhosDialogKind::OpenFile | OhosDialogKind::OpenFiles => {
                    let multiple = matches!(req.kind, OhosDialogKind::OpenFiles);
                    let options = with_dir(
                        FileDialogOptions::new("open-file").allow_many(multiple).filters(dialog_filters(&req.filters)),
                        req.directory.as_deref(),
                    );
                    let response = app.show_file_dialog(options).await.map_err(|e| e.to_string())?;
                    grants.persist(&response.files, false)
                }
                OhosDialogKind::PickFolder => {
                    let options = with_dir(FileDialogOptions::new("open-folder"), req.directory.as_deref());
                    let response = app.show_file_dialog(options).await.map_err(|e| e.to_string())?;
                    grants.persist(&response.files, true)
                }
                OhosDialogKind::SaveFile => {
                    let suggested = req.file_name.unwrap_or_else(|| "Untitled.pdf".into());
                    let options = with_dir(
                        FileDialogOptions::new("save-file").filters(dialog_filters(&req.filters)).suggested_name(suggested),
                        req.directory.as_deref(),
                    );
                    let response = app.show_file_dialog(options).await.map_err(|e| e.to_string())?;
                    let Some(uri) = response.files.first() else { return Ok(Vec::new()) };
                    let path = winit::platform::ohos::drag_drop::file_path(uri).ok_or("Cannot resolve selected save URI")?;
                    if grants.authorize_save(&app, &path).await? { Ok(vec![path]) } else { Ok(Vec::new()) }
                }
            }
        }
        .await;
        match result {
            Ok(paths) => {
                let _ = tx.send(paths);
            }
            Err(e) => {
                log::warn!("Pagrune OpenHarmony file dialog failed: {e}");
                let _ = tx.send(Vec::new());
            }
        }
    });
    rx.recv().unwrap_or_default()
}

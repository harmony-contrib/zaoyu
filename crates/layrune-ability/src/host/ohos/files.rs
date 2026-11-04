//! OpenHarmony file picker, save filters, and persistent URI grants (`file-grants.json`).
//!
//! Atomic replacement needs the containing directory: a single-file grant cannot create a sibling
//! temporary file.

use craft_dialogs::{Answer, FileDialog, Kind, Message, MessageButtons, MessageLevel, MessageResult, Request};
use designcraft_ui_egui::OhosDialogFn;
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
        if !self.directory.join("file-grants.json").exists() {
            return;
        }
        let bytes = match read_bounded(&self.directory.join("file-grants.json"), 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::warn!("Could not restore file grants: {error}");
                return;
            }
        };
        let saved: BTreeMap<String, u32> = match serde_json::from_slice(&bytes) {
            Ok(saved) => saved,
            Err(error) => {
                log::warn!("Could not read saved file grants: {error}");
                return;
            }
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
        designcraft_engine::atomic::write_atomic(&self.directory.join("file-grants.json"), &bytes).map_err(|e| e.to_string())?;
        Ok(paths)
    }

    pub fn can_save(&self, path: &Path) -> bool {
        path.parent().and_then(|parent| parent.canonicalize().ok()).is_some_and(|parent| {
            self.writable
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .any(|dir| dir.canonicalize().is_ok_and(|dir| parent.starts_with(dir)))
        })
    }

    /// Some launch Wants carry a transient read grant which cannot be persisted.
    pub fn resolve_want(&self, uri: &str) -> Option<PathBuf> {
        match self.persist(&[uri.to_owned()], false) {
            Ok(paths) => paths.into_iter().next(),
            Err(error) => {
                log::warn!("Could not retain the launch file grant; select it again next time: {error}");
                winit::platform::ohos::drag_drop::file_path(uri)
            }
        }
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
}

pub(super) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if u64::try_from(bytes.len()).map_err(|e| e.to_string())? > limit {
        return Err(format!("{} exceeds the {limit} byte limit", path.display()));
    }
    Ok(bytes)
}

async fn pick(grants: &Grants, app: &OpenHarmonyApp, dialog: FileDialog) -> Result<Answer, String> {
    let mut options = match dialog.kind {
        Kind::Open { multiple } => FileDialogOptions::new("open-file").allow_many(multiple),
        Kind::Folder => FileDialogOptions::new("open-folder"),
        Kind::Save => FileDialogOptions::new("save-file").suggested_name(dialog.file_name.unwrap_or_else(|| "Untitled.designcraft".into())),
    };
    options =
        options.filters(dialog.filters.into_iter().map(|f| FileDialogFilter::new().name(f.name).pattern(f.extensions.join(";"))).collect());
    if let Some(uri) = dialog.directory.as_deref().and_then(winit::platform::ohos::drag_drop::file_uri) {
        options = options.default_location(uri);
    }
    let response = app.show_file_dialog(options).await.map_err(|e| e.to_string())?;
    if response.files.is_empty() {
        return Ok(Answer::Cancelled);
    }
    let paths = match dialog.kind {
        Kind::Open { .. } => grants.persist(&response.files, false)?,
        Kind::Folder => grants.persist(&response.files, true)?,
        Kind::Save => {
            let uri = response.files.first().ok_or("The save picker returned no file")?;
            let path = winit::platform::ohos::drag_drop::file_path(uri).ok_or("Cannot resolve selected save URI")?;
            if !grants.authorize_save(app, &path).await? {
                return Ok(Answer::Cancelled);
            }
            vec![path]
        }
    };
    Ok(Answer::Paths(paths))
}

/// Replacement confirmations use the same asynchronous Ability window bridge as the picker.
async fn message(app: &OpenHarmonyApp, message: Message) -> Result<Answer, String> {
    use winit::platform::ohos::plugins::window::{WindowClient, WindowPromptRequest};
    let results = match message.buttons {
        MessageButtons::Ok => vec![MessageResult::Ok],
        MessageButtons::OkCancel => vec![MessageResult::Ok, MessageResult::Cancel],
        MessageButtons::YesNo => vec![MessageResult::Yes, MessageResult::No],
        MessageButtons::YesNoCancel => vec![MessageResult::Yes, MessageResult::No, MessageResult::Cancel],
        MessageButtons::OkCustom(ok) => vec![MessageResult::Custom(ok)],
        MessageButtons::OkCancelCustom(ok, cancel) => vec![MessageResult::Custom(ok), MessageResult::Custom(cancel)],
        MessageButtons::YesNoCancelCustom(yes, no, cancel) => {
            vec![MessageResult::Custom(yes), MessageResult::Custom(no), MessageResult::Custom(cancel)]
        }
    };
    let buttons = results
        .iter()
        .map(|result| match result {
            MessageResult::Ok => "OK".into(),
            MessageResult::Cancel => "Cancel".into(),
            MessageResult::Yes => "Yes".into(),
            MessageResult::No => "No".into(),
            MessageResult::Custom(label) => label.clone(),
        })
        .collect();
    let request = WindowPromptRequest {
        window_id: 0,
        message: message.title,
        detail: Some(message.description),
        level: match message.level {
            MessageLevel::Info => 0,
            MessageLevel::Warning => 1,
            MessageLevel::Error => 2,
        },
        buttons,
        cancel_index: Some(u32::try_from(results.len().saturating_sub(1)).map_err(|e| e.to_string())?),
        anchor: None,
        appearance: None,
    };
    let index = WindowClient::new(app).map_err(|e| e.to_string())?.show_prompt(request).await.map_err(|e| e.to_string())?;
    let result = results.get(usize::try_from(index).map_err(|e| e.to_string())?).ok_or("Invalid confirmation answer")?;
    Ok(Answer::Message(result.clone()))
}

pub(super) fn dialog_service(ability: OpenHarmonyApp, grants: Arc<Grants>, ctx: egui::Context) -> OhosDialogFn {
    Box::new(move |request, reply| {
        let (app, grants, ctx) = (ability.clone(), grants.clone(), ctx.clone());
        task::spawn(async move {
            let result = match request {
                Request::File(dialog) => pick(&grants, &app, dialog).await,
                Request::Message(prompt) => message(&app, prompt).await,
            };
            let answer = result.unwrap_or_else(|error| {
                log::error!("OpenHarmony file dialog failed: {error}");
                Answer::Unavailable(error)
            });
            let _ = reply.send(answer);
            ctx.request_repaint();
        });
    })
}

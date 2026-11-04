//! Preferences live in the Ability sandbox, beside the UI persistence and recovery folders.

use designcraft_ui_egui::DesignApp;
use std::path::{Path, PathBuf};

pub(super) struct Preferences {
    directory: PathBuf,
    keep_ui: bool,
    keep_engine: bool,
}

impl Preferences {
    pub(super) fn load(directory: &Path, app: &mut DesignApp) -> Self {
        let mut preferences = Self { directory: directory.to_owned(), keep_ui: false, keep_engine: false };
        for (name, engine) in [("ui.json", false), ("prefs.json", true)] {
            let path = directory.join(name);
            if !path.exists() {
                continue;
            }
            let loaded = super::files::read_bounded(&path, 4 << 20).and_then(|bytes| {
                if engine {
                    app.session.prefs = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    Ok(true)
                } else {
                    let read = designcraft_ui_egui::UiState::read_preferences(&bytes).map_err(|e| e.to_string())?;
                    app.ui = read.ui;
                    Ok(read.intact)
                }
            });
            if !matches!(loaded, Ok(true)) {
                log::warn!("Could not fully read {}; preserving it unchanged: {:?}", path.display(), loaded);
                if engine {
                    preferences.keep_engine = true;
                } else {
                    preferences.keep_ui = true;
                }
            }
        }
        app.sync_keymap();
        preferences
    }

    pub(super) fn save(&self, app: &DesignApp) -> Result<(), String> {
        if !self.keep_ui {
            let bytes = serde_json::to_vec_pretty(&app.ui).map_err(|e| e.to_string())?;
            designcraft_engine::atomic::write_atomic(&self.directory.join("ui.json"), &bytes).map_err(|e| e.to_string())?;
        }
        if !self.keep_engine {
            let bytes = serde_json::to_vec_pretty(&app.session.prefs).map_err(|e| e.to_string())?;
            designcraft_engine::atomic::write_atomic(&self.directory.join("prefs.json"), &bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

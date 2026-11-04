//! HarmonyOS lifecycle and services for the pinned SoundCraft editor.
#![forbid(unsafe_code)]

mod logging;
mod preferences;

use openharmony_ability::OpenHarmonyApp;
use soundcraft_ui_egui::{Branding, Services, SoundApp, UiState, dialogs, docking};
use std::{path::PathBuf, sync::Arc};

pub(super) fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    logging::install();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    for name in ["Autosave", "Presets"] {
        if let Err(error) = std::fs::create_dir_all(directory.join(name)) {
            log::warn!("could not create {name} directory: {error}");
        }
    }

    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability),
        viewport: egui::ViewportBuilder::default()
            .with_title("Sonune")
            .with_app_id("com.richerfu.sonune")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 600.0]),
        persistence_path: Some(directory.join("ui.ron")),
        system_font_fallback: false,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.flags =
            setup.instance_descriptor.flags.difference(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL).with_env();
        let device_descriptor = Arc::clone(&setup.device_descriptor);
        setup.device_descriptor = Arc::new(move |adapter| {
            let mut descriptor = device_descriptor(adapter);
            let supported = adapter.limits();
            descriptor.required_limits = descriptor.required_limits.or_worse_values_from(&supported);
            descriptor.required_limits.max_texture_dimension_2d = supported.max_texture_dimension_2d;
            descriptor
        });
        if std::env::var_os("WGPU_BACKEND").is_none() {
            setup.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
    }
    eframe::run_native(
        "Sonune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let services = Services {
                branding: Some(Branding {
                    name: "Sonune",
                    icon_png: include_bytes!("../../../branding/sonune-128.png"),
                    attribution: "Sonune · 造隅。Based on SoundCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
                }),
                pick_open: None,
                pick_save: None,
                pick_folder: None,
            };
            let mut app = SoundApp::new(soundcraft_engine::Engine::default(), None, services);
            app.autosave_dir = Some(directory.join("Autosave"));
            app.preset_dir = Some(directory.join("Presets"));
            app.leftovers = soundcraft_engine::autosave::leftovers(&directory.join("Autosave"));
            load_preferences(&mut app, &directory);
            log::info!("Sonune {} OpenHarmony workbench ready", env!("CARGO_PKG_VERSION"));
            Ok(Box::new(App { app, directory }))
        }),
    )
    .map_err(|error| error.to_string())
}

fn load_preferences(app: &mut SoundApp, directory: &std::path::Path) {
    let Some(bytes) = preferences::read(directory) else { return };
    let restored = (|| -> Result<UiState, String> {
        let mut saved = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        docking::reset_old_layouts(&mut saved);
        let mut ui: UiState = serde_json::from_value(saved).map_err(|error| error.to_string())?;
        docking::validate(&ui.docking)?;
        docking::validate_mix(&ui.mix_docking)?;
        ui.plugin_windows.clear();
        ui.status.clear();
        if !ui.ui_scale.is_finite() || ui.ui_scale <= 0.0 {
            ui.ui_scale = 1.0;
        }
        Ok(ui)
    })();
    match restored {
        Ok(ui) => {
            app.extra.ui_scale = ui.ui_scale;
            app.ui = ui;
        }
        Err(error) => log::warn!("could not restore ui.json: {error}"),
    }
}

struct App {
    app: SoundApp,
    directory: PathBuf,
}

impl App {
    fn save_preferences(&self) {
        let mut ui = self.app.ui.clone();
        ui.plugin_windows.clear();
        ui.status.clear();
        ui.ui_scale = self.app.extra.ui_scale;
        ui.docking_version = docking::LAYOUT_VERSION;
        match serde_json::to_vec_pretty(&ui) {
            Ok(bytes) => {
                if let Err(error) = preferences::write(&self.directory, &bytes) {
                    log::warn!("could not save ui.json: {error}");
                }
            }
            Err(error) => log::warn!("could not serialize preferences: {error}"),
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.guarded(|app| app.logic(ctx));
        if self.app.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if ctx.input(|input| input.viewport().close_requested())
            && dialogs::ask_to_save(&mut self.app, "app.quit", &serde_json::json!({}))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.app.take_recent_changed() {
            self.save_preferences();
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.guarded(|app| app.ui(ui));
    }

    fn on_exit(&mut self) {
        self.app.close_autosave();
        self.save_preferences();
        self.app.player = None;
        soundcraft_engine::shutdown_plugin_hosts();
        log::logger().flush();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_preferences();
    }
}

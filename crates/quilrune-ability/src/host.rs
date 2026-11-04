//! HarmonyOS lifecycle and services for the pinned WordCraft editor.
#![forbid(unsafe_code)]

mod logging;
mod preferences;

use openharmony_ability::OpenHarmonyApp;
use std::{path::PathBuf, sync::Arc};
use wordcraft_ui_egui::{Branding, Services, UiState, WordApp};

pub(super) fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    logging::install();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    if let Err(error) = std::fs::create_dir_all(directory.join("Templates")) {
        log::warn!("could not create templates directory: {error}");
    }
    let (loaded, skipped) = wordcraft_fonts::FontDb::global().load_user_fonts(&directory.join("fonts"));
    log::info!("user fonts: {} loaded, {} skipped", loaded.len(), skipped.len());

    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability),
        viewport: egui::ViewportBuilder::default()
            .with_title("Quilrune")
            .with_app_id("com.richerfu.quilrune")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 480.0]),
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
        "Quilrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let services = Services {
                branding: Some(Branding {
                    name: "Quilrune",
                    icon_png: include_bytes!("../../../branding/quilrune-128.png"),
                    attribution: "Quilrune · 造隅。Based on WordCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
                }),
                templates_dir: Some(directory.join("Templates")),
                ..Default::default()
            };
            let mut session = wordcraft_engine::Session::new(wordcraft_doc::Document::new());
            let ctx = cc.egui_ctx.clone();
            if let Err(error) = session.enable_recovery(directory.join("recovery"), Arc::new(move || ctx.request_repaint())) {
                log::warn!("local recovery unavailable: {error}");
            }
            let mut app = WordApp::new(session, services);
            app.ui.theme = wordcraft_ui_egui::theme::Appearance::Dark;
            if let Some(bytes) = preferences::read(&directory) {
                if let Some(ui) = UiState::from_json(&bytes) {
                    app.apply_prefs(ui);
                } else {
                    log::warn!("could not restore ui.json: invalid preferences");
                }
            }
            app.start_screen();
            log::info!("Quilrune {} OpenHarmony workbench ready", env!("CARGO_PKG_VERSION"));
            Ok(Box::new(App { app, directory }))
        }),
    )
    .map_err(|error| error.to_string())
}

struct App {
    app: WordApp,
    directory: PathBuf,
}

impl App {
    fn save_preferences(&self) {
        match serde_json::to_vec_pretty(&self.app.prefs()) {
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
        self.app.logic(ctx);
        if ctx.input(|input| input.viewport().close_requested()) && !self.app.close_requested() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        } else if self.app.quit_requested && !self.app.printing.busy() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }

    fn on_exit(&mut self) {
        self.app.session.finish_recovery();
        self.save_preferences();
        log::logger().flush();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_preferences();
    }
}

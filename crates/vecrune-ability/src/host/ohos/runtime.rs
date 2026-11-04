//! Vecrune startup and editor lifecycle.

use super::{files, logging, screen, services};

use openharmony_ability::OpenHarmonyApp;
use std::{
    path::PathBuf,
    sync::{Arc, PoisonError, mpsc::Receiver},
    time::{Duration, Instant},
};
use vectorcraft_engine::Session;
use vectorcraft_ui_egui::{UiState, VectorcraftApp};
use winit::platform::ohos::events::PlatformEvent;

pub(in crate::host) fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    let logger = logging::install_logger();
    logging::install_panic_hook();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    if let Some(logger) = logger {
        match logger.attach_dir(&directory.join("logs")) {
            Ok(path) => log::info!("Vecrune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display()),
            Err(error) => log::warn!("No log file: {error}"),
        }
    }

    let catalog = craft_font_catalog::Catalog::global();
    catalog.set_app_dirs(vec![PathBuf::from("/system/fonts")]);
    catalog.scan_in_background();
    for priority in ["/system/fonts/HarmonyOS_Sans_SC.ttf", "/system/fonts/HarmonyOS_Sans_TC.ttf", "/system/fonts/HarmonyOS_Sans.ttf"] {
        if let Ok(bytes) = std::fs::read(priority)
            && bytes.len() <= 40 << 20
        {
            vectorcraft_text::FontDb::global().add_font(bytes);
        }
    }
    vectorcraft_text::FontDb::global().scan_in_background();
    let grants = Arc::new(files::Grants::new(directory.clone()));
    let restore = grants.clone();
    let restored =
        std::thread::Builder::new().name("vectorcraft-grants".into()).spawn(move || restore.restore()).map_err(|e| e.to_string())?;

    // Subscribe before eframe creates its first surface so the launch Want is retained.
    let events = winit::platform::ohos::events::subscribe();
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        persistence_path: Some(directory.join("ui.ron")),
        viewport: egui::ViewportBuilder::default()
            .with_title("Vecrune")
            .with_app_id("com.richerfu.vecrune")
            .with_inner_size([1280.0, 820.0])
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        system_font_fallback: false,
        ..Default::default()
    };
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.flags =
            setup.instance_descriptor.flags.difference(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL).with_env();
        // PdfCraft also requests the supported surface size for high-density tablet windows.
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
    // Activating saved policies must finish before recent files or a launch Want are opened.
    restored.join().map_err(|_| "Could not restore saved file permissions")?;
    vectorcraft_engine::guard::catch_panic(|| {
        eframe::run_native(
            "Vecrune",
            options,
            Box::new(move |cc| {
                cc.egui_ctx.set_embed_viewports(true);
                VectorcraftApp::install_ime_reset_hooks(&cc.egui_ctx);
                let errors = files::Errors::default();
                let mut app = VectorcraftApp::new(Session::new(), services::build(&ability, &grants, &directory, &cc.egui_ctx, &errors));
                let (sampler, picker) = screen::service(cc.winit_window().cloned(), cc.egui_ctx.clone());
                app.services.screen_color = Some(sampler);
                #[cfg(feature = "ohos-control")]
                {
                    app = app.with_control(crate::host::control_server::start(7878, cc.egui_ctx.clone()));
                    vectorcraft_format::write_atomic(&directory.join("control.json"), b"{\"port\":7878}")?;
                }
                let mut host = Host {
                    app,
                    directory,
                    events,
                    grants,
                    picker,
                    errors,
                    saved_ui: Vec::new(),
                    saved_prefs: Vec::new(),
                    last_save: Instant::now(),
                    keep_ui: false,
                };
                host.restore();
                log::info!("Vecrune OpenHarmony editor ready");
                Ok(Box::new(host))
            }),
        )
    })
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

struct Host {
    app: VectorcraftApp,
    directory: PathBuf,
    events: Receiver<PlatformEvent>,
    grants: Arc<files::Grants>,
    picker: screen::Picker,
    errors: files::Errors,
    saved_ui: Vec<u8>,
    saved_prefs: Vec<u8>,
    last_save: Instant,
    keep_ui: bool,
}

impl Host {
    fn restore(&mut self) {
        let path = self.directory.join("ui.json");
        if let Ok(bytes) = files::read_limited(&path, 16 * 1024 * 1024) {
            let read = UiState::read_preferences(&bytes);
            if read.as_ref().is_none_or(|read| !read.intact || read.converted) {
                // Keep an unreadable/newer layout before writing this version's preferences.
                if let Err(error) = vectorcraft_format::write_atomic(&self.directory.join("ui.before-migration.json"), &bytes) {
                    self.keep_ui = true;
                    log::warn!("Could not keep the previous UI preferences: {error}");
                }
            }
            if let Some(read) = read {
                self.app.ui = read.ui.sanitized();
            }
        }
        if let Ok(bytes) = files::read_limited(&self.directory.join("prefs.json"), 16 * 1024 * 1024) {
            match serde_json::from_slice(&bytes) {
                Ok(prefs) => self.app.ui.engine_prefs = prefs,
                Err(error) => log::warn!("Could not read engine preferences: {error}"),
            }
        }
        vectorcraft_ui_egui::prefs_dialog::restore(&mut self.app);
    }

    fn save(&mut self) {
        let mut ui = self.app.ui.clone();
        ui.engine_prefs = self.app.session.prefs.to_json();
        for (name, value, saved, keep) in [
            ("prefs.json", serde_json::to_vec_pretty(&ui.engine_prefs), &mut self.saved_prefs, false),
            ("ui.json", serde_json::to_vec_pretty(&ui), &mut self.saved_ui, self.keep_ui),
        ] {
            if keep {
                continue;
            }
            match value {
                Ok(bytes) if bytes != *saved => match vectorcraft_format::write_atomic(&self.directory.join(name), &bytes) {
                    Ok(()) => *saved = bytes,
                    Err(error) => log::warn!("Could not save {name}: {error}"),
                },
                Err(error) => log::warn!("Could not encode {name}: {error}"),
                _ => {}
            }
        }
        self.last_save = Instant::now();
    }
}

impl eframe::App for Host {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The generic inbox opens browser files without a path. Picked OHOS files retain their
        // grant and native path so Save replaces the original and Recent Files can reopen it.
        let arrived = self
            .app
            .services
            .inbox
            .as_ref()
            .map(|inbox| std::mem::take(&mut *inbox.lock().unwrap_or_else(PoisonError::into_inner)))
            .unwrap_or_default();
        for (path, bytes) in arrived {
            if let Err(error) = vectorcraft_ui_egui::io::open_bytes(&mut self.app, &path, &bytes, Some(path.clone())) {
                vectorcraft_ui_egui::io::report_open_error(&mut self.app, &path, &error);
            }
        }
        if !self.app.file_dialog_open() {
            for event in self.events.try_iter() {
                if let PlatformEvent::Want { uri, .. } = event
                    && !uri.is_empty()
                    && let Some(path) = files::resolve_want(&self.grants, &uri)
                    && let Err(error) = vectorcraft_ui_egui::io::open_reporting(&mut self.app, &path.to_string_lossy())
                {
                    files::report(&self.errors, error);
                }
            }
        }
        for error in std::mem::take(&mut *self.errors.lock().unwrap_or_else(PoisonError::into_inner)) {
            vectorcraft_ui_egui::dialogs::confirm::tell(&mut self.app, "OpenHarmony service failed", &error);
            self.app.status(error);
        }
        self.app.tick(ctx);
        self.app.finish_ime_logic(ctx);
        if self.app.ui.status == "quit" {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
        self.picker.show(ui.ctx());
        if self.app.take_prefs_changed() || self.last_save.elapsed() >= Duration::from_secs(2) {
            self.save();
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }

    fn on_exit(&mut self) {
        self.save();
        vectorcraft_ui_egui::background::wait_all(&mut self.app);
        log::logger().flush();
    }
}

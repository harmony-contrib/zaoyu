//! OpenHarmony Ability lifecycle host, isolated from the desktop executable.

use super::{clipboard, files, logging, persistence};

use effectcraft_ui_egui::EffectcraftApp;
use openharmony_ability::{ColorMode, OpenHarmonyApp};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Duration,
};
use winit::platform::ohos::events::PlatformEvent;

pub fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    logging::install()?;
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| format!("Cannot create preferences directory {}: {error}", directory.display()))?;
    let directory = directory.canonicalize().map_err(|error| format!("Cannot resolve preferences directory: {error}"))?;
    logging::attach(&directory);
    log::info!("Motrune {} starting on OpenHarmony", env!("CARGO_PKG_VERSION"));

    let catalog = craft_font_catalog::Catalog::global();
    catalog.set_app_dirs(vec![PathBuf::from("/system/fonts")]);
    catalog.scan_in_background();
    let grants = Arc::new(files::Grants::new(directory.clone()));
    let (ready_tx, ready) = mpsc::channel();
    let startup_grants = Arc::clone(&grants);
    std::thread::Builder::new()
        .name("motrune-ohos-startup".into())
        .spawn(move || {
            let result = effectcraft_engine::guard::guarded("OpenHarmony font and file grant loading", || {
                startup_grants.restore();
                // This version of Motrune has its own font database. Register the same
                // directory there as well as in the suite catalogue, for text tools and imports.
                let fonts = &effectcraft_engine::text::fonts::DirectorySource { dirs: vec![PathBuf::from("/system/fonts")] };
                effectcraft_engine::text::fonts::add_source(fonts);
                effectcraft_engine::text::fonts::scan_system();
                Ok(())
            })
            .map_err(|error| error.to_string());
            let _ = ready_tx.send(result);
        })
        .map_err(|error| format!("Cannot start font and file grant loader: {error}"))?;

    // Subscribe before creating the first rendering surface so launch Wants are retained.
    let events = winit::platform::ohos::events::subscribe();
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        viewport: egui::ViewportBuilder::default()
            .with_title("Motrune")
            .with_app_id("com.richerfu.motrune")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true),
        persistence_path: Some(directory.join("ui.ron")),
        system_font_fallback: false,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.flags =
            setup.instance_descriptor.flags.difference(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL).with_env();
        let descriptor = Arc::clone(&setup.device_descriptor);
        setup.device_descriptor = Arc::new(move |adapter| {
            let mut descriptor = descriptor(adapter);
            descriptor.required_limits = descriptor.required_limits.or_worse_values_from(&adapter.limits());
            descriptor.required_limits.max_texture_dimension_2d = adapter.limits().max_texture_dimension_2d.min(16384);
            descriptor
        });
        if std::env::var_os("WGPU_BACKEND").is_none() {
            setup.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
    }
    eframe::run_native(
        "Motrune",
        options,
        Box::new(move |cc| {
            EffectcraftApp::install_ime_reset_hooks(&cc.egui_ctx);
            cc.egui_ctx.set_embed_viewports(true);
            let gpu_failures = effectcraft_ui_egui::gpu_failure::GpuFailureBridge::new(&cc.egui_ctx);
            if let Some(state) = &cc.wgpu_render_state {
                log::info!("graphics: {:?}", state.adapter.get_info());
                let errors = gpu_failures.clone();
                state.device.on_uncaptured_error(Arc::new(move |error| {
                    let message = format!("uncaptured GPU error: {error}");
                    if errors.report(&message, false) {
                        log::error!("{message}");
                    }
                }));
                let lost = gpu_failures.clone();
                state.device.set_device_lost_callback(move |reason, message| {
                    lost.report(&format!("GPU device lost ({reason:?}): {message}"), true);
                });
            }
            let mut session = effectcraft_host::session();
            session.config = Some(Arc::new(persistence::Config::new(directory.clone())));
            session.services = Arc::new(files::GrantedFiles(Arc::clone(&grants)));
            session.models_dir = Some(directory.join("models"));
            session.autosave.background = true;
            session.check_footage_on_open = true;
            // Keep recovery independent of an external file's read-only URI grant.
            session.autosave_folder_override = Some(directory.join("Auto-Save"));
            session.load_settings();
            if session.prefs.disk.disk_cache_folder.is_empty() {
                session.prefs.disk.disk_cache_folder = directory.join("Cache").to_string_lossy().into_owned();
            }
            if session.prefs.disk.media_cache_folder.is_empty() {
                session.prefs.disk.media_cache_folder = directory.join("Media Cache").to_string_lossy().into_owned();
            }
            if session.prefs.disk.conformed_media_folder.is_empty() {
                session.prefs.disk.conformed_media_folder = directory.join("Conformed Audio").to_string_lossy().into_owned();
            }
            session.prefs_changed();
            let recovery = session.begin_recovery();
            let mut app = EffectcraftApp::new(session);
            app.hooks.branding = Some(effectcraft_ui_egui::Branding {
                    name: "Motrune",
                    icon_png: include_bytes!("../../../../../branding/motrune-128.png"),
                    attribution: "Motrune · 造隅。Based on EffectCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
                });
            app.set_gpu_failure_bridge(gpu_failures);
            app.ui.start_screen = app.session.prefs.startup.show_home_on_launch && !cfg!(feature = "ohos-control");
            if let Some(recovery) = recovery {
                app.offer_recovery(recovery);
            }
            let file_app = ability.clone();
            let file_grants = Arc::clone(&grants);
            let file_ctx = cc.egui_ctx.clone();
            app.hooks.file_dialog_async =
                Some(Box::new(move |request, done| files::dialog(file_app.clone(), Arc::clone(&file_grants), file_ctx.clone(), request, done)));
            app.hooks.clipboard_text = Some(clipboard::text_service(ability.clone()));
            app.hooks.system_theme = Some(Box::new(move |_| match ability.config().color_mode {
                ColorMode::Light => Some(egui::Theme::Light),
                ColorMode::Dark => Some(egui::Theme::Dark),
                ColorMode::NoSet => None,
            }));
            // cpal 0.16 does not implement OHOS. The audio hooks stay unset, so preview
            // remains usable without trying to initialize a Linux/ALSA device.
            #[cfg(feature = "ohos-control")]
            {
                let rx = crate::host::control_server::start(7878, cc.egui_ctx.clone());
                app = app.with_control(rx);
                effectcraft_engine::config::atomic_write(&directory.join("control.json"), br#"{"port":7878}"#)
                    .map_err(|error| format!("Cannot write control endpoint: {error}"))?;
            }
            log::info!("Motrune OpenHarmony workbench ready");
            Ok(Box::new(Host { app, events, grants, ready: Some(ready), startup_error: None }))
        }),
    )
    .map_err(|error| error.to_string())
}

struct Host {
    app: EffectcraftApp,
    events: mpsc::Receiver<PlatformEvent>,
    grants: Arc<files::Grants>,
    ready: Option<mpsc::Receiver<Result<(), String>>>,
    startup_error: Option<String>,
}

impl Host {
    fn loading(&mut self, ctx: &egui::Context) -> bool {
        if let Some(ready) = &self.ready {
            match ready.try_recv() {
                Ok(Ok(())) => self.ready = None,
                Ok(Err(error)) => {
                    self.startup_error = Some(error);
                    self.ready = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.startup_error = Some("Startup loading stopped. Restart Motrune to retry.".into());
                    self.ready = None;
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
            }
        }
        self.ready.is_some() || self.startup_error.is_some()
    }
}

impl eframe::App for Host {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.loading(ctx) {
            self.app.logic(ctx, frame);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.loading(ui.ctx()) {
            paint_startup_splash(
                ui,
                include_bytes!("../../../../../branding/motrune-128.png"),
                "Motrune",
                "EffectCraft · 造隅特效",
                egui::Color32::from_rgb(0xE0, 0x36, 0x8F),
                self.startup_error.as_deref().unwrap_or("Loading installed fonts and file access…"),
                self.startup_error.is_some(),
            );
        } else {
            if let Some(error) = ui.ctx().data_mut(|data| data.remove_temp::<String>(egui::Id::unique("motrune-ohos-file-error"))) {
                self.app.ui.status = error;
            }
            self.app.ui(ui, frame);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.ready.is_none() && self.startup_error.is_none() && self.app.is_ready() {
            for event in self.events.try_iter() {
                if let PlatformEvent::Want { uri, .. } = event
                    && !uri.is_empty()
                    && let Some(path) = self.grants.resolve_want(&uri)
                {
                    input.dropped_files.push(Arc::new(OpenedFile(path)));
                    self.app.ui.start_screen = false;
                }
            }
        }
        self.app.raw_input_hook(ctx, input);
    }

    fn on_exit(&mut self) {
        self.app.session.save_prefs();
        self.app.on_exit();
        log::logger().flush();
    }
}

#[derive(Debug)]
struct OpenedFile(PathBuf);

impl egui::DroppedFile for OpenedFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        effectcraft_engine::Services::read_file(&effectcraft_engine::FsServices, &self.0.to_string_lossy())
            .map_err(|error| error.to_string())
    }
}

fn paint_startup_splash(
    ui: &mut egui::Ui,
    icon_png: &[u8],
    name: &str,
    subtitle: &str,
    accent: egui::Color32,
    status: &str,
    is_error: bool,
) {
    let bg = egui::Color32::from_rgb(0x12, 0x13, 0x16);
    let card_bg = egui::Color32::from_rgb(0x18, 0x1A, 0x1F);
    let border = egui::Color32::from_rgb(0x2A, 0x2D, 0x35);
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg)).show(ui, |ui| {
        let rect = ui.max_rect();
        let card_size = egui::vec2(360.0, 236.0);
        let card_rect = egui::Rect::from_center_size(rect.center(), card_size);
        let painter = ui.painter();
        painter.rect_filled(card_rect, 16.0, card_bg);
        painter.rect_stroke(card_rect, 16.0, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
        let bar_rect = egui::Rect::from_min_size(card_rect.min + egui::vec2(16.0, 0.0), egui::vec2(card_size.x - 32.0, 3.0));
        painter.rect_filled(bar_rect, 2.0, accent);

        let key = egui::Id::unique(("zaoyu_splash_icon", name));
        let texture = ui.ctx().data(|data| data.get_temp::<egui::TextureHandle>(key)).or_else(|| {
            let icon = eframe::icon_data::from_png_bytes(icon_png).ok()?;
            let size = [usize::try_from(icon.width).ok()?, usize::try_from(icon.height).ok()?];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &icon.rgba);
            let handle = ui.ctx().load_texture(format!("{name}-splash-icon"), image, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|data| data.insert_temp(key, handle.clone()));
            Some(handle)
        });

        let inner = card_rect.shrink2(egui::vec2(24.0, 24.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Center)), |ui| {
            ui.add_space(6.0);
            if let Some(texture) = texture {
                let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(texture.id(), icon_rect, uv, egui::Color32::WHITE);
            }
            ui.add_space(12.0);
            ui.label(egui::RichText::new(name).size(22.0).strong().color(egui::Color32::WHITE));
            ui.add_space(2.0);
            ui.label(egui::RichText::new(subtitle).size(12.5).color(accent));
            ui.add_space(18.0);
            if is_error {
                ui.label(egui::RichText::new(status).size(12.5).color(egui::Color32::from_rgb(0xFF, 0x7A, 0x85)));
            } else {
                ui.horizontal(|ui| {
                    let width = ui.available_width();
                    ui.add_space(((width - 240.0) * 0.5).max(0.0));
                    ui.spinner();
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(status).size(12.5).color(egui::Color32::from_rgb(0x9A, 0x9D, 0xA6)));
                });
            }
        });
    });
}

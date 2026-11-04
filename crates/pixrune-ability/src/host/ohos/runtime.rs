//! Ability lifecycle, storage and native rendering for Pixrune.

use super::{display, files, logging, services};
use openharmony_ability::OpenHarmonyApp;
use photocraft_engine::Session;
use photocraft_ui_egui::PhotocraftApp;
use std::{path::PathBuf, sync::Arc, time::Duration};

pub(crate) fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    let logger = logging::install_logger();
    logging::install_panic_hook();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    if let Some(logger) = logger {
        match logger.attach_dir(&directory.join("logs")) {
            Ok(path) => log::info!("Pixrune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display()),
            Err(error) => log::warn!("No log file: {error}"),
        }
    }
    let monitor = display::detect_async();
    let grants = Arc::new(files::Grants::new(directory.clone()));
    let restore = Arc::clone(&grants);
    let (fonts_tx, fonts_ready) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("pixrune-fonts".into())
        .spawn(move || {
            restore.restore();
            let result = std::panic::catch_unwind(photocraft_ui_egui::type_tool::families)
                .map(|_| ())
                .map_err(|_| "Unable to load installed fonts. Restart Pixrune to retry.".to_owned());
            let _ = fonts_tx.send(result);
        })
        .map_err(|error| error.to_string())?;
    let events = winit::platform::ohos::events::subscribe();
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        viewport: egui::ViewportBuilder::default()
            .with_title("Pixrune")
            .with_app_id("com.richerfu.pixrune")
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
    #[cfg(feature = "ohos-control")]
    let control = {
        let root = directory.join("validation");
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let workspace = photocraft_automation::AuthorizedWorkspace::new(Some(&root), Some(&root)).map_err(|error| error.to_string())?;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
        let port = listener.local_addr().map_err(|error| error.to_string())?.port();
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let token = photocraft_automation::security::generate_token().map_err(|error| error.to_string())?;
        let control_file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(directory.join("control.json"))
            .map_err(|error| error.to_string())?;
        control_file.set_permissions(std::fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?;
        serde_json::to_writer(control_file, &serde_json::json!({"port": port, "token": token})).map_err(|error| error.to_string())?;
        Some((listener, workspace, token))
    };
    #[cfg(not(feature = "ohos-control"))]
    let control: Option<(std::net::TcpListener, photocraft_automation::AuthorizedWorkspace, String)> = None;
    eframe::run_native(
        "Pixrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            PhotocraftApp::install_type_reset_hooks(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let render_state = cc.wgpu_render_state.clone();
            let screen_pick = display::screen_color_service(cc);
            let create = move || {
                let cursor_pos: photocraft_ui_egui::CursorPosFn = Box::new(|ctx| {
                    if let Some((x, y)) = winit::platform::ohos::drag_drop::last_position() {
                        let scale = ctx.pixels_per_point();
                        if scale.is_finite() && scale > 0.0 {
                            return Some(egui::pos2(x as f32 / scale, y as f32 / scale));
                        }
                    }
                    ctx.input(|input| input.pointer.latest_pos())
                });
                let services = services::build(
                    &ability,
                    &grants,
                    &ctx,
                    events,
                    Some(cursor_pos),
                    screen_pick,
                    control.as_ref().map(|(_, workspace, _)| workspace.clone()),
                );
                let mut app = PhotocraftApp::new(Session::new(), services);
                app.background_jobs = true;
                if let Some(receiver) = monitor {
                    app.services.read_displays = Some(Arc::new(display::detect_async));
                    photocraft_ui_egui::monitor_status::pending(&mut app, receiver);
                }
                if let Some(state) = render_state {
                    app.perf.gpu_info.set_adapter(&state.adapter.get_info());
                    app.perf.gpu_info.set_available(&state.available_adapters);
                    photocraft_ui_egui::texel_guard::install(&ctx, &state.adapter.get_info());
                    if app.session.prefs().performance.effective_rendering_mode() != photocraft_engine::prefs::RenderingMode::Cpu {
                        app.set_wgpu(state);
                    } else {
                        app.watch_window_device(&state.device);
                    }
                }
                #[cfg(feature = "ohos-control")]
                if let Some((listener, _, token)) = control {
                    app = app.with_control(crate::host::control_server::start(listener, token, ctx.clone()));
                }
                log::info!("Pixrune OpenHarmony editor ready");
                app
            };
            Ok(Box::new(StartupApp { fonts_ready, create: Some(Box::new(create)), app: None, error: None }))
        }),
    )
    .map_err(|error| error.to_string())
}

/// Keep the Ability responsive while fonts and persisted file grants are loaded.
struct StartupApp {
    fonts_ready: std::sync::mpsc::Receiver<Result<(), String>>,
    create: Option<Box<dyn FnOnce() -> PhotocraftApp>>,
    app: Option<PhotocraftApp>,
    error: Option<String>,
}

impl eframe::App for StartupApp {
    fn on_exit(&mut self) {
        if let Some(app) = &mut self.app {
            app.on_exit();
        }
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.app.is_none() && self.error.is_none() {
            match self.fonts_ready.try_recv() {
                Ok(Ok(())) => self.app = self.create.take().map(|create| create()),
                Ok(Err(error)) => self.error = Some(error),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Font loading stopped. Restart Pixrune to retry.".into())
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
            }
        }
        if let Some(app) = &mut self.app {
            app.logic(ctx, frame);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.app {
            app.ui(ui, frame);
        } else {
            paint_startup_splash(
                ui,
                include_bytes!("../../../../../branding/pixrune-128.png"),
                "Pixrune",
                "PhotoCraft · 造隅修图",
                egui::Color32::from_rgb(0x2F, 0x7B, 0xF5),
                self.error.as_deref().unwrap_or("Loading installed fonts…"),
                self.error.is_some(),
            );
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Some(app) = &mut self.app {
            app.raw_input_hook(ctx, input);
        }
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
                    ui.add_space(((width - 180.0) * 0.5).max(0.0));
                    ui.spinner();
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(status).size(12.5).color(egui::Color32::from_rgb(0x9A, 0x9D, 0xA6)));
                });
            }
        });
    });
}

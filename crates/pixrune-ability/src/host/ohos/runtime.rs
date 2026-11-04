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
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.4);
                    ui.heading("Pixrune");
                    if let Some(error) = &self.error {
                        ui.label(error);
                    } else {
                        ui.spinner();
                        ui.label("Loading installed fonts…");
                    }
                });
            });
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Some(app) = &mut self.app {
            app.raw_input_hook(ctx, input);
        }
    }
}

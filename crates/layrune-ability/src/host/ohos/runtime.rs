//! Ability lifecycle, sandbox storage and platform services, following PhotoCraft and PdfCraft.

use super::{files, preferences};

use designcraft_engine::Session;
use designcraft_ui_egui::{DesignApp, Services};
use openharmony_ability::OpenHarmonyApp;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock, mpsc},
    time::SystemTime,
};
use winit::platform::ohos::{
    events::PlatformEvent,
    plugins::{clipboard::ClipboardClient, permission::PermissionExt as _},
    task,
};

struct OhosLogger(crate::host::logging::AppLogger);

impl log::Log for OhosLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.0.enabled(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        self.0.log(record);
        let thread = std::thread::current();
        let line = crate::host::logging::format_line(
            &crate::host::logging::timestamp(SystemTime::now()),
            record.level(),
            thread.name().unwrap_or("?"),
            record.target(),
            &record.args().to_string(),
        );
        let message = line.trim_end_matches('\n').replace('\0', "");
        match record.level() {
            log::Level::Error => ohos_hilog_binding::error(message),
            log::Level::Warn => ohos_hilog_binding::warn(message),
            log::Level::Info => ohos_hilog_binding::info(message),
            log::Level::Debug | log::Level::Trace => ohos_hilog_binding::debug(message),
        }
    }

    fn flush(&self) {
        self.0.flush();
    }
}

fn install_logger() -> Option<&'static crate::host::logging::AppLogger> {
    static LOGGER: OnceLock<OhosLogger> = OnceLock::new();
    let spec = std::env::var("RUST_LOG").ok().filter(|s| !s.trim().is_empty());
    let filter = crate::host::logging::Filter::parse(spec.as_deref().unwrap_or(crate::host::logging::DEFAULT_FILTER));
    let max = filter.max();
    let logger = LOGGER.get_or_init(|| OhosLogger(crate::host::logging::AppLogger::new(filter, true)));
    log::set_logger(logger).ok()?;
    log::set_max_level(max);
    Some(&logger.0)
}

struct Paste {
    command: String,
    params: serde_json::Value,
    document: Option<u64>,
    focus: Option<egui::Id>,
    text: Result<Option<String>, String>,
}

struct App {
    app: DesignApp,
    grants: Arc<files::Grants>,
    events: mpsc::Receiver<PlatformEvent>,
    pastes: mpsc::Receiver<Paste>,
    preferences: preferences::Preferences,
}

impl eframe::App for App {
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if let Err(error) = self.preferences.save(&self.app) {
            log::error!("Could not save preferences: {error}");
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let paths = self
            .events
            .try_iter()
            .filter_map(|event| match event {
                PlatformEvent::Want { uri, .. } if !uri.is_empty() => {
                    self.grants.resolve_want(&uri).map(|p| p.to_string_lossy().into_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        self.app.open_documents_later(paths);
        for paste in self.pastes.try_iter() {
            if paste.document != self.app.session.active().map(|d| d.uid) || paste.focus != ctx.memory(|memory| memory.focused()) {
                continue;
            }
            match paste.text {
                Ok(text) => {
                    let mut params = paste.params;
                    if let (Some(text), Some(object)) = (text, params.as_object_mut()) {
                        object.insert("text".into(), text.into());
                    }
                    let hook = self.app.services.ohos_paste.take();
                    if let Err(error) = self.app.run(&paste.command, params) {
                        self.app.status(error);
                    }
                    self.app.services.ohos_paste = hook;
                }
                Err(error) => self.app.status(error),
            }
        }
        self.app.logic(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui);
        self.app.show_file_dialogs(Some(&*frame));
    }

    fn on_exit(&mut self) {
        if let Err(error) = self.app.finish_confirmed_quit() {
            log::error!("Could not finish quit: {error}");
        }
        if !self.app.session.documents().is_empty()
            && let Err(error) = self.app.run("file.recovery.save", serde_json::json!({}))
        {
            log::error!("Could not preserve documents on exit: {error}");
        }
        if let Err(error) = self.preferences.save(&self.app) {
            log::error!("Could not save preferences: {error}");
        }
    }
}

/// The document font database predates craft-font-catalog; populate it from the shared catalogue
/// here, leaving the engine and desktop discovery unchanged.
fn load_fonts() -> Result<std::thread::JoinHandle<()>, String> {
    let catalog = craft_font_catalog::Catalog::global();
    catalog.set_app_dirs(vec![PathBuf::from("/system/fonts")]);
    catalog.scan_in_background();
    std::thread::Builder::new()
        .name("layrune-ohos-fonts".into())
        .spawn(move || {
            let mut seen = std::collections::HashSet::new();
            let mut total = 0usize;
            for priority in
                ["/system/fonts/HarmonyOS_Sans_SC.ttf", "/system/fonts/HarmonyOS_Sans_TC.ttf", "/system/fonts/HarmonyOS_Sans.ttf"]
            {
                let path = PathBuf::from(priority);
                if let Ok(bytes) = std::fs::read(&path)
                    && bytes.len() <= 40 << 20
                {
                    seen.insert(path);
                    total = total.saturating_add(bytes.len());
                    designcraft_fonts::FontDb::global().add_font(bytes);
                }
            }
            for face in catalog.faces().iter() {
                if seen.len() >= 1024 {
                    break;
                }
                let Some(path) = &face.path else { continue };
                if !seen.insert(path.clone()) || std::fs::metadata(path).map_or(true, |m| m.len() > 40 << 20) {
                    continue;
                }
                let Ok(data) = catalog.load(face) else { continue };
                if data.bytes.len() > 40 << 20 || total.saturating_add(data.bytes.len()) > 256 << 20 {
                    continue;
                }
                total = total.saturating_add(data.bytes.len());
                designcraft_fonts::FontDb::global().add_font(data.bytes.to_vec());
            }
        })
        .map_err(|e| e.to_string())
}

pub fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    let logger = install_logger();
    std::panic::set_hook(Box::new(|info| log::error!("internal error: {info}\n{}", std::backtrace::Backtrace::capture())));
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    if let Some(logger) = logger {
        match logger.attach_dir(&directory.join("logs")) {
            Ok(path) => log::info!("Layrune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display()),
            Err(error) => log::warn!("No log file: {error}"),
        }
    }
    let fonts = load_fonts()?;
    let grants = Arc::new(files::Grants::new(directory.clone()));
    let startup_grants = grants.clone();
    let (ready_tx, ready) = mpsc::channel();
    std::thread::Builder::new()
        .name("layrune-grants".into())
        .spawn(move || {
            startup_grants.restore();
            if fonts.join().is_err() {
                log::error!("System font loading stopped; bundled document fonts remain available");
            }
            let _ = ready_tx.send(());
        })
        .map_err(|e| e.to_string())?;
    let events = winit::platform::ohos::events::subscribe();
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        viewport: egui::ViewportBuilder::default()
            .with_title("Layrune")
            .with_app_id("com.richerfu.layrune")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true),
        persistence_path: Some(directory.join("ui.ron")),
        system_font_fallback: false,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        // Match PdfCraft's GLES startup limits on API 26 SDKs.
        setup.instance_descriptor.flags =
            setup.instance_descriptor.flags.difference(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL).with_env();
        let descriptor = setup.device_descriptor.clone();
        setup.device_descriptor = Arc::new(move |adapter| {
            let mut descriptor = descriptor(adapter);
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
        "Layrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let ctx = cc.egui_ctx.clone();
            let (paste_tx, pastes) = mpsc::channel();
            let clipboard_app = ability.clone();
            let clipboard_ctx = ctx.clone();
            let clipboard_commands = task::CommandQueue::default();
            let writable = grants.clone();
            let can_save = grants.clone();
            let services = Services {
                branding: Some(designcraft_ui_egui::Branding {
                    name: "Layrune",
                    icon_png: include_bytes!("../../../../../branding/layrune-128.png"),
                    attribution: "Layrune · 造隅。Based on DesignCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
                }),
                read: Some(Box::new(|path| files::read_bounded(std::path::Path::new(path), 512 << 20))),
                write: Some(Box::new(move |path, bytes| {
                    let path = std::path::Path::new(path);
                    if !writable.can_save(path) {
                        return Err("Authorize the containing folder with Save As before saving this file".into());
                    }
                    designcraft_engine::atomic::write_atomic(path, bytes).map_err(|e| e.to_string())
                })),
                ohos_dialog: Some(files::dialog_service(ability, grants.clone(), ctx.clone())),
                ohos_can_save: Some(Box::new(move |path| can_save.can_save(path))),
                ohos_paste: Some(Box::new(move |command, params, document| {
                    let (app, ctx, tx) = (clipboard_app.clone(), clipboard_ctx.clone(), paste_tx.clone());
                    let focus = ctx.memory(|memory| memory.focused());
                    clipboard_commands.push(async move {
                        let text = async {
                            app.request_permission("ohos.permission.READ_PASTEBOARD").await.map_err(|e| e.to_string())?;
                            ClipboardClient::new(&app).map_err(|e| e.to_string())?.read_text().await.map_err(|e| e.to_string())
                        }
                        .await;
                        let _ = tx.send(Paste { command, params, document, focus, text });
                        ctx.request_repaint();
                    });
                })),
                ..Default::default()
            };
            let mut session = Session::new();
            session.recovery_dir = Some(designcraft_engine::recovery::run_dir(&directory.join("Recovery")).map_err(std::io::Error::other)?);
            let mut app = DesignApp::new(session, services);
            let preferences = preferences::Preferences::load(&directory, &mut app);
            #[cfg(feature = "ohos-control")]
            {
                let receiver = crate::host::control_server::start(7878, ctx);
                if matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
                    return Err(std::io::Error::other("Could not start the OHOS control server on port 7878").into());
                }
                app = app.with_control(receiver);
                designcraft_engine::atomic::write_atomic(&directory.join("control.json"), br#"{"port":7878}"#).map_err(std::io::Error::other)?;
            }
            Ok(Box::new(StartupApp { ready, app: Some(App { app, grants, events, pastes, preferences }), restored: false }))
        }),
    )
    .map_err(|e| e.to_string())
}

/// Restore grants before opening recovery/recent documents without blocking the first frame.
struct StartupApp {
    ready: mpsc::Receiver<()>,
    app: Option<App>,
    restored: bool,
}

impl eframe::App for StartupApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(app) = &mut self.app {
            app.save(storage);
        }
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.restored {
            match self.ready.try_recv() {
                Ok(()) => {
                    self.restored = true;
                    if let Some(app) = &mut self.app
                        && let Err(error) = app.app.run("file.recovery.open", serde_json::json!({}))
                    {
                        app.app.status(error);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.restored = true;
                    if let Some(app) = &mut self.app {
                        app.app.status("File grant restoration stopped; select files again");
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    return;
                }
            }
        }
        if let Some(app) = &mut self.app {
            app.logic(ctx, frame);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.restored {
            if let Some(app) = &mut self.app {
                app.ui(ui, frame);
            }
        } else {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.spinner();
                ui.label("Restoring file access…");
            });
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.restored
            && let Some(app) = &mut self.app
        {
            app.raw_input_hook(ctx, input);
        }
    }

    fn on_exit(&mut self) {
        if let Some(app) = &mut self.app {
            app.on_exit();
        }
    }
}

//! OpenHarmony lifecycle host. Desktop entry points and platform modules stay independent.

mod clipboard;
mod files;
mod grant_store;
mod jobs;

use filmcraft_engine::{Session, autosave::AutosaveConfig};
use filmcraft_ui_egui::{FilmcraftApp, HostHooks};
use openharmony_ability::OpenHarmonyApp;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock, mpsc},
    time::SystemTime,
};
use winit::platform::ohos::{events::PlatformEvent, plugins::url::UrlExt as _};

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
    let spec = std::env::var("RUST_LOG").ok().filter(|spec| !spec.trim().is_empty());
    let filter = crate::host::logging::Filter::parse(spec.as_deref().unwrap_or(crate::host::logging::DEFAULT_FILTER));
    let max = filter.max();
    let logger = LOGGER.get_or_init(|| OhosLogger(crate::host::logging::AppLogger::new(filter, true)));
    log::set_logger(logger).ok()?;
    log::set_max_level(max);
    Some(&logger.0)
}

fn hooks(ability: &OpenHarmonyApp, grants: &Arc<files::Grants>, ctx: &egui::Context, errors: &mpsc::Sender<String>) -> HostHooks {
    let (url_app, url_ctx, url_errors) = (ability.clone(), ctx.clone(), errors.clone());
    let (path_app, path_ctx, path_errors) = (ability.clone(), ctx.clone(), errors.clone());
    let language_app = ability.clone();
    let theme_app = ability.clone();
    HostHooks {
        branding: Some(filmcraft_ui_egui::Branding {
            name: "Cliprune",
            icon_png: include_bytes!("../../../../branding/cliprune-128.png"),
            attribution: "Cliprune is an independent HarmonyOS adaptation of FilmCraft. MIT OR Apache-2.0. Copyright 2026 ArtCraft Team and the FilmCraft contributors. Upstream contributor and model credits are retained.",
        }),
        run_dialog: Some(files::dialog_runner(ability.clone(), Arc::clone(grants), ctx.clone(), errors.clone())),
        open_url: Some(Box::new(move |url| {
            let future = url_app.open_url(url);
            jobs::spawn("url", async move { future.await.map_err(|error| error.to_string()) }, url_ctx.clone(), url_errors.clone());
            Ok(())
        })),
        open_path: Some(Box::new(move |path, reveal| {
            let app = path_app.clone();
            let path = path.to_owned();
            jobs::spawn(
                "open-path",
                async move {
                    if reveal {
                        let path = std::path::Path::new(&path);
                        let directory = if path.is_dir() { path } else { path.parent().ok_or("The file has no containing directory")? };
                        app.reveal_in_dir(directory.to_string_lossy().into_owned()).await
                    } else {
                        let uri =
                            winit::platform::ohos::drag_drop::file_uri(std::path::Path::new(&path)).ok_or("Cannot resolve file URI")?;
                        app.open_file(uri).await
                    }
                    .map_err(|error| error.to_string())
                },
                path_ctx.clone(),
                path_errors.clone(),
            );
            Ok(())
        })),
        clipboard_media: Some(clipboard::service(ability.clone(), Arc::clone(grants), ctx.clone(), errors.clone())),
        cursor_screen_position: Some(Box::new(|| {
            let (x, y) = winit::platform::ohos::drag_drop::last_position()?;
            let valid = |value: f64| value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX);
            if valid(x) && valid(y) { Some((x as i32, y as i32)) } else { None }
        })),
        system_languages: Some(Box::new(move || {
            let language = language_app.config().language;
            if language.is_empty() { sys_locale::get_locales().collect() } else { vec![language] }
        })),
        system_theme: Some(Box::new(move |_| match theme_app.config().color_mode {
            openharmony_ability::ColorMode::Dark => Some(egui::Theme::Dark),
            openharmony_ability::ColorMode::Light => Some(egui::Theme::Light),
            _ => None,
        })),
        ..Default::default()
    }
}

fn local_offset(unix_seconds: i64) -> i32 {
    chrono::DateTime::from_timestamp(unix_seconds, 0).map_or(0, |time| time.with_timezone(&chrono::Local).offset().local_minus_utc())
}

pub fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    let logger = install_logger();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let logs = directory.join(crate::host::logging::LOG_DIR);
    if let Some(logger) = logger {
        match logger.attach_dir(&logs) {
            Ok(path) => {
                log::info!("Cliprune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display())
            }
            Err(error) => log::warn!("No log file: {error}"),
        }
    }
    filmcraft_ui_egui::crash::install(Some(logs));
    craft_font_catalog::Catalog::global().set_app_dirs(vec![PathBuf::from("/system/fonts")]);
    craft_font_catalog::Catalog::global().scan_in_background();
    // FilmCraft's text engine also needs these families for titles and captions.
    filmcraft_text::fonts::add_source(&filmcraft_text::fonts::DirectorySource { dirs: vec![PathBuf::from("/system/fonts")] });
    let grants = Arc::new(files::Grants::new(directory.clone()));
    let startup_error = grants.restore().err();
    // Subscribe before creating the surface so the launch Want is retained.
    let events = winit::platform::ohos::events::subscribe();
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        viewport: egui::ViewportBuilder::default()
            .with_title("Cliprune")
            .with_app_id("com.richerfu.cliprune")
            .with_inner_size([1600.0, 980.0])
            .with_min_inner_size([900.0, 560.0])
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
    eframe::run_native(
        "Cliprune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            filmcraft_ui_egui::install_ime_reset_callbacks(&cc.egui_ctx);
            let mut session = Session::default();
            let mut config = AutosaveConfig::new(&directory);
            config.local_offset = local_offset;
            let persistence_error =
                session.start_autosave(config).err().map(|error| format!("Auto-save and recovery unavailable: {error}"));
            if session.prefs.general.at_startup == "openMostRecent" {
                if let Some(path) = session.prefs.general.recent_projects.iter().find(|path| std::path::Path::new(path).is_file()).cloned()
                    && let Err(error) = session.execute("file.open", serde_json::json!({"path": path}))
                {
                    log::warn!("Cannot reopen recent project: {error}");
                }
            } else if session.prefs.general.at_startup != "emptyProject"
                && let Err(error) = session.execute("file.openDemoProject", serde_json::json!({}))
            {
                log::warn!("Cannot open demo project: {error}");
            }
            let mut app = FilmcraftApp::new(session);
            let (error_tx, errors) = mpsc::channel();
            app.hooks = hooks(&ability, &grants, &cc.egui_ctx, &error_tx);
            if let Some(error) = persistence_error.or(startup_error) {
                app.ui.status = error;
            }
            if let Some(state) = cc.wgpu_render_state.clone()
                && std::env::var_os("FILMCRAFT_CPU_COMPOSITE").is_none()
            {
                app.set_wgpu(state);
            }
            log::info!("Cliprune OpenHarmony editor ready");
            Ok(Box::new(OhosApp { app, grants, events, errors, error_tx, failure: None }))
        }),
    )
    .map_err(|error| error.to_string())
}

struct OhosApp {
    app: FilmcraftApp,
    grants: Arc<files::Grants>,
    events: mpsc::Receiver<PlatformEvent>,
    errors: mpsc::Receiver<String>,
    error_tx: mpsc::Sender<String>,
    failure: Option<String>,
}

impl OhosApp {
    fn guarded(&mut self, action: impl FnOnce(&mut Self)) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(self))).is_err() {
            let error =
                "An OpenHarmony operation stopped unexpectedly. Your session is still open; save your work and see Logs for details."
                    .to_owned();
            self.app.ui.status = error.clone();
            self.failure = Some(error);
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        for error in self.errors.try_iter().take(64) {
            self.app.ui.status = error;
        }
        for event in self.events.try_iter().take(64) {
            if let PlatformEvent::Want { uri, .. } = event
                && !uri.is_empty()
            {
                match self.grants.resolve_want(&uri) {
                    Ok(path) => {
                        let command = if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("fcproj")) {
                            "file.open"
                        } else {
                            "file.import"
                        };
                        let path = path.to_string_lossy();
                        let params =
                            if command == "file.open" { serde_json::json!({"path": path}) } else { serde_json::json!({"paths": [path]}) };
                        // The normal UI command asks about unsaved work before replacing a project.
                        if let Err(error) = filmcraft_ui_egui::menus::invoke(&mut self.app, ctx, command, params) {
                            let _ = self.error_tx.send(error);
                        }
                    }
                    Err(error) => self.app.ui.status = error,
                }
            }
        }
    }
}

impl eframe::App for OhosApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.guarded(|host| host.app.raw_input_hook(ctx, input));
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.guarded(|host| {
            host.poll(ctx);
            host.app.logic(ctx, frame);
        });
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.guarded(|host| {
            host.poll(ui.ctx());
            host.app.ui(ui, frame);
        });
        if let Some(error) = self.failure.clone() {
            egui::Window::new("Cliprune error").collapsible(false).show(ui.ctx(), |ui| {
                ui.label(error);
                if ui.button("Continue").clicked() {
                    self.failure = None;
                }
            });
        }
    }

    fn on_exit(&mut self) {
        self.guarded(|host| host.app.on_exit());
    }
}

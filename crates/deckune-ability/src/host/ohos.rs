//! OpenHarmony lifecycle, preferences, logging and embedded egui host.

use deckcraft_ui_egui::SlideApp;
use openharmony_ability::OpenHarmonyApp;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::SystemTime,
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
        let hilog_msg = line.trim_end_matches('\n').replace('\0', "");
        match record.level() {
            log::Level::Error => ohos_hilog_binding::error(hilog_msg),
            log::Level::Warn => ohos_hilog_binding::warn(hilog_msg),
            log::Level::Info => ohos_hilog_binding::info(hilog_msg),
            log::Level::Debug | log::Level::Trace => ohos_hilog_binding::debug(hilog_msg),
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

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let trace = std::backtrace::Backtrace::capture();
        let report = if trace.status() == std::backtrace::BacktraceStatus::Captured {
            format!("internal error: {info}\n{trace}")
        } else {
            format!("internal error: {info}")
        };
        if log::log_enabled!(log::Level::Error) {
            log::error!("{report}");
        } else {
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("deckune: {report}\n"));
        }
    }));
}

pub fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    let logger = install_logger();
    install_panic_hook();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    if let Some(logger) = logger {
        match logger.attach_dir(&directory.join("logs")) {
            Ok(path) => {
                log::info!("Deckune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display())
            }
            Err(error) => log::warn!("no log file: {error}"),
        }
    }
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability),
        viewport: egui::ViewportBuilder::default()
            .with_title("Deckune")
            .with_app_id("com.richerfu.deckune")
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
    match eframe::icon_data::from_png_bytes(include_bytes!("../../../../branding/deckune-128.png")) {
        Ok(icon) => options.viewport = options.viewport.with_icon(icon),
        Err(error) => log::warn!("could not load Deckune icon: {error}"),
    }
    eframe::run_native(
        "Deckune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            deckcraft_ui_egui::install_ime_pass_hooks(&cc.egui_ctx);
            let services = deckcraft_ui_egui::Services {
                branding: Some(deckcraft_ui_egui::Branding {
                    name: "Deckune",
                    icon_png: include_bytes!("../../../../branding/deckune-128.png"),
                    attribution: "Deckune · 造隅。Based on DeckCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
                }),
                read: Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string()))),
                write: Some(Box::new(|p: &str, b: &[u8]| std::fs::write(p, b).map_err(|e| e.to_string()))),
                ..Default::default()
            };
            let mut app = SlideApp::new(deckcraft_engine::Session::new(), services);
            app.ui.brightness = deckcraft_ui_egui::theme::Brightness::Dark;
            let (ui, prefs) = deckcraft_ui_egui::settings::load(&directory);
            if let Some(ui) = ui {
                app.ui = ui;
            }
            if let Some(prefs) = prefs {
                app.session.prefs = prefs;
            }
            app.session.recovery_dir = Some(directory.join("recovery"));
            match app.session.execute("file.recovery.open", &serde_json::json!({})) {
                Ok(value) => {
                    if let Some(count) = value.get("opened").and_then(|opened| opened.as_array()).map(Vec::len).filter(|count| *count > 0) {
                        app.set_status(format!("Recovered {count} unsaved presentation(s) from the last session"));
                    }
                }
                Err(error) => log::warn!("could not restore presentations: {error}"),
            }
            app.open_backstage(deckcraft_ui_egui::backstage::Page::Home);
            log::info!("Deckune OpenHarmony workbench ready");
            Ok(Box::new(App { app, directory }))
        }),
    )
    .map_err(|error| error.to_string())
}

struct App {
    app: SlideApp,
    directory: PathBuf,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.logic(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        deckcraft_ui_egui::settings::save(&self.directory, &self.app.ui, &self.app.session.prefs);
    }

    fn on_exit(&mut self) {
        deckcraft_ui_egui::settings::save(&self.directory, &self.app.ui, &self.app.session.prefs);
    }
}

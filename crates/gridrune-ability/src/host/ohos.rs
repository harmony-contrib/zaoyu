//! OpenHarmony lifecycle, preferences, logging and embedded egui host.

mod clipboard;
mod files;
mod services;

use gridcraft_ui_egui::SheetApp;
use openharmony_ability::OpenHarmonyApp;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::SystemTime,
};
use winit::platform::ohos::events::PlatformEvent;

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
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("gridrune: {report}\n"));
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
                log::info!("Gridrune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display())
            }
            Err(error) => log::warn!("no log file: {error}"),
        }
    }
    let grants = Arc::new(files::Grants::new(directory.clone()));
    grants.restore();
    // Subscribe before surface creation so launch Wants reach the first frame.
    let events = winit::platform::ohos::events::subscribe();
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        viewport: egui::ViewportBuilder::default()
            .with_title("Gridrune")
            .with_app_id("com.richerfu.gridrune")
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
    eframe::run_native(
        "Gridrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let services = services::build(&ability, &grants, &cc.egui_ctx);
            let mut sheet = SheetApp::new(gridcraft_engine::Session::new(), services);
            load_prefs(&mut sheet, &directory);
            sheet.backstage = Some(gridcraft_ui_egui::backstage::Backstage::new(gridcraft_ui_egui::backstage::Page::Home));
            gridcraft_ui_egui::fonts::start_scan();
            SheetApp::setup_context_for_language(&cc.egui_ctx, sheet.ui.dark, sheet.ui.language);
            log::info!("Gridrune OpenHarmony workbench ready");
            Ok(Box::new(App { sheet, directory, events, grants }))
        }),
    )
    .map_err(|error| error.to_string())
}

struct App {
    sheet: SheetApp,
    directory: PathBuf,
    events: std::sync::mpsc::Receiver<PlatformEvent>,
    grants: Arc<files::Grants>,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for event in self.events.try_iter() {
            if let PlatformEvent::Want { uri, .. } = event
                && !uri.is_empty()
                && let Some(inbox) = &self.sheet.services.inbox
            {
                files::open_uri(Arc::clone(&self.grants), uri, Arc::clone(inbox), ctx.clone());
            }
        }
        self.sheet.logic(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.sheet.raw_input_hook(ctx, input);
        for file in std::mem::take(&mut input.dropped_files) {
            if let Some(uri) = winit::platform::ohos::drag_drop::file_uri(file.path())
                && let Some(inbox) = &self.sheet.services.inbox
            {
                files::open_uri(Arc::clone(&self.grants), uri, Arc::clone(inbox), ctx.clone());
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.sheet.ui(ui);
    }

    fn on_exit(&mut self) {
        save_prefs(&self.sheet, &self.directory);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        save_prefs(&self.sheet, &self.directory);
    }
}

fn load_prefs(sheet: &mut SheetApp, directory: &std::path::Path) {
    sheet.ui.language = gridcraft_ui_egui::i18n::Language::system();
    sheet.ui.dark = true;
    sheet.ui.system_theme = false;
    if let Ok(bytes) = files::read_bounded(&directory.join("ui.json"), 4 * 1024 * 1024)
        && let Ok(ui) = serde_json::from_slice::<gridcraft_ui_egui::UiState>(&bytes)
    {
        let saved = serde_json::from_slice(&bytes).ok().and_then(|value| gridcraft_ui_egui::i18n::saved_language(&value));
        let language = saved.unwrap_or(sheet.ui.language);
        sheet.ui = ui.at_startup();
        sheet.ui.language = language;
    }
    if let Ok(bytes) = files::read_bounded(&directory.join("prefs.json"), 4 * 1024 * 1024)
        && let Ok(prefs) = serde_json::from_slice(&bytes)
    {
        sheet.session.prefs = prefs;
    }
}

fn save_prefs(sheet: &SheetApp, directory: &std::path::Path) {
    for (name, bytes) in
        [("ui.json", serde_json::to_vec_pretty(&sheet.ui)), ("prefs.json", serde_json::to_vec_pretty(&sheet.session.prefs))]
    {
        let result = bytes.map_err(|error| error.to_string()).and_then(|bytes| files::write_atomic(&directory.join(name), &bytes));
        if let Err(error) = result {
            log::warn!("could not save {name}: {error}");
        }
    }
}

//! OpenHarmony lifecycle, sandbox persistence and native egui hosting.

use super::logging;
use openharmony_ability::OpenHarmonyApp;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::SystemTime,
};

const ICON_PNG: &[u8] = include_bytes!("../../../../branding/visrune-128.png");

struct OhosLogger(logging::AppLogger);

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
        let line = logging::format_line(
            &logging::timestamp(SystemTime::now()),
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

fn install_logger() -> Option<&'static logging::AppLogger> {
    static LOGGER: OnceLock<OhosLogger> = OnceLock::new();
    let spec = std::env::var("RUST_LOG").ok().filter(|spec| !spec.trim().is_empty());
    let filter = logging::Filter::parse(spec.as_deref().unwrap_or(logging::DEFAULT_FILTER));
    let max = filter.max();
    let logger = LOGGER.get_or_init(|| OhosLogger(logging::AppLogger::new(filter, true)));
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
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("visrune: {report}\n"));
        }
    }));
}

pub(super) fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    let logger = install_logger();
    install_panic_hook();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Cannot create application data directory {}: {error}", directory.display()))?;
    if let Some(logger) = logger {
        match logger.attach_dir(&directory.join("logs")) {
            Ok(path) => log::info!("Visrune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display()),
            Err(error) => log::warn!("Cannot open session log: {error}"),
        }
    }
    let icon = eframe::icon_data::from_png_bytes(ICON_PNG).map_err(|error| format!("Cannot decode the Visrune icon: {error}"))?;

    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability),
        viewport: egui::ViewportBuilder::default()
            .with_title("Visrune")
            .with_app_id("com.richerfu.visrune")
            .with_icon(icon)
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
        "Visrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let branding = artcraft_native::Branding {
                name: "Visrune",
                icon_png: ICON_PNG,
                attribution: "Visrune · 造隅。Based on ArtCraft Studio, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
            };
            let app = artcraft_native::ArtcraftApp::with_branding(cc, branding, &directory);
            log::info!("Visrune OpenHarmony workbench ready");
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| error.to_string())
}

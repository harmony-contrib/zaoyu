//! OpenHarmony lifecycle, preferences, logging and embedded egui host.

use cadcraft_ui_egui::CadApp;
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
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("dimrune: {report}\n"));
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
                log::info!("Dimrune {}, log file {}", env!("CARGO_PKG_VERSION"), path.display())
            }
            Err(error) => log::warn!("no log file: {error}"),
        }
    }
    install_io();
    cadcraft_engine::settings_dir::set(Some(directory.clone()));
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability),
        viewport: egui::ViewportBuilder::default()
            .with_title("Dimrune")
            .with_app_id("com.richerfu.dimrune")
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
    options.stencil_buffer = 8;
    match eframe::icon_data::from_png_bytes(include_bytes!("../../../../branding/dimrune-128.png")) {
        Ok(icon) => options.viewport = options.viewport.with_icon(icon),
        Err(error) => log::warn!("could not load Dimrune icon: {error}"),
    }
    eframe::run_native(
        "Dimrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let services = cadcraft_ui_egui::Services {
                branding: Some(cadcraft_ui_egui::Branding {
                    name: "Dimrune",
                    icon_png: include_bytes!("../../../../branding/dimrune-128.png"),
                    attribution: "Dimrune · 造隅。Based on CADCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
                }),
                pick_open: None,
                pick_save: None,
                pick_lisp: None,
                no_file_system: false,
            };
            let mut app = CadApp::new(cadcraft_engine::Session::empty(), services);
            if let Some(profile) = cc.storage.and_then(|storage| storage.get_string(cadcraft_ui_egui::options::PROFILE_KEY)) {
                app.load_profile(&profile);
            }
            if let Some(prefs) = cc.storage.and_then(|storage| storage.get_string(cadcraft_ui_egui::PREFS_KEY)) {
                app.load_prefs(&prefs);
            }
            if let Some(prefs) = cc.storage.and_then(|storage| storage.get_string(cadcraft_ui_egui::autosave::PREFS_KEY)) {
                cadcraft_ui_egui::autosave::load_prefs(&prefs);
            }
            app.enable_autosave(&directory.join("autosave"));
            if let Some(rs) = &cc.wgpu_render_state {
                app.set_wgpu(rs, eframe::egui_wgpu::depth_format_from_bits(0, 8));
            }
            let metric = app.session.settings.measureinit;
            app.session.new_drawing(metric);
            app.ui.start_tab = true;
            log::info!("Dimrune OpenHarmony workbench ready");
            Ok(Box::new(App { app }))
        }),
    )
    .map_err(|error| error.to_string())
}

/// Keep the engine's format hooks identical to CADCraft's native host.
fn install_io() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });
    cadcraft_engine::cmd::file::set_image_decoder(|b| cadcraft_io::raster::decode(b).map_err(|e| e.to_string()));
    cadcraft_engine::cmd::file::set_write_as(|d, name, o| {
        cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: o.version, binary: o.binary }).map_err(|e| e.to_string())
    });
    // AutoLISP's entget, entmake and entmod see entities as DXF groups.
    cadcraft_engine::cmd::file::set_entity_codec(cadcraft_engine::cmd::file::EntityCodec {
        groups: cadcraft_io::entity_groups,
        entity: cadcraft_io::entity_from_groups,
    });
}

struct App {
    app: CadApp,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.guarded_logic(ctx);
        if std::mem::take(&mut self.app.quit_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
        cadcraft_ui_egui::cmdline::capture_tab(ctx, input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.guarded_ui(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(cadcraft_ui_egui::options::PROFILE_KEY, self.app.profile_json());
        storage.set_string(cadcraft_ui_egui::PREFS_KEY, self.app.prefs_json());
        storage.set_string(cadcraft_ui_egui::autosave::PREFS_KEY, cadcraft_ui_egui::autosave::prefs_json());
    }

    fn on_exit(&mut self) {
        self.app.cancel_screenshots();
        self.app.shutdown_autosave();
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

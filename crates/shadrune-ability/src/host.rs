//! HarmonyOS lifecycle and persistence for the pinned MaterialCraft editor.
#![forbid(unsafe_code)]

use std::{path::PathBuf, sync::Arc};

pub(super) fn start(ability: openharmony_ability::OpenHarmonyApp) -> Result<(), String> {
    install_logger();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| format!("Cannot create Shadrune data directory: {error}"))?;

    let session = materialcraft_engine::new_session();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Shadrune")
        .with_app_id("com.richerfu.shadrune")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([960.0, 600.0])
        .with_drag_and_drop(true);
    match eframe::icon_data::from_png_bytes(include_bytes!("../../../branding/shadrune-128.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(error) => log::warn!("Cannot load Shadrune window icon: {error}"),
    }
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability),
        viewport,
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
        "Shadrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let mut app = materialcraft_ui_egui::MaterialCraftApp::new(cc, session);
            app.services.branding = Some(materialcraft_ui_egui::Branding {
                name: "Shadrune",
                icon_png: include_bytes!("../../../branding/shadrune-128.png"),
                attribution: "Shadrune · 造隅。Based on MaterialCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
            });
            log::info!("Shadrune {} OpenHarmony workbench ready", env!("CARGO_PKG_VERSION"));
            Ok(Box::new(App { app }))
        }),
    )
    .map_err(|error| format!("Shadrune event loop failed: {error}"))
}

fn install_logger() {
    let spec = std::env::var("RUST_LOG").ok().filter(|value| !value.trim().is_empty());
    let filter = ohos_hilog_binding::log::FilterBuilder::new()
        .parse(spec.as_deref().unwrap_or("warn,materialcraft=info,shadrune_ability=info"))
        .build();
    ohos_hilog_binding::log::init_once(
        ohos_hilog_binding::log::Config::default().with_tag("Shadrune").with_max_level(log::LevelFilter::Trace).with_filter(filter).format(
            |formatter, record| {
                write!(
                    formatter,
                    "{} {} {}: {}",
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                    record.level(),
                    record.target(),
                    record.args()
                )
            },
        ),
    );
}

struct App {
    app: materialcraft_ui_egui::MaterialCraftApp,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app.save(storage);
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        self.app.auto_save_interval()
    }
}

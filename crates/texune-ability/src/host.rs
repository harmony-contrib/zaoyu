//! HarmonyOS lifecycle and persistence for the pinned TextureCraft editor.
#![forbid(unsafe_code)]

use std::{path::PathBuf, sync::Arc};

pub(super) fn start(ability: openharmony_ability::OpenHarmonyApp) -> Result<(), String> {
    install_logger();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| format!("Cannot create Texune data directory: {error}"))?;

    let session = texturecraft_engine::sample_session(256, 64)
        .or_else(|_| {
            texturecraft_engine::Session::new_project(
                "Untitled",
                texturecraft_engine::geom::primitives::material_ball(),
                "builtin:material_ball",
                None,
                256,
            )
        })
        .map_err(|error| format!("Cannot create Texune initial session: {error}"))?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Texune")
        .with_app_id("com.richerfu.texune")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([960.0, 600.0])
        .with_drag_and_drop(true);
    match eframe::icon_data::from_png_bytes(include_bytes!("../../../branding/texune-128.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(error) => log::warn!("Cannot load Texune window icon: {error}"),
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
        "Texune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let mut app = texturecraft_ui_egui::TextureCraftApp::new(cc, session);
            app.ui.windows.start = app.ui.prefs.start_window;
            app.services.branding = Some(texturecraft_ui_egui::Branding {
                name: "Texune",
                icon_png: include_bytes!("../../../branding/texune-128.png"),
                attribution: "Texune · 造隅。Based on TextureCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
            });
            log::info!("Texune {} OpenHarmony workbench ready", env!("CARGO_PKG_VERSION"));
            Ok(Box::new(App { app }))
        }),
    )
    .map_err(|error| format!("Texune event loop failed: {error}"))
}

fn install_logger() {
    let spec = std::env::var("RUST_LOG").ok().filter(|value| !value.trim().is_empty());
    let filter = ohos_hilog_binding::log::FilterBuilder::new()
        .parse(spec.as_deref().unwrap_or("warn,texturecraft=info,texune_ability=info"))
        .build();
    ohos_hilog_binding::log::init_once(
        ohos_hilog_binding::log::Config::default().with_tag("Texune").with_max_level(log::LevelFilter::Trace).with_filter(filter).format(
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
    app: texturecraft_ui_egui::TextureCraftApp,
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

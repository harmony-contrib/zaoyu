//! HarmonyOS lifecycle and services for the pinned LightCraft editor.
#![forbid(unsafe_code)]

mod files;
mod logging;

use lightcraft_ui_egui::{LightcraftApp, Services};
use openharmony_ability::OpenHarmonyApp;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::platform::ohos::{
    plugins::files::{FileDialogFilter, FileDialogOptions},
    task,
};

const BRANDING: lightcraft_ui_egui::Branding = lightcraft_ui_egui::Branding {
    name: "Lutrune",
    icon_png: include_bytes!("../../../branding/lutrune-128.png"),
    attribution: "Lutrune · 造隅。Based on LightCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
};

pub(super) fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    logging::install();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let grants = Arc::new(files::Grants::new(directory.clone()));
    grants.restore()?;

    let catalog = craft_font_catalog::Catalog::global();
    catalog.set_app_dirs(vec![PathBuf::from("/system/fonts")]);
    catalog.scan_in_background();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Lutrune")
        .with_app_id("com.richerfu.lutrune")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([760.0, 480.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(BRANDING.icon_png) {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        viewport,
        persistence_path: Some(directory.join("ui.ron")),
        system_font_fallback: false,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    configure_gpu(&mut options);

    eframe::run_native(
        "Lutrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let mut session = lightcraft_engine::Session::new().with_fs().with_default_denoise_models().with_system_clock();
            let library_error = session.open_library(directory.join("library"), true).err();
            let mut app = LightcraftApp::new(session, services(&ability, &grants, &directory, &cc.egui_ctx));
            app.hooks.branding = Some(BRANDING);
            load_prefs(&mut app, &directory);
            if let Some(error) = library_error {
                let message = format!("Could not open the Lutrune library: {error}");
                log::error!("{message}");
                app.notices.push(message);
            } else {
                lightcraft_ui_egui::panels::empty_library::open_on_empty_library(&mut app);
            }
            log::info!("Lutrune {} OpenHarmony editor ready", env!("CARGO_PKG_VERSION"));
            Ok(Box::new(App { app, directory }))
        }),
    )
    .map_err(|error| error.to_string())
}

fn configure_gpu(options: &mut eframe::NativeOptions) {
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
}

fn services(ability: &OpenHarmonyApp, grants: &Arc<files::Grants>, directory: &Path, ctx: &egui::Context) -> Services {
    let ability = ability.clone();
    let grants = Arc::clone(grants);
    let repaint = ctx.clone();
    let links = ctx.clone();
    Services {
        local_places: Some(vec![("Lutrune".into(), directory.to_string_lossy().into_owned())]),
        picker: Some(Box::new(move |request| {
            use lightcraft_ui_egui::pick::PickKind;
            let kind = match request.kind {
                PickKind::Files | PickKind::File => "open-file",
                PickKind::Folder => "open-folder",
                PickKind::Save => "save-file",
            };
            let mut options = FileDialogOptions::new(kind).allow_many(request.kind == PickKind::Files);
            if let Some((name, extensions)) = request.filter {
                options = options.filters(vec![FileDialogFilter::new().name(name).pattern(extensions.join(";"))]);
            }
            if let Some(name) = request.file_name {
                options = options.suggested_name(name);
            }
            let ability = ability.clone();
            let grants = Arc::clone(&grants);
            let repaint = repaint.clone();
            let (sender, receiver) = std::sync::mpsc::channel();
            task::spawn(async move {
                let paths = match files::pick(&ability, &grants, options).await {
                    Ok(paths) => paths.into_iter().map(|path| path.to_string_lossy().into_owned()).collect(),
                    Err(error) => {
                        log::warn!("Lutrune file dialog failed: {error}");
                        Vec::new()
                    }
                };
                let _ = sender.send(paths);
                repaint.request_repaint();
            });
            Ok(receiver)
        })),
        write_shared: Some(Arc::new(lightcraft_engine::export::write_file)),
        write: Some(Box::new(lightcraft_engine::export::write_file)),
        write_new: Some(Box::new(lightcraft_engine::export::write_file_new)),
        png: Some(Box::new(|image: &lightcraft_raster::Rgba8| {
            match lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(image), &lightcraft_codecs::EncodeMeta::default()) {
                Ok(bytes) => bytes,
                Err(error) => {
                    log::warn!("Lutrune PNG encoding failed: {error}");
                    Vec::new()
                }
            }
        })),
        open_url: Some(Box::new(move |url| {
            if !url.starts_with("https://") && !url.starts_with("http://") && !url.starts_with("mailto:") {
                return Err("Only HTTP, HTTPS and mailto links can be opened".into());
            }
            links.open_url(egui::OpenUrl::new_tab(url));
            Ok(())
        })),
        ..Default::default()
    }
}

struct App {
    app: LightcraftApp,
    directory: PathBuf,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.logic(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.perf.host_frame_ms = frame.info().cpu_usage.map(|seconds| f64::from(seconds) * 1000.0);
        self.app.ui(ui);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        save_prefs(&self.app, &self.directory);
    }

    fn on_exit(&mut self) {
        save_prefs(&self.app, &self.directory);
        if let Err(error) = self.app.session.close_library() {
            log::error!("Could not save the Lutrune library: {error}");
        }
        if !self.app.shutdown(Duration::from_secs(5)) {
            log::warn!("Lutrune background work did not finish before shutdown");
        }
    }
}

fn load_prefs(app: &mut LightcraftApp, directory: &Path) {
    if let Ok(bytes) = files::read_bounded(&directory.join("ui.json"), 4 * 1024 * 1024) {
        match serde_json::from_slice::<lightcraft_ui_egui::UiState>(&bytes) {
            Ok(ui) => app.ui = ui.sanitized(),
            Err(error) => log::warn!("Could not read Lutrune preferences: {error}"),
        }
    }
    lightcraft_ui_egui::i18n::set_language(app.ui.language);
}

fn save_prefs(app: &LightcraftApp, directory: &Path) {
    let result = serde_json::to_vec_pretty(&app.ui)
        .map_err(|error| error.to_string())
        .and_then(|bytes| files::write_atomic(&directory.join("ui.json"), &bytes));
    if let Err(error) = result {
        log::warn!("Could not save Lutrune preferences: {error}");
    }
}

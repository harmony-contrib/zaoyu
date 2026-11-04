//! HarmonyOS lifecycle and services for the pinned FigCraft editor.
#![forbid(unsafe_code)]

mod files;
mod logging;

use figcraft_engine::Session;
use figcraft_ui_egui::{DialogFuture, FigApp, Services, windows};
use openharmony_ability::OpenHarmonyApp;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    task::{Poll, Waker},
};
use winit::platform::ohos::{
    plugins::files::{FileDialogFilter, FileDialogOptions},
    task,
};

const ICON: &[u8] = include_bytes!("../../../branding/framune-128.png");

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
    craft_font_catalog::Catalog::global().set_app_dirs(vec![PathBuf::from("/system/fonts")]);

    let mut session = Session::empty();
    if let Ok(bytes) = files::read_bounded(&directory.join("prefs.json"), 4 * 1024 * 1024) {
        match serde_json::from_slice(&bytes) {
            Ok(prefs) => session.prefs = prefs,
            Err(error) => log::warn!("Could not read Framune preferences: {error}"),
        }
    }
    session.execute("file.new", &serde_json::json!({})).map_err(|error| error.to_string())?;
    figcraft_ui_egui::i18n::set(&session.prefs.language, None);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Framune")
        .with_app_id("com.richerfu.framune")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([800.0, 500.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
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
        "Framune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let repaint = cc.egui_ctx.clone();
            if let Err(error) = std::thread::Builder::new().name("framune-fonts".into()).spawn(move || {
                figcraft_text::scan_system_fonts();
                repaint.request_repaint();
            }) {
                log::warn!("Could not start the Framune font scan: {error}");
            }
            let host_services = HostServices { ability, grants, directory: directory.clone(), context: cc.egui_ctx.clone() };
            let mut fig = FigApp::new(session, Box::new(host_services));
            fig.ui.home = true;
            windows::install(fig);
            log::info!("Framune {} OpenHarmony editor ready", env!("CARGO_PKG_VERSION"));
            Ok(Box::new(App { directory, title: "Framune".into() }))
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

struct HostServices {
    ability: OpenHarmonyApp,
    grants: Arc<files::Grants>,
    directory: PathBuf,
    context: egui::Context,
}

#[derive(Default)]
struct DialogState {
    result: Option<Option<PathBuf>>,
    waker: Option<Waker>,
}

impl HostServices {
    fn dialog(&self, options: FileDialogOptions) -> DialogFuture {
        let ability = self.ability.clone();
        let grants = Arc::clone(&self.grants);
        let repaint = self.context.clone();
        let state = Arc::new(Mutex::new(DialogState::default()));
        let answer = Arc::clone(&state);
        // Both the bridge and grant activation stay off the egui thread. The future is woken
        // only when an answer arrives; it does not spin while the system picker is open.
        task::spawn(async move {
            let result = match files::pick(&ability, &grants, options).await {
                Ok(paths) => paths.into_iter().next(),
                Err(error) => {
                    log::warn!("Framune file dialog failed: {error}");
                    None
                }
            };
            let waker = {
                let mut answer = answer.lock().unwrap_or_else(PoisonError::into_inner);
                answer.result = Some(result);
                answer.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
            repaint.request_repaint();
        });
        Box::pin(std::future::poll_fn(move |context| {
            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(result) = state.result.take() {
                Poll::Ready(result)
            } else {
                state.waker = Some(context.waker().clone());
                Poll::Pending
            }
        }))
    }
}

impl Services for HostServices {
    fn branding(&self) -> Option<figcraft_ui_egui::Branding> {
        Some(figcraft_ui_egui::Branding {
            name: "Framune",
            icon_png: ICON,
            attribution: "Framune · 造隅。Based on FigCraft, copyright the original contributors. MIT OR Apache-2.0; see packaged licenses.",
        })
    }

    fn pick_open(&mut self, title: &str, extensions: &[&str]) -> DialogFuture {
        self.dialog(dialog_options("open-file", title, extensions))
    }

    fn pick_save(&mut self, title: &str, name: &str, extensions: &[&str]) -> DialogFuture {
        self.dialog(dialog_options("save-file", title, extensions).suggested_name(name))
    }

    fn pick_folder(&mut self, _title: &str) -> DialogFuture {
        self.dialog(FileDialogOptions::new("open-folder"))
    }

    fn set_egui_context(&mut self, context: &egui::Context) {
        self.context = context.clone();
    }

    fn clipboard_set_text(&mut self, text: &str) {
        figcraft_ui_egui::egui_copy_text(&self.context, text);
    }

    fn clipboard_set_png(&mut self, width: u32, height: u32, rgba: &[u8]) {
        figcraft_ui_egui::egui_copy_png(&self.context, width, height, rgba);
    }

    fn open_url(&mut self, url: &str) {
        if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:") {
            self.context.open_url(egui::OpenUrl::new_tab(url));
        } else {
            log::warn!("Only HTTP, HTTPS and mailto links can be opened");
        }
    }

    fn save_prefs(&mut self, prefs: &figcraft_engine::Prefs) {
        save_prefs(prefs, &self.directory);
    }

    fn system_dark(&self) -> Option<bool> {
        match self.ability.config().color_mode {
            openharmony_ability::ColorMode::Light => Some(false),
            _ => Some(true),
        }
    }
}

fn dialog_options(kind: &str, title: &str, extensions: &[&str]) -> FileDialogOptions {
    let options = FileDialogOptions::new(kind);
    if extensions.is_empty() { options } else { options.filters(vec![FileDialogFilter::new().name(title).pattern(extensions.join(";"))]) }
}

struct App {
    directory: PathBuf,
    title: String,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let title = windows::with_app(|fig| {
            windows::guard(fig, ctx, |fig| {
                fig.background_tick(ctx);
                windows::on_close_request(fig, ctx);
            });
            fig.title()
        });
        if let Some(title) = title
            && title != self.title
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(events) = windows::with_app(|fig| windows::take_input(fig, raw.viewport_id)) {
            raw.events.extend(events);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let _ = windows::with_app(|fig| windows::guard(fig, &ctx, |fig| fig.ui(ui)));
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        let _ = windows::with_app(|fig| save_prefs(&fig.session.prefs, &self.directory));
    }

    fn on_exit(&mut self) {
        let _ = windows::with_app(|fig| {
            save_prefs(&fig.session.prefs, &self.directory);
            drop(std::mem::take(&mut fig.session.live));
        });
    }
}

fn save_prefs(prefs: &figcraft_engine::Prefs, directory: &Path) {
    let result = serde_json::to_vec_pretty(prefs)
        .map_err(|error| error.to_string())
        .and_then(|bytes| files::write_atomic(&directory.join("prefs.json"), &bytes));
    if let Err(error) = result {
        log::warn!("Could not save Framune preferences: {error}");
    }
}

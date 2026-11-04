//! OpenHarmony Ability lifecycle host and platform integrations for Pagrune.

use super::files;
use openharmony_ability::OpenHarmonyApp;
use pdfcraft_ui_egui::{Clip, OhosClipboardHooks, OsEvent, PdfCraftApp};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, PoisonError},
    time::SystemTime,
};
use winit::platform::ohos::{
    events::PlatformEvent,
    plugins::clipboard::{ClipboardBridgePlugin, ClipboardClient},
    plugins::permission::PermissionExt as _,
    task::CommandQueue,
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
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("pagrune: {report}\n"));
        }
    }));
}

fn clipboard_revision(app: &OpenHarmonyApp) -> Option<u32> {
    app.registered_plugin::<ClipboardBridgePlugin>().ok().flatten()?.revision().ok()
}

fn decode_image_rgba(bytes: &[u8]) -> Option<Clip> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = reader.info().size();
    let (width, height) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    let expected = width.checked_mul(height)?.checked_mul(4)?;
    if width == 0 || height == 0 || width > 16384 || height > 16384 || expected > 64 * 1024 * 1024 {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let slice = buf.get(..info.buffer_size())?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => slice.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(expected);
            for &[r, g, b] in slice.as_chunks::<3>().0 {
                out.extend_from_slice(&[r, g, b, 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(expected);
            for &[g, a] in slice.as_chunks::<2>().0 {
                out.extend_from_slice(&[g, g, g, a]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(expected);
            for &g in slice {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == expected).then_some(Clip::Image { width, height, rgba })
}

type CachedClip = Option<(u32, Option<Clip>)>;

fn install_clipboard_hooks(ability: &OpenHarmonyApp) {
    let cache: Arc<Mutex<CachedClip>> = Arc::new(Mutex::new(None));
    let commands = Arc::new(CommandQueue::default());

    let read = {
        let app = ability.clone();
        let cache = Arc::clone(&cache);
        let commands = Arc::clone(&commands);
        Box::new(move || {
            let rev = clipboard_revision(&app)?;
            if let Some(cached) = cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .and_then(|(stored_rev, clip)| (*stored_rev == rev).then(|| clip.clone()))
            {
                return cached;
            }
            let app = app.clone();
            let cache = Arc::clone(&cache);
            commands.push(async move {
                let rev = clipboard_revision(&app);
                let result: Result<Option<Clip>, winit::platform::ohos::ability::napi_ohos::Error> = async {
                    let _ = app.request_permission("ohos.permission.READ_PASTEBOARD").await?;
                    let client = ClipboardClient::new(&app)?;
                    if let Ok(contents) = client.read_content().await {
                        let png_bytes = contents.png.or_else(|| {
                            contents
                                .uris
                                .iter()
                                .filter_map(|uri| winit::platform::ohos::drag_drop::file_path(uri))
                                .find_map(|path| std::fs::read(path).ok())
                        });
                        if let Some(bytes) = png_bytes
                            && let Some(clip) = decode_image_rgba(&bytes)
                        {
                            return Ok(Some(clip));
                        }
                    }
                    let text = client.read_text().await?.filter(|t| !t.trim().is_empty()).map(Clip::Text);
                    Ok(text)
                }
                .await;
                match result {
                    Ok(clip) => {
                        if let Some(r) = rev.filter(|r| Some(*r) == clipboard_revision(&app)) {
                            *cache.lock().unwrap_or_else(PoisonError::into_inner) = Some((r, clip));
                        }
                    }
                    Err(e) => log::warn!("OHOS pasteboard read failed: {e}"),
                }
            });
            None
        })
    };

    let set_image = {
        let app = ability.clone();
        let cache = Arc::clone(&cache);
        let commands = Arc::clone(&commands);
        Box::new(move |width: usize, height: usize, rgba: &[u8]| -> Result<(), String> {
            let (w, h) = (u32::try_from(width).map_err(|e| e.to_string())?, u32::try_from(height).map_err(|e| e.to_string())?);
            let rgba_vec = rgba.to_vec();
            let cached_clip = Clip::Image { width, height, rgba: rgba_vec.clone() };
            let app = app.clone();
            let cache = Arc::clone(&cache);
            commands.push(async move {
                let res = async { ClipboardClient::new(&app)?.write_image(&rgba_vec, w, h).await }.await;
                match res {
                    Ok(()) => {
                        if let Some(rev) = clipboard_revision(&app) {
                            *cache.lock().unwrap_or_else(PoisonError::into_inner) = Some((rev, Some(cached_clip)));
                        }
                    }
                    Err(e) => log::warn!("OHOS image pasteboard write failed: {e}"),
                }
            });
            Ok(())
        })
    };

    pdfcraft_ui_egui::set_ohos_clipboard_hooks(OhosClipboardHooks { read, set_image });
}

fn surface_texture_limits(mut required: eframe::wgpu::Limits, supported: &eframe::wgpu::Limits) -> eframe::wgpu::Limits {
    required = required.or_worse_values_from(supported);
    required.max_texture_dimension_2d = supported.max_texture_dimension_2d;
    required
}

pub(crate) fn start(ability: OpenHarmonyApp) -> Result<(), String> {
    let logger = install_logger();
    install_panic_hook();
    let directory = ability
        .pref_path()
        .or_else(|| ability.base_path())
        .map(PathBuf::from)
        .ok_or("Ability did not provide an application data directory")?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    if let Some(logger) = logger {
        match logger.attach_dir(&directory.join("logs")) {
            Ok(path) => log::info!("Pagrune {}, log file {}", pdfcraft_engine::build_info::version(), path.display()),
            Err(e) => log::warn!("no log file: {e}"),
        }
    }

    let grants = Arc::new(files::Grants::new(directory.clone()));
    let startup_grants = Arc::clone(&grants);
    let restored = std::thread::Builder::new()
        .name("pagrune-grants".into())
        .spawn(move || startup_grants.restore())
        .map_err(|error| error.to_string())?;

    {
        let grants = Arc::clone(&grants);
        let app = ability.clone();
        pdfcraft_ui_egui::set_ohos_dialog_handler(Arc::new(move |req| files::handle_dialog(&grants, &app, req)));
    }
    pdfcraft_ui_egui::set_ohos_drag_pointer(|ctx: &egui::Context| {
        if let Some((x, y)) = winit::platform::ohos::drag_drop::last_position() {
            let ppp = ctx.pixels_per_point();
            if ppp.is_finite() && ppp > 0.0 {
                return Some(egui::pos2(x as f32 / ppp, y as f32 / ppp));
            }
        }
        ctx.input(|i| i.pointer.latest_pos())
    });
    install_clipboard_hooks(&ability);

    let events = winit::platform::ohos::events::subscribe();
    let mut options = eframe::NativeOptions {
        openharmony_app: Some(ability.clone()),
        viewport: egui::ViewportBuilder::default()
            .with_title("Pagrune")
            .with_app_id("com.richerfu.pagrune")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true),
        persistence_path: Some(directory.join("app.ron")),
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
            descriptor.required_limits = surface_texture_limits(descriptor.required_limits, &adapter.limits());
            descriptor
        });
        if std::env::var_os("WGPU_BACKEND").is_none() {
            setup.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
    }

    restored.join().map_err(|_| "Could not restore saved file permissions")?;
    let result = eframe::run_native(
        "Pagrune",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(true);
            let mut app = PdfCraftApp::new();
            app.branding = Some(pdfcraft_ui_egui::Branding {
                name: "Pagrune",
                icon_png: include_bytes!("../../../../../branding/pagrune-128.png"),
                attribution: "Pagrune is an independent HarmonyOS adaptation of PdfCraft. MIT OR Apache-2.0. Upstream contributors retain their copyrights.",
            });
            app.units = pdfcraft_ui_egui::units::Unit::system();
            if let Some(json) = cc.storage.and_then(|s| s.get_string("pdfcraft")) {
                app.restore(&json);
            }
            app.enable_recovery(pdfcraft_ui_egui::RecoveryStore::new(directory.join("recovery")));
            let events = Mutex::new(events);
            let grants = Arc::clone(&grants);
            app.os_events = Some(Box::new(move || {
                let Ok(rx) = events.lock() else {
                    return Vec::new();
                };
                rx.try_iter()
                    .filter_map(|event| match event {
                        PlatformEvent::Want { uri, .. } if !uri.is_empty() => {
                            grants.resolve_want(&uri).map(|path| OsEvent::Open(vec![path.to_string_lossy().into_owned()]))
                        }
                        _ => None,
                    })
                    .collect()
            }));
            #[cfg(feature = "ohos-control")]
            {
                let client = app.attach_control(&cc.egui_ctx);
                match pdfcraft_ui_egui::control::serve(client) {
                    Ok(ep) => {
                        let control_path = directory.join("control.json");
                        let json = serde_json::json!({ "port": ep.port, "token": ep.token, "pid": std::process::id() }).to_string();
                        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                        let written =
                            std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(&control_path).and_then(
                                |mut file| {
                                    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
                                    std::io::Write::write_all(&mut file, json.as_bytes())
                                },
                            );
                        if let Err(e) = written {
                            log::warn!("could not write control file {}: {e}", control_path.display());
                        } else {
                            log::info!("UI control channel on 127.0.0.1:{} (connection details in {})", ep.port, control_path.display());
                        }
                    }
                    Err(e) => log::warn!("could not start UI control channel: {e}"),
                }
            }
            app.reopen_last_files(&[]);
            log::info!("Pagrune OpenHarmony workbench ready");
            Ok(Box::new(app))
        }),
    );
    result.map_err(|e| e.to_string())
}

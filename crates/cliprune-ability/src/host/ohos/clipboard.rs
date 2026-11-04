//! Typed pasteboard bridge; file URIs retain their grants across launches.

use filmcraft_ui_egui::ClipboardMedia;
use image::ImageDecoder as _;
use openharmony_ability::OpenHarmonyApp;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use winit::platform::ohos::plugins::{
    clipboard::{ClipboardBridgePlugin, ClipboardClient},
    permission::PermissionExt as _,
};

use super::files::Grants;

const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

type Cache = Arc<Mutex<Option<(u32, Option<ClipboardMedia>)>>>;

fn revision(app: &OpenHarmonyApp) -> Option<u32> {
    app.registered_plugin::<ClipboardBridgePlugin>().ok().flatten()?.revision().ok()
}

fn decode_png(bytes: &[u8]) -> Result<ClipboardMedia, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Clipboard image exceeds the 64 MiB limit".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(MAX_IMAGE_BYTES as u64);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|error| format!("Cannot read clipboard image: {error}"))?;
    let (width, height) = decoder.dimensions();
    let width = usize::try_from(width).map_err(|error| error.to_string())?;
    let height = usize::try_from(height).map_err(|error| error.to_string())?;
    let expected = width.checked_mul(height).and_then(|size| size.checked_mul(4)).ok_or("Clipboard image is too large")?;
    if width == 0 || height == 0 || expected > MAX_IMAGE_BYTES {
        return Err("Clipboard image is empty or exceeds the 64 MiB limit".into());
    }
    let rgba = image::DynamicImage::from_decoder(decoder).map_err(|error| format!("Cannot decode clipboard image: {error}"))?.to_rgba8();
    Ok(ClipboardMedia::Image { width, height, rgba: rgba.into_raw() })
}

fn clone_media(media: &ClipboardMedia) -> ClipboardMedia {
    match media {
        ClipboardMedia::Files(paths) => ClipboardMedia::Files(paths.clone()),
        ClipboardMedia::Image { width, height, rgba } => ClipboardMedia::Image { width: *width, height: *height, rgba: rgba.clone() },
    }
}

/// The UI hook is synchronous, so wait on the worker's reply only when a paste was requested.
/// All bridge ownership and image decoding remain on the worker, as in the sibling hosts.
pub fn service(
    app: OpenHarmonyApp,
    grants: Arc<Grants>,
    ctx: egui::Context,
    errors: mpsc::Sender<String>,
) -> Box<dyn FnMut() -> Option<ClipboardMedia>> {
    let cache: Cache = Arc::new(Mutex::new(None));
    Box::new(move || {
        let rev = revision(&app)?;
        if let Some((_, media)) = cache.lock().unwrap_or_else(PoisonError::into_inner).as_ref().filter(|(cached, _)| *cached == rev) {
            return media.as_ref().map(clone_media);
        }
        let (app, grants, cache) = (app.clone(), Arc::clone(&grants), Arc::clone(&cache));
        let response = super::jobs::spawn(
            "clipboard",
            async move {
                let permission = app.request_permission("ohos.permission.READ_PASTEBOARD").await.map_err(|error| error.to_string())?;
                if !permission.first().is_some_and(|result| result.code == 0) {
                    return Err("Pasteboard access was denied".into());
                }
                let content = ClipboardClient::new(&app)
                    .map_err(|error| error.to_string())?
                    .read_content()
                    .await
                    .map_err(|error| error.to_string())?;
                let media = if !content.uris.is_empty() {
                    Some(ClipboardMedia::Files(
                        grants.persist(&content.uris, false)?.into_iter().map(|path| path.to_string_lossy().into_owned()).collect(),
                    ))
                } else {
                    content.png.as_deref().map(decode_png).transpose()?
                };
                if revision(&app) == Some(rev) {
                    *cache.lock().unwrap_or_else(PoisonError::into_inner) = Some((rev, media.as_ref().map(clone_media)));
                }
                Ok(media)
            },
            ctx.clone(),
            errors.clone(),
        );
        match response.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(media) => media,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = errors.send("Pasteboard did not respond; please retry Paste".into());
                None
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => None,
        }
    })
}

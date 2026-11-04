//! Inject OpenHarmony services without compiling any desktop platform module.

use openharmony_ability::OpenHarmonyApp;
use std::{
    path::Path,
    sync::{Arc, Mutex, PoisonError},
};
use vectorcraft_engine::cmd::{fileio, recovery::FolderStore};
use vectorcraft_ui_egui::{
    FilePick, Inbox, Services,
    picks::PickRequest,
    place::{PlaceArrival, PlaceInbox},
};
use winit::platform::ohos::{plugins::url::UrlExt as _, task};

use super::{
    clipboard,
    files::{self, Errors, Grants},
};

pub fn build(ability: &OpenHarmonyApp, grants: &Arc<Grants>, directory: &Path, ctx: &egui::Context, errors: &Errors) -> Services {
    let inbox: Inbox = Arc::new(Mutex::new(Vec::new()));
    let place_inbox: PlaceInbox = Arc::new(Mutex::new(Vec::new()));
    let open_async = {
        let (ability, grants, ctx, inbox, errors) = (ability.clone(), grants.clone(), ctx.clone(), inbox.clone(), errors.clone());
        Box::new(move || {
            let (ability, grants, ctx, inbox, errors) = (ability.clone(), grants.clone(), ctx.clone(), inbox.clone(), errors.clone());
            task::spawn(async move {
                let pick = FilePick { filters: FilePick::filters(fileio::open_filters()), ..Default::default() };
                match files::pick(&ability, &grants, PickRequest::Open(pick)).await {
                    Ok(paths) => {
                        for path in paths {
                            match files::read(&path) {
                                Ok(bytes) => inbox.lock().unwrap_or_else(PoisonError::into_inner).push((path, bytes)),
                                Err(error) => files::report(&errors, error),
                            }
                        }
                    }
                    Err(error) => files::report(&errors, error),
                }
                ctx.request_repaint();
            });
        })
    };
    let place_async = {
        let (ability, grants, ctx, inbox, errors) = (ability.clone(), grants.clone(), ctx.clone(), place_inbox.clone(), errors.clone());
        Box::new(move || {
            let (ability, grants, ctx, inbox, errors) = (ability.clone(), grants.clone(), ctx.clone(), inbox.clone(), errors.clone());
            task::spawn(async move {
                match files::pick(&ability, &grants, PickRequest::OpenMany).await {
                    Ok(paths) => {
                        for path in paths {
                            match files::read(&path) {
                                Ok(bytes) => inbox.lock().unwrap_or_else(PoisonError::into_inner).push(PlaceArrival {
                                    name: path,
                                    bytes,
                                    drop: None,
                                }),
                                Err(error) => files::report(&errors, error),
                            }
                        }
                    }
                    Err(error) => files::report(&errors, error),
                }
                ctx.request_repaint();
            });
        })
    };
    let writer: vectorcraft_ui_egui::background::SharedWriteFn = {
        let (ability, grants) = (ability.clone(), grants.clone());
        Arc::new(move |path, bytes| {
            files::authorize(&ability, &grants, path)?;
            vectorcraft_format::write_atomic(Path::new(path), bytes).map_err(|e| e.to_string())
        })
    };
    let stream: vectorcraft_ui_egui::background::SharedStreamFn = {
        let (ability, grants) = (ability.clone(), grants.clone());
        Arc::new(move |path, fill| {
            files::authorize(&ability, &grants, path)?;
            vectorcraft_format::write_atomic_with(Path::new(path), |file| fill(file)).map_err(|e| e.to_string())
        })
    };
    let url_ability = ability.clone();
    let url_errors = errors.clone();
    let probe_ability = ability.clone();
    let probe_errors = errors.clone();
    Services {
        branding: Some(vectorcraft_ui_egui::Branding {
            name: "Vecrune",
            icon_png: include_bytes!("../../../../branding/vecrune-128.png"),
            attribution: "Vecrune is an independent HarmonyOS adaptation of VectorCraft. MIT OR Apache-2.0. Upstream contributors retain their copyrights. Fonts: Source Sans 3, Inter, JetBrains Mono (OFL). Icons: Lucide (ISC).",
        }),
        start_pick: Some(files::start_pick(ability.clone(), grants.clone(), ctx.clone(), errors.clone())),
        dialogs_are_sheets: true,
        open_async: Some(open_async),
        inbox: Some(inbox),
        place_async: Some(place_async),
        place_inbox: Some(place_inbox),
        read: Some(Box::new(files::read)),
        write: Some(Box::new({
            let writer = writer.clone();
            move |path, bytes| writer(path, bytes)
        })),
        write_shared: Some(writer),
        stream_shared: Some(stream),
        exists: Some(Box::new(|path| Path::new(path).exists())),
        system_clipboard: Some(Box::new(clipboard::Clipboard::new(ability.clone(), errors.clone()))),
        clipboard_probe: Some(Box::new(move || Box::new(clipboard::Clipboard::new(probe_ability, probe_errors)))),
        clipboard_read: Some(clipboard::text_reader(ability.clone(), errors.clone())),
        open_url: Some(Box::new(move |url| {
            let future = url_ability.open_url(url);
            let errors = url_errors.clone();
            task::spawn(async move {
                if let Err(error) = future.await {
                    files::report(&errors, format!("Open URL failed: {error}"));
                }
            });
        })),
        recovery_store: Some(Arc::new(FolderStore::new(directory.join("recovery")))),
        ..Default::default()
    }
}

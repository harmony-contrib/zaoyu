//! GridCraft's existing host service hooks, backed by OpenHarmony Ability plugins.

use super::{clipboard::Clipboard, files};
use gridcraft_ui_egui::{Inbox, Services};
use openharmony_ability::OpenHarmonyApp;
use std::sync::Arc;
use winit::platform::ohos::{plugins::url::UrlExt as _, task};

pub fn build(ability: &OpenHarmonyApp, grants: &Arc<files::Grants>, ctx: &egui::Context) -> Services {
    let inbox = Inbox::default();
    let open_inbox = Arc::clone(&inbox);
    let open_ability = ability.clone();
    let open_grants = Arc::clone(grants);
    let open_ctx = ctx.clone();
    let picture_ability = ability.clone();
    let picture_grants = Arc::clone(grants);
    let picture_ctx = ctx.clone();
    let save_ability = ability.clone();
    let save_grants = Arc::clone(grants);
    let url_ability = ability.clone();
    let clipboard = Clipboard::new(ability.clone());
    let html = clipboard.clone();
    let text = clipboard.clone();
    Services {
        branding: Some(gridcraft_ui_egui::Branding {
            name: "Gridrune",
            icon_png: include_bytes!("../../../../../branding/gridrune-128.png"),
            attribution: "Gridrune is an independent HarmonyOS adaptation of GridCraft. MIT OR Apache-2.0. Copyright 2026 ArtCraft Team and the GridCraft contributors. Upstream contributor and model credits are retained.",
        }),
        open_async: Some(Box::new(move || {
            files::open_async(open_ability.clone(), Arc::clone(&open_grants), Arc::clone(&open_inbox), open_ctx.clone(), false);
        })),
        pick_picture_async: Some(Box::new(move |inbox| {
            files::open_async(picture_ability.clone(), Arc::clone(&picture_grants), inbox, picture_ctx.clone(), true);
        })),
        inbox: Some(inbox),
        pick_save: Some(Box::new(move |suggested| files::pick_save(&save_ability, &save_grants, suggested))),
        copy_html: Some(Box::new(move |html, text| clipboard.copy_html(html, text))),
        paste_html: Some(Box::new(move || html.read("text/html"))),
        paste_text: Some(Box::new(move || text.read("text/plain"))),
        open_url: Some(Box::new(move |url| {
            let gridcraft_engine::links::LinkKind::External(url) = gridcraft_engine::links::classify(url) else {
                log::warn!("Only http, https and mailto links can be opened");
                return;
            };
            let future = url_ability.open_url(url.as_str());
            task::spawn(async move {
                if let Err(error) = future.await {
                    log::warn!("Open URL failed: {error}");
                }
            });
        })),
        ..Default::default()
    }
}

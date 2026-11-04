//! Native pasteboard formats through the same Ability plugins as the sibling apps.

use openharmony_ability::OpenHarmonyApp;
use std::sync::{
    Arc, Mutex, PoisonError,
    atomic::{AtomicUsize, Ordering},
};
use vectorcraft_engine::cmd::clipboard::{Flavour, TEXT};
use vectorcraft_ui_egui::{ClipboardRead, SystemClipboard};
use winit::platform::ohos::{
    plugins::{
        clipboard::{ClipboardBridgePlugin, ClipboardClient, ClipboardMimeContent},
        permission::PermissionExt as _,
    },
    task::CommandQueue,
};

use super::files::{self, Errors, block_on};

pub struct Clipboard {
    ability: OpenHarmonyApp,
    commands: Arc<CommandQueue>,
    revision: Arc<Mutex<Option<u32>>>,
    pending: Arc<AtomicUsize>,
    errors: Errors,
}

fn revision(ability: &OpenHarmonyApp) -> Option<u32> {
    ability.registered_plugin::<ClipboardBridgePlugin>().ok().flatten()?.revision().ok()
}

impl Clipboard {
    pub fn new(ability: OpenHarmonyApp, errors: Errors) -> Self {
        Self { ability, errors, commands: Arc::default(), revision: Arc::default(), pending: Arc::default() }
    }
}

fn read(ability: &OpenHarmonyApp, mimes: &[&'static str]) -> Result<Option<Flavour>, String> {
    block_on(async {
        ability.request_permission("ohos.permission.READ_PASTEBOARD").await.map_err(|e| e.to_string())?;
        let client = ClipboardClient::new(ability).map_err(|e| e.to_string())?;
        let requested = mimes.iter().map(|mime| (*mime).to_owned()).collect();
        let response = client.read_mime(requested, false).await.map_err(|e| e.to_string())?;
        Ok(mimes.iter().find_map(|mime| {
            response
                .contents
                .iter()
                .find(|content| content.mime == *mime || *mime == "image/*" && content.mime.starts_with("image/"))
                .map(|content| Flavour { mime, data: content.data.clone() })
        }))
    })
}

impl SystemClipboard for Clipboard {
    fn write(&mut self, flavours: &[Flavour]) -> Result<(), String> {
        let contents = flavours.iter().map(|f| ClipboardMimeContent { mime: f.mime.into(), data: f.data.clone() }).collect();
        let (ability, revision_slot, errors) = (self.ability.clone(), self.revision.clone(), self.errors.clone());
        let pending = self.pending.clone();
        pending.fetch_add(1, Ordering::SeqCst);
        *self.revision.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.commands.push(async move {
            let result = async { ClipboardClient::new(&ability)?.write_mime(contents).await }.await;
            match result {
                Ok(()) => *revision_slot.lock().unwrap_or_else(PoisonError::into_inner) = revision(&ability),
                Err(error) => files::report(&errors, format!("Clipboard write failed: {error}")),
            }
            pending.fetch_sub(1, Ordering::SeqCst);
        });
        Ok(())
    }

    fn holds_ours(&mut self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
            || self.revision.lock().unwrap_or_else(PoisonError::into_inner).is_some_and(|ours| Some(ours) == revision(&self.ability))
    }

    fn read(&mut self, mimes: &[&'static str]) -> Option<Flavour> {
        match read(&self.ability, mimes) {
            Ok(flavour) => flavour,
            Err(error) => {
                files::report(&self.errors, format!("Clipboard read failed: {error}"));
                None
            }
        }
    }

    fn read_deferred(&mut self, mimes: &[&'static str]) -> Option<ClipboardRead> {
        let (ability, errors, mimes) = (self.ability.clone(), self.errors.clone(), mimes.to_vec());
        Some(ClipboardRead::Later(Box::new(move || match read(&ability, &mimes) {
            Ok(flavour) => flavour,
            Err(error) => {
                files::report(&errors, format!("Clipboard read failed: {error}"));
                None
            }
        })))
    }

    fn has(&mut self, mimes: &[&'static str]) -> bool {
        // This is used by the UI's background probe. Metadata requires no paste permission.
        block_on(async {
            let client = ClipboardClient::new(&self.ability).ok()?;
            let response = client.read_mime(Vec::new(), true).await.ok()?;
            Some(
                response
                    .available
                    .iter()
                    .any(|available| mimes.iter().any(|mime| available == mime || *mime == "image/*" && available.starts_with("image/"))),
            )
        })
        .unwrap_or(false)
    }
}

pub fn text_reader(ability: OpenHarmonyApp, errors: Errors) -> Box<dyn FnMut() -> Option<String>> {
    let mut clipboard = Clipboard::new(ability, errors);
    Box::new(move || clipboard.read(&[TEXT]).and_then(|flavour| String::from_utf8(flavour.data).ok()))
}

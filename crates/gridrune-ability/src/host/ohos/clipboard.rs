//! Ordered pasteboard operations through the same Ability plugins as PhotoCraft/PdfCraft.

use openharmony_ability::OpenHarmonyApp;
use std::sync::Arc;
use winit::platform::ohos::{
    plugins::{
        clipboard::{ClipboardClient, ClipboardMimeContent},
        permission::PermissionExt as _,
    },
    task::CommandQueue,
};

#[derive(Clone)]
pub struct Clipboard {
    ability: OpenHarmonyApp,
    commands: Arc<CommandQueue>,
}

impl Clipboard {
    pub fn new(ability: OpenHarmonyApp) -> Self {
        Self { ability, commands: Arc::default() }
    }

    pub fn copy_html(&self, html: &str, text: &str) -> Result<(), String> {
        if html.len().saturating_add(text.len()) > 16 * 1024 * 1024 {
            return Err("Clipboard content exceeds 16 MB".into());
        }
        let contents = vec![
            ClipboardMimeContent { mime: "text/html".into(), data: html.as_bytes().to_vec() },
            ClipboardMimeContent { mime: "text/plain".into(), data: text.as_bytes().to_vec() },
        ];
        let ability = self.ability.clone();
        self.commands.push(async move {
            let result = async { ClipboardClient::new(&ability)?.write_mime(contents).await }.await;
            if let Err(error) = result {
                log::warn!("OHOS pasteboard write failed: {error}");
            }
        });
        Ok(())
    }

    pub fn read(&self, mime: &'static str) -> Option<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let ability = self.ability.clone();
        self.commands.push(async move {
            let result: Result<Option<String>, String> = async {
                ability.request_permission("ohos.permission.READ_PASTEBOARD").await.map_err(|error| error.to_string())?;
                let response = ClipboardClient::new(&ability)
                    .map_err(|error| error.to_string())?
                    .read_mime(vec![mime.into()], false)
                    .await
                    .map_err(|error| error.to_string())?;
                response
                    .contents
                    .into_iter()
                    .find(|content| content.mime == mime)
                    .map(|content| {
                        if content.data.len() > 16 * 1024 * 1024 {
                            return Err("Clipboard content exceeds 16 MB".into());
                        }
                        String::from_utf8(content.data).map_err(|error| error.to_string())
                    })
                    .transpose()
            }
            .await;
            let _ = tx.send(result);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(Ok(text)) => text,
            Ok(Err(error)) => {
                log::warn!("OHOS pasteboard read failed: {error}");
                None
            }
            Err(error) => {
                log::warn!("OHOS pasteboard read did not complete: {error}");
                None
            }
        }
    }
}

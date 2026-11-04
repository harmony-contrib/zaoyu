//! Vecrune native entry and HarmonyOS services in one application crate.
#![cfg(target_env = "ohos")]

mod host;

#[openharmony_ability_derive::ability]
fn openharmony(app: openharmony_ability::OpenHarmonyApp) {
    if let Err(error) = host::start(app) {
        log::error!("Vecrune startup failed: {error}");
    }
}

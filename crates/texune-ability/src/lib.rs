//! HarmonyOS Ability registration and the native Texune editor host.
#![cfg(target_env = "ohos")]

mod host;

#[openharmony_ability_derive::ability]
fn openharmony(app: openharmony_ability::OpenHarmonyApp) {
    if let Err(error) = host::start(app) {
        log::error!("Texune startup failed: {error}");
    }
}

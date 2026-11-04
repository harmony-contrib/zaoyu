//! HarmonyOS Ability registration and the native Shadrune editor host.
#![cfg(target_env = "ohos")]

mod host;

#[openharmony_ability_derive::ability]
fn openharmony(app: openharmony_ability::OpenHarmonyApp) {
    if let Err(error) = host::start(app) {
        log::error!("Shadrune startup failed: {error}");
    }
}

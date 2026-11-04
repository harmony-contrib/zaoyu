//! N-API registration is isolated from the safe editor host.
#![cfg(target_env = "ohos")]

#[openharmony_ability_derive::ability]
fn openharmony(app: openharmony_ability::OpenHarmonyApp) {
    if let Err(error) = vecrune_host::start(app) {
        log::error!("Vecrune startup failed: {error}");
    }
}

//! Native Ability registration with an isolated safe editor host module.
#![cfg(target_env = "ohos")]

mod host;

#[openharmony_ability_derive::ability]
fn openharmony(app: openharmony_ability::OpenHarmonyApp) {
    if let Err(error) = host::start(app) {
        log::error!("Pixrune startup failed: {error}");
    }
}

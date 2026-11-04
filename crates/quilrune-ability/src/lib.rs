//! HarmonyOS Ability registration and the safe Quilrune editor host.
#![cfg(target_env = "ohos")]

// The N-API macro generates the FFI boundary; the host forbids unsafe code.
mod host;

#[openharmony_ability_derive::ability]
fn openharmony(app: openharmony_ability::OpenHarmonyApp) {
    if let Err(error) = host::start(app) {
        log::error!("Quilrune startup failed: {error}");
    }
}

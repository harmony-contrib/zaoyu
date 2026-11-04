//! HarmonyOS host for the pinned ArtCraft Studio application, distributed as Visrune.
//!
//! The host supplies lifecycle, sandbox storage and branding to the upstream UI.
#![forbid(unsafe_code)]

mod logging;
mod ohos;

/// Start the editor with the Ability's lifecycle and sandbox context.
pub(super) fn start(ability: openharmony_ability::OpenHarmonyApp) -> Result<(), String> {
    ohos::start(ability)
}

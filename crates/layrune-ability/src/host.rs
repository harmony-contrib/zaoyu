//! HarmonyOS host for the pinned DesignCraft editor, distributed as Layrune.
//!
//! This module owns lifecycle, permissions, persistence and OS services. Engine and
//! editor UI dependencies remain in the pinned upstream workspace.
#![forbid(unsafe_code)]

#[cfg(feature = "ohos-control")]
mod control_server;
#[allow(dead_code)]
mod logging;
mod ohos;

/// Startup failure at the native host boundary; upstream service details are retained.
#[derive(Debug)]
pub(super) struct StartupError {
    detail: String,
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for StartupError {}

/// Start the editor with the Ability's lifecycle and sandbox context.
pub(super) fn start(ability: openharmony_ability::OpenHarmonyApp) -> Result<(), StartupError> {
    ohos::start(ability).map_err(|detail| StartupError { detail })
}

//! Platform services and lifecycle composition.

mod clipboard;
mod files;
mod logging;
mod persistence;
mod runtime;

pub(super) use runtime::start;

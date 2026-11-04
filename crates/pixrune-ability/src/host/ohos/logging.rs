//! HiLog and rotating application log files, following the sibling hosts.
use std::{sync::OnceLock, time::SystemTime};

struct OhosLogger(crate::host::logging::AppLogger);

impl log::Log for OhosLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.0.enabled(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        self.0.log(record);
        let thread = std::thread::current();
        let line = crate::host::logging::format_line(
            &crate::host::logging::timestamp(SystemTime::now()),
            record.level(),
            thread.name().unwrap_or("?"),
            record.target(),
            &record.args().to_string(),
        );
        let hilog_msg = line.trim_end_matches('\n').replace('\0', "");
        match record.level() {
            log::Level::Error => ohos_hilog_binding::error(hilog_msg),
            log::Level::Warn => ohos_hilog_binding::warn(hilog_msg),
            log::Level::Info => ohos_hilog_binding::info(hilog_msg),
            log::Level::Debug | log::Level::Trace => ohos_hilog_binding::debug(hilog_msg),
        }
    }

    fn flush(&self) {
        self.0.flush();
    }
}

pub fn install_logger() -> Option<&'static crate::host::logging::AppLogger> {
    static LOGGER: OnceLock<OhosLogger> = OnceLock::new();
    let spec = std::env::var("RUST_LOG").ok().filter(|s| !s.trim().is_empty());
    let filter = crate::host::logging::Filter::parse(spec.as_deref().unwrap_or(crate::host::logging::DEFAULT_FILTER));
    let max = filter.max();
    let logger = LOGGER.get_or_init(|| OhosLogger(crate::host::logging::AppLogger::new(filter, true)));
    log::set_logger(logger).ok()?;
    log::set_max_level(max);
    Some(&logger.0)
}

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let trace = std::backtrace::Backtrace::capture();
        let report = if trace.status() == std::backtrace::BacktraceStatus::Captured {
            format!("internal error: {info}\n{trace}")
        } else {
            format!("internal error: {info}")
        };
        if log::log_enabled!(log::Level::Error) {
            log::error!("{report}");
        } else {
            // Before a logger exists, stderr is a best-effort last resort for the report.
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("pixrune: {report}\n"));
        }
    }));
}

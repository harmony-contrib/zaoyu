//! Forward native host and upstream diagnostics to HarmonyOS HiLog.

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        let application = metadata.target().starts_with("quilrune") || metadata.target().starts_with("wordcraft");
        metadata.level() <= if application { log::Level::Info } else { log::Level::Warn }
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = format!(
            "{} {} {}: {}",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            record.level(),
            record.target(),
            record.args(),
        )
        .replace('\0', "");
        match record.level() {
            log::Level::Error => ohos_hilog_binding::error(message),
            log::Level::Warn => ohos_hilog_binding::warn(message),
            log::Level::Info => ohos_hilog_binding::info(message),
            log::Level::Debug | log::Level::Trace => ohos_hilog_binding::debug(message),
        }
    }

    fn flush(&self) {}
}

pub(super) fn install() {
    static LOGGER: Logger = Logger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    std::panic::set_hook(Box::new(|info| {
        log::error!("internal error: {info}\n{}", std::backtrace::Backtrace::capture());
    }));
}

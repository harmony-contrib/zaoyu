//! Route Rust diagnostics to the Ability's HiLog stream.

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} Lutrune {} [{}] {}",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            record.level(),
            record.target(),
            record.args(),
        )
        .replace('\0', "");
        match record.level() {
            log::Level::Error => ohos_hilog_binding::error(line),
            log::Level::Warn => ohos_hilog_binding::warn(line),
            log::Level::Info => ohos_hilog_binding::info(line),
            log::Level::Debug | log::Level::Trace => ohos_hilog_binding::debug(line),
        }
    }

    fn flush(&self) {}
}

pub(super) fn install() {
    static LOGGER: Logger = Logger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

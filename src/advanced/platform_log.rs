//! The `log` crate's backend. GPUI and its platform crates report through
//! `log`, and without a backend every line goes nowhere, Windows giving up on
//! a lost GPU device among them. Warnings and errors go to the small always-on
//! `logs/platform.log`.

use super::debug_mode;
use log::{Level, LevelFilter, Log, Metadata, Record};

struct PlatformLog;

static LOGGER: PlatformLog = PlatformLog;

/// Installs the backend. Calls after the first do nothing.
pub fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Warn);
    }
}

impl Log for PlatformLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Warn
    }

    /// `log_err` calls this directly, past the crate's level check, and leaves
    /// the target empty here, since it finds it in a `crates/` path segment.
    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        debug_mode::platform_log(&format!(
            "{} {}:{} {}",
            record.level(),
            record.file().unwrap_or(record.target()),
            record.line().unwrap_or(0),
            record.args(),
        ));
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logged() -> String {
        let root = crate::settings::safe_app_root().unwrap();
        std::fs::read_to_string(debug_mode::platform_log_path(&root)).unwrap_or_default()
    }

    fn send(level: Level, message: &str) {
        PlatformLog.log(
            &Record::builder()
                .level(level)
                .target("")
                .file(Some("third_party/gpui-pre-windows/src/platform.rs"))
                .line(Some(7))
                .args(format_args!("{message}"))
                .build(),
        );
    }

    #[test]
    fn an_error_from_log_err_reaches_the_file() {
        send(Level::Error, "the device is gone");
        assert!(
            logged().contains(
                "ERROR third_party/gpui-pre-windows/src/platform.rs:7 the device is gone"
            )
        );
    }

    #[test]
    fn a_warning_reaches_the_file() {
        send(Level::Warn, "a retry");
        assert!(logged().contains("WARN third_party/gpui-pre-windows/src/platform.rs:7 a retry"));
    }

    #[test]
    fn info_lines_stay_out() {
        send(Level::Info, "routine");
        assert!(!logged().contains("routine"));
    }
}

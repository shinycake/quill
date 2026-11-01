use super::debug_mode;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::time::{Duration, Instant};

/// A site that keeps logging writes one line per window, so an error raised
/// every frame can't rotate the first, useful line out of the file.
const REPEAT_WINDOW: Duration = Duration::from_secs(60);

/// The queue never makes the caller wait, because the caller is usually the
/// thread that draws. A full queue drops the line.
const QUEUE_LINES: usize = 256;

const WRITER_THREAD: &str = "platform-log";

static LOGGER: PlatformLog = PlatformLog::new(debug_mode::platform_log, Instant::now);

pub fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Warn);
    }
}

struct PlatformLog {
    queue: OnceLock<Option<SyncSender<Message>>>,
    write: fn(&str),
    now: fn() -> Instant,
}

enum Message {
    Line(Line),
    Flush(Sender<()>),
}

struct Line {
    site: String,
    text: String,
    at: Instant,
}

impl PlatformLog {
    const fn new(write: fn(&str), now: fn() -> Instant) -> Self {
        Self {
            queue: OnceLock::new(),
            write,
            now,
        }
    }

    fn queue(&self) -> Option<&SyncSender<Message>> {
        self.queue.get_or_init(|| spawn_writer(self.write)).as_ref()
    }
}

/// The writer exists from the first warning on, so a run without one has no
/// extra thread.
fn spawn_writer(write: fn(&str)) -> Option<SyncSender<Message>> {
    let (queue, messages) = mpsc::sync_channel(QUEUE_LINES);
    std::thread::Builder::new()
        .name(WRITER_THREAD.to_owned())
        .spawn(move || {
            let mut repeats = Repeats::default();
            for message in messages {
                match message {
                    Message::Line(line) => {
                        if let Some(text) = repeats.admit(line) {
                            write(&text);
                        }
                    }
                    Message::Flush(done) => {
                        let _ = done.send(());
                    }
                }
            }
        })
        .ok()?;
    Some(queue)
}

#[derive(Default)]
struct Repeats {
    sites: HashMap<String, Site>,
}

struct Site {
    written_at: Instant,
    held_back: u64,
}

impl Repeats {
    /// The text to write for `line`, or `None` inside its site's window. The
    /// line after a window counts what the window held back.
    fn admit(&mut self, line: Line) -> Option<String> {
        let Some(site) = self.sites.get_mut(&line.site) else {
            self.sites.insert(
                line.site,
                Site {
                    written_at: line.at,
                    held_back: 0,
                },
            );
            return Some(line.text);
        };
        if line.at.saturating_duration_since(site.written_at) < REPEAT_WINDOW {
            site.held_back = site.held_back.saturating_add(1);
            return None;
        }
        site.written_at = line.at;
        match std::mem::take(&mut site.held_back) {
            0 => Some(line.text),
            held_back => Some(format!("{} (and {held_back} more like it)", line.text)),
        }
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
        let Some(queue) = self.queue() else {
            return;
        };
        let site = format!(
            "{}:{}",
            record.file().unwrap_or(record.target()),
            record.line().unwrap_or(0),
        );
        let text = format!("{} {site} {}", record.level(), record.args());
        let at = (self.now)();
        let _ = queue.try_send(Message::Line(Line { site, text, at }));
    }

    /// Blocks until the writer has written every line queued before the call.
    /// The writer itself can't wait on its own queue, so a panic on that
    /// thread skips the flush instead of hanging the panic hook.
    fn flush(&self) {
        if std::thread::current().name() == Some(WRITER_THREAD) {
            return;
        }
        let Some(queue) = self.queue.get().and_then(Option::as_ref) else {
            return;
        };
        let (done, finished) = mpsc::channel();
        if queue.send(Message::Flush(done)).is_ok() {
            let _ = finished.recv();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::{self, ThreadId};

    const FILE: &str = "third_party/gpui-pre-windows/src/platform.rs";

    fn logged() -> String {
        LOGGER.flush();
        let root = crate::settings::safe_app_root().unwrap();
        std::fs::read_to_string(debug_mode::platform_log_path(&root)).unwrap_or_default()
    }

    fn send(logger: &PlatformLog, level: Level, line: u32, message: &str) {
        logger.log(
            &Record::builder()
                .level(level)
                .target("")
                .file(Some(FILE))
                .line(Some(line))
                .args(format_args!("{message}"))
                .build(),
        );
    }

    fn frozen_clock() -> Instant {
        static AT: OnceLock<Instant> = OnceLock::new();
        *AT.get_or_init(Instant::now)
    }

    #[test]
    fn an_error_from_log_err_reaches_the_file() {
        send(&LOGGER, Level::Error, 7, "the device is gone");
        assert!(logged().contains(&format!("ERROR {FILE}:7 the device is gone")));
    }

    #[test]
    fn a_warning_reaches_the_file() {
        send(&LOGGER, Level::Warn, 8, "a retry");
        assert!(logged().contains(&format!("WARN {FILE}:8 a retry")));
    }

    #[test]
    fn info_lines_stay_out() {
        send(&LOGGER, Level::Info, 9, "routine");
        assert!(!logged().contains("routine"));
    }

    #[test]
    fn a_line_is_written_off_the_thread_that_logged_it() {
        static WRITTEN_ON: Mutex<Option<ThreadId>> = Mutex::new(None);
        let logger = PlatformLog::new(
            |_| *WRITTEN_ON.lock().unwrap() = Some(thread::current().id()),
            frozen_clock,
        );

        send(&logger, Level::Error, 1, "drawn on this thread");
        logger.flush();

        let written_on = WRITTEN_ON.lock().unwrap().unwrap();
        assert_ne!(written_on, thread::current().id());
    }

    #[test]
    fn a_burst_from_one_site_writes_one_line() {
        static LINES: AtomicUsize = AtomicUsize::new(0);
        let logger = PlatformLog::new(
            |_| {
                LINES.fetch_add(1, Ordering::Relaxed);
            },
            frozen_clock,
        );

        for _ in 0..50 {
            send(&logger, Level::Error, 1, "failed to render");
        }
        logger.flush();

        assert_eq!(LINES.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn flush_returns_after_every_queued_line_is_written() {
        static LINES: AtomicUsize = AtomicUsize::new(0);
        let logger = PlatformLog::new(
            |_| {
                LINES.fetch_add(1, Ordering::Relaxed);
            },
            frozen_clock,
        );

        for site in 1..=100 {
            send(&logger, Level::Error, site, "the renderer failed");
        }
        logger.flush();

        assert_eq!(LINES.load(Ordering::Relaxed), 100);
    }

    fn line(site: &str, at: Instant) -> Line {
        Line {
            site: site.to_owned(),
            text: format!("text from {site}"),
            at,
        }
    }

    #[test]
    fn a_site_that_keeps_logging_writes_one_line_a_window_with_its_count() {
        let start = Instant::now();
        let mut repeats = Repeats::default();
        let mut admit = |after: Duration| repeats.admit(line("a.rs:1", start + after));

        assert_eq!(admit(Duration::ZERO).as_deref(), Some("text from a.rs:1"));
        assert_eq!(admit(Duration::from_secs(1)), None);
        assert_eq!(admit(Duration::from_secs(59)), None);
        assert_eq!(
            admit(REPEAT_WINDOW).as_deref(),
            Some("text from a.rs:1 (and 2 more like it)")
        );
        assert_eq!(admit(REPEAT_WINDOW + Duration::from_secs(1)), None);
    }

    #[test]
    fn a_site_quiet_for_a_window_writes_its_plain_line() {
        let start = Instant::now();
        let mut repeats = Repeats::default();

        repeats.admit(line("a.rs:1", start));

        assert_eq!(
            repeats
                .admit(line("a.rs:1", start + REPEAT_WINDOW))
                .as_deref(),
            Some("text from a.rs:1")
        );
    }

    #[test]
    fn one_site_does_not_hold_back_another() {
        let start = Instant::now();
        let mut repeats = Repeats::default();

        repeats.admit(line("a.rs:1", start));

        assert_eq!(
            repeats.admit(line("a.rs:2", start)).as_deref(),
            Some("text from a.rs:2")
        );
    }
}

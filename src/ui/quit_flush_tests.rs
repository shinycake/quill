use gpui_kit::TestAppContext;
use log::{Log, Metadata, Record};
use std::sync::atomic::{AtomicUsize, Ordering};

static FLUSHES: AtomicUsize = AtomicUsize::new(0);

struct FlushCounter;

impl Log for FlushCounter {
    fn enabled(&self, _: &Metadata) -> bool {
        false
    }

    fn log(&self, _: &Record) {}

    fn flush(&self) {
        FLUSHES.fetch_add(1, Ordering::Relaxed);
    }
}

/// macOS exits from `applicationWillTerminate`, so the app's `shutdown` is the
/// last Quill code that runs on a normal quit.
#[gpui_kit::test]
fn shutdown_flushes_the_log(cx: &mut TestAppContext) {
    log::set_logger(&FlushCounter).expect("no other test installs a logger");

    cx.update(|cx| cx.shutdown());

    assert_eq!(FLUSHES.load(Ordering::Relaxed), 1);
}

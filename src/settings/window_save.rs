//! Off-thread, latest-wins saving of the main-window geometry.
//!
//! A drag or resize reports a new geometry many times a second and the
//! atomic write fsyncs, which can stall for tens of milliseconds on
//! Windows or a slow disk. The UI thread only stages the newest state
//! (a mutex-guarded slot); a short-lived writer thread takes whatever is
//! staged and writes it, so intermediate states are dropped. Quitting
//! flushes synchronously.

use super::{WindowState, safe_app_root, write_json_atomic};
use std::path::Path;
use std::sync::Mutex;

/// The staged state, the write lock and the background writer. The writer
/// holds `write` before it takes `staged`, so a flush that gets `write` sees
/// every earlier write finished.
struct Saver {
    staged: Mutex<Option<WindowState>>,
    write: Mutex<()>,
    writer: Mutex<Writer>,
}

/// The background writer: none, or one thread that keeps taking the staged
/// state until the slot is empty (or a write failed), then leaves for `Idle`
/// itself. A save while it runs only stages.
enum Writer {
    Idle,
    Running { thread: std::thread::JoinHandle<()> },
}

static SAVER: Saver = Saver {
    staged: Mutex::new(None),
    write: Mutex::new(()),
    writer: Mutex::new(Writer::Idle),
};

impl Saver {
    fn stage(&self, state: WindowState) {
        if let Ok(mut staged) = self.staged.lock() {
            *staged = Some(state);
        }
    }

    /// Write the staged state, if any, to `path`. False when the write
    /// failed (the state stays staged).
    fn write_staged(&self, path: &Path) -> bool {
        let Ok(_write) = self.write.lock() else {
            return false;
        };
        let state = self.staged.lock().ok().and_then(|mut s| s.take());
        if let Some(state) = state
            && let Err(err) = write_json_atomic(path, &state)
        {
            // Back in the slot (unless a newer geometry was staged meanwhile),
            // so the next save and the quit flush try it again.
            if let Ok(mut staged) = self.staged.lock() {
                staged.get_or_insert(state);
            }
            super::deferred::note_write_failure(err);
            return false;
        }
        true
    }

    /// Start the writer with `spawn` unless one runs. A running writer
    /// re-reads the slot before it exits, so the staged state is its to
    /// write; a thread that could not start leaves the writer `Idle`, and
    /// the next save (or the quit flush) writes the slot.
    fn start_writer(&self, spawn: impl FnOnce() -> std::io::Result<std::thread::JoinHandle<()>>) {
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        // A writer thread that is gone (it panicked) is not running: start a
        // new one rather than only staging until the quit flush.
        if matches!(&*writer, Writer::Running { thread } if !thread.is_finished()) {
            return;
        }
        *writer = Writer::Idle;
        if let Ok(handle) = spawn() {
            *writer = Writer::Running { thread: handle };
        }
    }

    /// The writer thread's body: write until the slot is empty, then go
    /// `Idle`. The slot is checked while holding `writer`, so a save that
    /// staged after the check finds `Idle` and starts a new writer.
    fn run_writer(&self, path: &Path) {
        loop {
            let wrote = self.write_staged(path);
            let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
            let more = wrote && self.staged.lock().is_ok_and(|staged| staged.is_some());
            if !more {
                *writer = Writer::Idle;
                return;
            }
        }
    }
}

fn state_path() -> Option<std::path::PathBuf> {
    Some(safe_app_root()?.join("window_state.json"))
}

/// Stage `state` as the newest geometry to save (cheap, never touches disk).
pub fn stage_window_state(state: WindowState) {
    SAVER.stage(state);
}

/// Write the staged geometry on a background thread. A call while the
/// writer runs only stages: the writer takes the newest state before it
/// exits.
pub fn save_staged_window_state() {
    let Some(path) = state_path() else {
        return;
    };
    SAVER.start_writer(|| {
        std::thread::Builder::new()
            .name("quill-window-state".into())
            .spawn(move || SAVER.run_writer(&path))
    });
}

/// Write the staged geometry now, after any write in flight; for quitting.
pub fn flush_window_state() {
    if let Some(path) = state_path() {
        SAVER.write_staged(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::{Saver, WindowState, Writer};
    use std::sync::Mutex;

    fn state(x: f32) -> WindowState {
        WindowState {
            x,
            y: 0.,
            width: 800.,
            height: 600.,
            maximized: false,
            sidebar_width: 300.,
            sidebar_hidden: false,
        }
    }

    #[test]
    fn only_the_newest_staged_state_is_written() {
        let dir = crate::scratch::process_dir("quill-window-save");
        let path = dir.join("window_state.json");
        let saver = Saver {
            staged: Mutex::new(None),
            write: Mutex::new(()),
            writer: Mutex::new(Writer::Idle),
        };
        saver.write_staged(&path);
        assert!(!path.exists(), "nothing staged, nothing written");
        saver.stage(state(1.));
        saver.stage(state(2.));
        saver.write_staged(&path);
        let saved: WindowState =
            serde_json::from_slice(&std::fs::read(&path).unwrap_or_default()).unwrap_or(state(-1.));
        assert_eq!(saved.x, 2.);
        std::fs::remove_file(&path).ok();
        saver.write_staged(&path);
        assert!(!path.exists(), "a state is written once");
    }

    #[test]
    fn a_failed_write_keeps_the_state_staged_and_reports_it() {
        let dir = crate::scratch::process_dir("quill-window-save-failed");
        let blocker = dir.join("not-a-folder");
        std::fs::write(&blocker, b"x").ok();
        let path = blocker.join("window_state.json");
        let saver = Saver {
            staged: Mutex::new(None),
            write: Mutex::new(()),
            writer: Mutex::new(Writer::Idle),
        };
        let _ = crate::settings::take_deferred_write_failure();
        saver.stage(state(3.));
        saver.write_staged(&path);
        assert!(crate::settings::take_deferred_write_failure().is_some());
        saver.stage(state(4.));
        saver.write_staged(&path);
        let staged = saver.staged.lock().ok().and_then(|s| *s);
        assert_eq!(staged.map(|s| s.x), Some(4.), "the newest state stays");
        let good = dir.join("window_state.json");
        saver.write_staged(&good);
        assert!(good.exists(), "written once the disk takes it");
        std::fs::remove_file(&good).ok();
        std::fs::remove_file(&blocker).ok();
    }

    fn running(saver: &Saver) -> bool {
        saver
            .writer
            .lock()
            .is_ok_and(|writer| matches!(*writer, Writer::Running { .. }))
    }

    #[test]
    fn saves_while_the_writer_runs_start_no_second_thread() {
        let dir = crate::scratch::process_dir("quill-window-save-writer");
        let path = dir.join("window_state.json");
        let saver = Saver {
            staged: Mutex::new(None),
            write: Mutex::new(()),
            writer: Mutex::new(Writer::Idle),
        };
        let started = std::cell::Cell::new(0);
        let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spawn = || {
            started.set(started.get() + 1);
            let release = release.clone();
            Ok(std::thread::spawn(move || {
                while !release.load(std::sync::atomic::Ordering::SeqCst) {
                    std::thread::park();
                }
            }))
        };
        saver.stage(state(1.));
        saver.start_writer(spawn);
        for x in 2..50 {
            saver.stage(state(x as f32));
            saver.start_writer(spawn);
        }
        assert_eq!(started.get(), 1, "one writer, however many reports");
        // The writer's body takes the newest state, then leaves.
        saver.run_writer(&path);
        let saved: WindowState =
            serde_json::from_slice(&std::fs::read(&path).unwrap_or_default()).unwrap_or(state(-1.));
        assert_eq!(saved.x, 49.);
        assert!(!running(&saver), "the writer left with an empty slot");
        saver.stage(state(60.));
        saver.start_writer(spawn);
        assert_eq!(started.get(), 2, "a later report starts a new writer");
        release.store(true, std::sync::atomic::Ordering::SeqCst);
        std::fs::remove_file(&path).ok();
    }

    /// Sweep r3-02 (07-002): a writer thread that died leaves `Running`
    /// behind; the next save starts a new one.
    #[test]
    fn a_writer_thread_that_died_is_replaced_by_the_next_save() {
        let saver = Saver {
            staged: Mutex::new(None),
            write: Mutex::new(()),
            writer: Mutex::new(Writer::Idle),
        };
        let dead = std::thread::spawn(|| {});
        while !dead.is_finished() {
            std::thread::yield_now();
        }
        *saver.writer.lock().unwrap_or_else(|e| e.into_inner()) = Writer::Running { thread: dead };
        let started = std::cell::Cell::new(0);
        saver.stage(state(1.));
        saver.start_writer(|| {
            started.set(started.get() + 1);
            Ok(std::thread::spawn(|| {}))
        });
        assert_eq!(started.get(), 1, "the dead writer is replaced");
    }

    #[test]
    fn a_writer_that_cannot_start_leaves_the_slot_for_the_next_save() {
        let saver = Saver {
            staged: Mutex::new(None),
            write: Mutex::new(()),
            writer: Mutex::new(Writer::Idle),
        };
        saver.stage(state(1.));
        saver.start_writer(|| Err(std::io::Error::other("no threads")));
        assert!(!running(&saver));
        assert!(saver.staged.lock().is_ok_and(|s| s.is_some()));
    }

    #[test]
    fn a_failed_write_ends_the_writer_with_the_state_staged() {
        let dir = crate::scratch::process_dir("quill-window-save-writer-failed");
        let blocker = dir.join("not-a-folder");
        std::fs::write(&blocker, b"x").ok();
        let saver = Saver {
            staged: Mutex::new(None),
            write: Mutex::new(()),
            writer: Mutex::new(Writer::Idle),
        };
        let _ = crate::settings::take_deferred_write_failure();
        saver.stage(state(1.));
        saver.start_writer(|| Ok(std::thread::spawn(|| {})));
        saver.run_writer(&blocker.join("window_state.json"));
        assert!(!running(&saver), "no retry loop on a failing disk");
        assert!(saver.staged.lock().is_ok_and(|s| s.is_some()));
        let _ = crate::settings::take_deferred_write_failure();
        std::fs::remove_file(&blocker).ok();
    }
}

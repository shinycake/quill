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

/// The staged state and the write lock. The writer holds `write` before it
/// takes `staged`, so a flush that gets `write` sees every earlier write
/// finished.
struct Saver {
    staged: Mutex<Option<WindowState>>,
    write: Mutex<()>,
}

static SAVER: Saver = Saver {
    staged: Mutex::new(None),
    write: Mutex::new(()),
};

impl Saver {
    fn stage(&self, state: WindowState) {
        if let Ok(mut staged) = self.staged.lock() {
            *staged = Some(state);
        }
    }

    /// Write the staged state, if any, to `path`.
    fn write_staged(&self, path: &Path) {
        let Ok(_write) = self.write.lock() else {
            return;
        };
        let state = self.staged.lock().ok().and_then(|mut s| s.take());
        if let Some(state) = state {
            let _ = write_json_atomic(path, &state);
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

/// Write the staged geometry on a background thread. Calls while a write is
/// running just queue behind it and find nothing left to write.
pub fn save_staged_window_state() {
    let Some(path) = state_path() else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("quill-window-state".into())
        .spawn(move || SAVER.write_staged(&path));
}

/// Write the staged geometry now, after any write in flight; for quitting.
pub fn flush_window_state() {
    if let Some(path) = state_path() {
        SAVER.write_staged(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::{Saver, WindowState};
    use std::sync::Mutex;

    fn state(x: f32) -> WindowState {
        WindowState {
            x,
            y: 0.,
            width: 800.,
            height: 600.,
            maximized: false,
            sidebar_width: 300.,
        }
    }

    #[test]
    fn only_the_newest_staged_state_is_written() {
        let dir = crate::scratch::process_dir("quill-window-save");
        let path = dir.join("window_state.json");
        let saver = Saver {
            staged: Mutex::new(None),
            write: Mutex::new(()),
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
}

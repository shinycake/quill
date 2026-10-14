//! Which picture files carry HDR data, for the "HDR" tag on the chat
//! bubble's file pill. Probing reads metadata only, but still touches the
//! disk, so it runs once per path in the background and the answer is
//! remembered for the session. A file that couldn't be opened is asked
//! again after a while (it may have been mid-write).

use crate::ui::QuillApp;
use gpui_kit::{AppContext, Context};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Paths remembered before the cache starts over (a few KB of paths).
const CAPACITY: usize = 1024;

/// How long an unreadable file waits before it is probed again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

enum Entry {
    Probing,
    Known(bool),
    Unreadable(Instant),
}

static PROBED: LazyLock<Mutex<HashMap<PathBuf, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether the file at `path` carries HDR data. The first ask starts a
/// background probe and answers `false`; the bubble repaints when it
/// lands.
pub(crate) fn has_hdr(path: &Path, cx: &mut Context<QuillApp>) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let Ok(mut probed) = PROBED.lock() else {
        return false;
    };
    match probed.get(path) {
        Some(Entry::Known(hdr)) => return *hdr,
        Some(Entry::Probing) => return false,
        Some(Entry::Unreadable(at)) if at.elapsed() < RETRY_AFTER => return false,
        Some(Entry::Unreadable(_)) | None => {}
    }
    if probed.len() >= CAPACITY {
        probed.clear();
    }
    probed.insert(path.to_path_buf(), Entry::Probing);
    drop(probed);
    let path = path.to_path_buf();
    let job = cx.background_spawn({
        let path = path.clone();
        async move { super::probe_file(&path) }
    });
    cx.spawn(async move |this, cx| {
        let result = job.await;
        let entry = match result {
            Some(hdr) => Entry::Known(hdr),
            None => Entry::Unreadable(Instant::now()),
        };
        if let Ok(mut probed) = PROBED.lock() {
            probed.insert(path, entry);
        }
        if result == Some(true) {
            let _ = this.update(cx, |_, cx| cx.notify());
        }
    })
    .detach();
    false
}

//! Which picture files carry HDR data, for the "HDR" tag on the chat
//! bubble's file pill. Probing reads metadata only, but still touches the
//! disk, so it runs once per path in the background and the answer is
//! remembered for the session. A file that couldn't be opened is asked
//! again after a while (it may have been mid-write). A path has at most one
//! probe: the entry owns the job, and only the job's own end (an answer or
//! unreadable) leaves `Probing`.

use crate::ui::QuillApp;
use gpui_kit::{AppContext, Context, Task};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Paths remembered before the cache starts over (a few KB of paths).
const CAPACITY: usize = 1024;

/// How long an unreadable file waits before it is probed again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

enum Entry {
    /// The probe of this path is running; the entry owns the job, which
    /// replaces it with the answer when it ends.
    Probing {
        _job: Task<()>,
    },
    Known(bool),
    Unreadable(Instant),
}

static PROBED: LazyLock<Mutex<HashMap<PathBuf, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// What a cache entry says to do now.
#[derive(Debug, PartialEq, Eq)]
enum Lookup {
    Answer(bool),
    Wait,
    Probe,
}

fn lookup(entry: Option<&Entry>) -> Lookup {
    match entry {
        Some(Entry::Known(hdr)) => Lookup::Answer(*hdr),
        Some(Entry::Probing { .. }) => Lookup::Wait,
        Some(Entry::Unreadable(at)) if at.elapsed() < RETRY_AFTER => Lookup::Wait,
        Some(Entry::Unreadable(_)) | None => Lookup::Probe,
    }
}

/// Whether the file at `path` carries HDR data. The first ask starts a
/// background probe and answers `false`; the bubble repaints when it
/// lands.
pub(crate) fn has_hdr(path: &Path, cx: &mut Context<QuillApp>) -> bool {
    if !cfg!(any(target_os = "macos", windows)) {
        return false;
    }
    let Ok(mut probed) = PROBED.lock() else {
        return false;
    };
    match lookup(probed.get(path)) {
        Lookup::Answer(hdr) => return hdr,
        Lookup::Wait => return false,
        Lookup::Probe => {}
    }
    if probed.len() >= CAPACITY {
        // A running probe stays: dropping it would let the path be probed a
        // second time while the first read is still blocked.
        probed.retain(|_, entry| matches!(entry, Entry::Probing { .. }));
    }
    let key = path.to_path_buf();
    let probe_path = key.clone();
    // A probe that panics counts as unreadable.
    let job = cx.background_spawn(async move {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::probe_file(&probe_path)
        }))
        .unwrap_or(None)
    });
    let owner = key.clone();
    let task = cx.spawn(async move |this, cx| {
        let result = job.await;
        if result == Some(true) {
            let _ = this.update(cx, |_, cx| cx.notify());
        }
        let entry = match result {
            Some(hdr) => Entry::Known(hdr),
            None => Entry::Unreadable(Instant::now()),
        };
        if let Ok(mut probed) = PROBED.lock() {
            probed.insert(owner, entry);
        }
    });
    probed.insert(key, Entry::Probing { _job: task });
    false
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{Entry, Lookup, lookup};
    use std::time::Instant;

    #[test]
    fn a_running_probe_is_waited_for_never_started_again() {
        let running = Entry::Probing {
            _job: gpui_kit::Task::ready(()),
        };
        assert_eq!(lookup(Some(&running)), Lookup::Wait);
        assert_eq!(lookup(Some(&Entry::Known(true))), Lookup::Answer(true));
        assert_eq!(
            lookup(Some(&Entry::Unreadable(Instant::now()))),
            Lookup::Wait
        );
        assert_eq!(lookup(None), Lookup::Probe);
    }
}

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

/// How long a probe may run before it counts as lost (a stalled network
/// drive): the file is then probed again, so one hung read never turns the
/// tag off for good.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

enum Entry {
    /// The probe job started at `since`; it records its own answer.
    Probing {
        since: Instant,
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
        Some(Entry::Probing { since }) if since.elapsed() < PROBE_TIMEOUT => Lookup::Wait,
        Some(Entry::Unreadable(at)) if at.elapsed() < RETRY_AFTER => Lookup::Wait,
        Some(Entry::Probing { .. } | Entry::Unreadable(_)) | None => Lookup::Probe,
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
        probed.clear();
    }
    probed.insert(
        path.to_path_buf(),
        Entry::Probing {
            since: Instant::now(),
        },
    );
    drop(probed);
    let path = path.to_path_buf();
    // The job records its own answer, so `Probing` leaves even when the
    // foreground continuation below is dropped; a probe that panics counts
    // as unreadable.
    let job = cx.background_spawn(async move {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| super::probe_file(&path)))
                .unwrap_or(None);
        let entry = match result {
            Some(hdr) => Entry::Known(hdr),
            None => Entry::Unreadable(Instant::now()),
        };
        if let Ok(mut probed) = PROBED.lock() {
            probed.insert(path, entry);
        }
        result
    });
    cx.spawn(async move |this, cx| {
        let result = job.await;
        if result == Some(true) {
            let _ = this.update(cx, |_, cx| cx.notify());
        }
    })
    .detach();
    false
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{Entry, Lookup, PROBE_TIMEOUT, lookup};
    use std::time::{Duration, Instant};

    #[test]
    fn a_probe_that_never_returns_is_asked_again() {
        let now = Instant::now();
        let fresh = Entry::Probing { since: now };
        assert_eq!(lookup(Some(&fresh)), Lookup::Wait);
        if let Some(since) = now.checked_sub(PROBE_TIMEOUT + Duration::from_secs(1)) {
            let lost = Entry::Probing { since };
            assert_eq!(lookup(Some(&lost)), Lookup::Probe);
        }
        assert_eq!(lookup(Some(&Entry::Known(true))), Lookup::Answer(true));
        assert_eq!(lookup(None), Lookup::Probe);
    }
}

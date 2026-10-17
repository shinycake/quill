//! Ending a helper process without making the calling thread wait for it.
//!
//! `Child::wait` after `kill` can take a while (a camera device or a web
//! view tearing down), which freezes the UI when `Drop` runs there. These
//! helpers hand the child to a detached thread that reaps it.

use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Kill `child` and reap it on a detached thread.
pub fn kill_and_reap(child: Child) {
    reap_after(child, Duration::ZERO);
}

/// Give `child` up to `grace` to exit by itself, then kill it, all on a
/// detached thread. If no thread can be started the child is killed here
/// and polled briefly (about 50 ms at most) for its exit, then left to the OS.
pub fn reap_after(child: Child, grace: Duration) {
    let slot = Arc::new(Mutex::new(Some(child)));
    let worker = slot.clone();
    let spawned = std::thread::Builder::new()
        .name("quill-child-reap".into())
        .spawn(move || {
            let child = worker.lock().ok().and_then(|mut slot| slot.take());
            if let Some(child) = child {
                reap(child, grace);
            }
        });
    if spawned.is_err() {
        let child = slot.lock().ok().and_then(|mut slot| slot.take());
        if let Some(mut child) = child {
            let _ = child.kill();
            // Never block the caller: a few polls for the reap, then leave
            // the child to the OS.
            for _ in 0..5 {
                if !matches!(child.try_wait(), Ok(None)) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn reap(mut child: Child, grace: Duration) {
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::{Duration, Instant, kill_and_reap};
    use std::process::{Command, Stdio};

    #[test]
    fn kill_and_reap_returns_at_once_and_ends_the_child() {
        let Ok(child) = Command::new("sleep").arg("30").stdin(Stdio::null()).spawn() else {
            return;
        };
        let pid = child.id();
        let started = Instant::now();
        kill_and_reap(child);
        assert!(started.elapsed() < Duration::from_secs(1));
        let gone = (0..200).any(|_| {
            std::thread::sleep(Duration::from_millis(25));
            Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(Stdio::null())
                .status()
                .map(|s| !s.success())
                .unwrap_or(true)
        });
        assert!(gone);
    }
}

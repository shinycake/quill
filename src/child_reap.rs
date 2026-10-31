//! Ending a helper process without making the calling thread wait for it.
//!
//! `Child::wait` after `kill` can take a while (a camera device or a web
//! view tearing down), which freezes the UI when `Drop` runs there. These
//! helpers hand the child to a detached thread that reaps it.

use std::process::Child;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Kill `child` now (sending the signal does not block) and reap it on a
/// detached thread.
pub fn kill_and_reap(mut child: Child) {
    let _ = child.kill();
    reap_after(child, Duration::ZERO);
}

/// Give `child` up to `grace` to exit by itself, then kill it, all on a
/// detached thread. If no thread can be started the child is killed here
/// and polled briefly (about 50 ms at most) for its exit, then left to the OS.
pub fn reap_after(child: Child, grace: Duration) {
    reap_after_with(child, grace, None);
}

/// [`reap_after`], and the returned receiver hears (or disconnects) once
/// the child is gone, for a caller that may want to wait for that.
pub fn reap_after_notice(child: Child, grace: Duration) -> Receiver<()> {
    let (done, notice) = sync_channel(1);
    reap_after_with(child, grace, Some(done));
    notice
}

fn reap_after_with(child: Child, grace: Duration, done: Option<SyncSender<()>>) {
    let slot = Arc::new(Mutex::new(Some(child)));
    let worker = slot.clone();
    let spawned = std::thread::Builder::new()
        .name("quill-child-reap".into())
        .spawn(move || {
            let child = worker.lock().ok().and_then(|mut slot| slot.take());
            if let Some(child) = child {
                reap(child, grace);
            }
            if let Some(done) = done {
                let _ = done.try_send(());
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

/// Collect `child` once it exits by itself, so it does not stay behind as a
/// zombie until Quill quits. Never kills: whatever `open` or `xdg-open`
/// started may stay in the foreground.
pub fn wait_in_background(mut child: Child) -> Receiver<()> {
    let (done, notice) = sync_channel(1);
    let _ = crate::thread_guard::spawn_guarded("quill-child-wait", move || {
        let _ = child.wait();
        let _ = done.try_send(());
    });
    notice
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
    use super::{kill_and_reap, wait_in_background};
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::mpsc::TryRecvError;

    fn process_state(pid: u32) -> String {
        Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .unwrap_or_default()
    }

    #[test]
    fn a_child_left_to_exit_is_collected_and_never_killed() {
        let Ok(child) = Command::new("true").stdin(Stdio::null()).spawn() else {
            return;
        };
        let pid = child.id();
        let _ = wait_in_background(child).recv();
        assert_eq!(process_state(pid), "", "no zombie left behind");
    }

    #[test]
    fn waiting_leaves_a_running_child_alone() {
        let Ok(child) = Command::new("sleep").arg("30").stdin(Stdio::null()).spawn() else {
            return;
        };
        let pid = child.id();
        let notice = wait_in_background(child);
        assert!(
            matches!(notice.try_recv(), Err(TryRecvError::Empty)),
            "the child is still running"
        );
        assert!(!process_state(pid).starts_with('Z'));
        let _ = Command::new("kill").arg(pid.to_string()).status();
        let _ = notice.recv();
        assert_eq!(process_state(pid), "");
    }

    /// The signal is sent before the call returns. The child is a shell
    /// waiting for a line; the test closes its input right after the call,
    /// so a child the call left running reads the end, says `survived` and
    /// exits. The read of its output ends either way: at its death or at
    /// that exit.
    #[test]
    fn kill_and_reap_kills_the_child_before_it_returns() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "read -r line; echo survived"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        kill_and_reap(child);
        drop(input);
        let mut said = String::new();
        output.read_to_string(&mut said).unwrap();
        assert_eq!(said, "");
    }
}

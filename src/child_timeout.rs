//! Run a helper program with a time limit.
//!
//! `Command::output` and `Command::status` wait forever. A wedged D-Bus
//! portal, a file on a stalled network share or a hung ffmpeg would then pin
//! a thread (and whatever flag it holds) for the life of the process. Every
//! child that is waited on goes through here instead: when the limit passes
//! the child is killed and reaped, and the caller gets
//! [`io::ErrorKind::TimedOut`].
#![deny(clippy::indexing_slicing)]

use crate::media_tools::NoConsole;
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How often a running child is checked for exit.
const POLL: Duration = Duration::from_millis(10);
/// How long the output readers get to finish after the child is gone. A
/// grandchild that inherited the pipes can keep them open past the kill.
const READER_GRACE: Duration = Duration::from_millis(500);

/// Short probes (`ffprobe` on one file, `gsettings get`, `fc-list`).
pub const PROBE_LIMIT: Duration = Duration::from_secs(10);
/// Desktop services over D-Bus (`gdbus`, `xdg-mime`, `xdg-settings`).
pub const DESKTOP_QUERY_LIMIT: Duration = Duration::from_secs(5);
/// A notification helper that waits for a click (`notify-send --wait`): the
/// user may take their time, but not for ever.
pub const NOTIFICATION_WAIT_LIMIT: Duration = Duration::from_secs(5 * 60);

/// Time an ffmpeg job on `media_seconds` of media may take: a fixed 30 s for
/// start-up and slow disks, plus 4 s for each second of media (a slow
/// software encoder runs far below real time), at most ten minutes. An
/// unknown (zero or negative) duration gets the two-minute middle.
pub fn transcode_limit(media_seconds: f64) -> Duration {
    if !media_seconds.is_finite() || media_seconds <= 0.0 {
        return Duration::from_secs(120);
    }
    let seconds = (30.0 + media_seconds * 4.0).min(600.0);
    Duration::from_secs_f64(seconds)
}

/// `cmd.output()` with a limit: stdin is closed, stdout and stderr are
/// captured, and a child still running after `limit` is killed and reaped.
pub fn output_within(cmd: &mut Command, limit: Duration) -> io::Result<Output> {
    let mut child = cmd
        .no_console()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let readers = child
        .stdout
        .take()
        .map(read_all_in_thread)
        .transpose()
        .and_then(|stdout| {
            let stderr = child.stderr.take().map(read_all_in_thread).transpose()?;
            Ok((stdout, stderr))
        });
    let (stdout, stderr) = match readers {
        Ok(readers) => readers,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
    };
    let status = match wait_within(&mut child, limit) {
        Ok(status) => status,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
    };
    Ok(Output {
        status,
        stdout: collect(stdout)?,
        stderr: collect(stderr)?,
    })
}

/// What `program args` prints on stdout when it exits successfully within
/// `limit`; `None` for a missing program, a failure, a timeout or output
/// that is not text. For the desktop's one-line answers (`gsettings get`).
pub fn stdout_within(program: &str, args: &[&str], limit: Duration) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    let out = output_within(&mut cmd, limit).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `cmd.status()` with a limit. Stdin is closed; stdout and stderr stay as
/// the caller set them. Those must not be `Stdio::piped()`: nothing here
/// drains a pipe, so a chatty child would block until the limit. Use
/// `Stdio::null()` or a log file (as the callers do), or [`output_within`]
/// to read what the child prints.
pub fn status_within(cmd: &mut Command, limit: Duration) -> io::Result<ExitStatus> {
    let mut child = cmd.no_console().stdin(Stdio::null()).spawn()?;
    wait_within(&mut child, limit).inspect_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
    })
}

/// Wait for an already started `child` for at most `limit`. The child is
/// left running on a timeout: the caller kills and reaps it.
pub fn wait_within(child: &mut std::process::Child, limit: Duration) -> io::Result<ExitStatus> {
    let deadline = Instant::now().checked_add(limit);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the helper program did not finish in time",
            ));
        }
        std::thread::sleep(POLL);
    }
}

/// Whether `err` is a [`wait_within`] timeout.
pub fn is_timeout(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::TimedOut
}

/// Read `pipe` to its end on a thread of its own, so a child that fills its
/// pipe never stalls waiting for us to read. A thread that cannot start is an
/// error: the output would be silently empty.
fn read_all_in_thread<R: Read + Send + 'static>(
    mut pipe: R,
) -> io::Result<mpsc::Receiver<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("quill-child-output".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            let _ = tx.send(bytes);
        })?;
    Ok(rx)
}

/// What a reader read. A reader that has not finished within
/// [`READER_GRACE`] of the child's exit (a grandchild kept the pipe open) is
/// an error rather than a silently cut output.
fn collect(reader: Option<mpsc::Receiver<Vec<u8>>>) -> io::Result<Vec<u8>> {
    match reader {
        None => Ok(Vec::new()),
        Some(rx) => rx.recv_timeout(READER_GRACE).map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "the helper program's output did not end in time",
            )
        }),
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[test]
    fn output_is_captured() {
        let out = output_within(&mut sh("echo hi; echo oops >&2"), Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hi\n");
        assert_eq!(out.stderr, b"oops\n");
    }

    #[test]
    fn large_output_does_not_stall_the_child() {
        let out = output_within(
            &mut sh("head -c 1000000 /dev/zero"),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(out.stdout.len(), 1_000_000);
    }

    #[test]
    fn a_hung_child_is_killed_and_reaped_after_the_limit() {
        let marker = std::env::temp_dir().join(format!("quill-hung-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let started = Instant::now();
        let script = format!("sleep 30; touch {}", marker.display());
        let err = output_within(&mut sh(&script), Duration::from_millis(200)).unwrap_err();
        assert!(is_timeout(&err));
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!marker.exists(), "the killed child never got to run on");
    }

    #[test]
    fn a_hung_child_is_killed_by_status_within_too() {
        let started = Instant::now();
        let err = status_within(&mut sh("sleep 30"), Duration::from_millis(200)).unwrap_err();
        assert!(is_timeout(&err));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_chatty_child_with_its_output_sent_away_does_not_stall_status_within() {
        let started = Instant::now();
        let mut cmd = sh("head -c 1000000 /dev/zero; head -c 1000000 /dev/zero >&2");
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let status = status_within(&mut cmd, Duration::from_secs(10)).unwrap();
        assert!(status.success());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_cut_off_by_a_pipe_holding_grandchild_is_an_error() {
        // The background sleep inherits stdout and keeps it open after the
        // shell exits.
        let err = output_within(&mut sh("echo hi; sleep 5 &"), Duration::from_secs(5)).unwrap_err();
        assert!(is_timeout(&err));
    }

    #[test]
    fn a_missing_program_is_a_spawn_error_not_a_timeout() {
        let err = output_within(
            &mut Command::new("/definitely/not/a/program"),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(!is_timeout(&err));
    }

    #[test]
    fn transcode_limit_scales_with_the_media_and_is_capped() {
        assert_eq!(transcode_limit(0.0), Duration::from_secs(120));
        assert_eq!(transcode_limit(f64::NAN), Duration::from_secs(120));
        assert_eq!(transcode_limit(10.0), Duration::from_secs(70));
        assert_eq!(transcode_limit(100_000.0), Duration::from_secs(600));
    }
}

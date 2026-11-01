//! Retrying a file operation that Windows refuses for a moment because
//! another process (an antivirus scanner, the search indexer, a backup tool)
//! holds the file open without delete sharing. Elsewhere nothing is retried.

use std::io;
use std::path::Path;
use std::time::Duration;

/// For writes made on the UI thread: two pauses, 300 ms at most.
pub const SHORT: Duration = Duration::from_millis(300);

/// For the updater and other work off the UI thread: ten pauses, about 4 s.
pub const LONG: Duration = Duration::from_millis(4_200);

const FIRST_PAUSE: Duration = Duration::from_millis(100);
const MAX_PAUSE: Duration = Duration::from_millis(500);

/// ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
fn windows_lock(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

fn transient(error: &io::Error) -> bool {
    cfg!(windows) && windows_lock(error)
}

/// `std::fs::rename` (which replaces `to`), run again on a transient lock
/// until the pauses would pass `budget`.
pub fn rename_retrying(from: &Path, to: &Path, budget: Duration) -> io::Result<()> {
    retry(budget, || std::fs::rename(from, to))
}

/// Runs `op` again on a transient lock, pausing 100 ms, then twice as long
/// each time up to 500 ms, while the pauses stay within `budget`.
pub fn retry<T>(budget: Duration, op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    retry_with(budget, transient, std::thread::sleep, op)
}

fn retry_with<T>(
    budget: Duration,
    transient: impl Fn(&io::Error) -> bool,
    mut pause: impl FnMut(Duration),
    mut op: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    let mut delay = FIRST_PAUSE;
    let mut spent = Duration::ZERO;
    loop {
        match op() {
            Err(error) if transient(&error) && spent.saturating_add(delay) <= budget => {
                pause(delay);
                spent = spent.saturating_add(delay);
                delay = delay.saturating_mul(2).min(MAX_PAUSE);
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sharing_violation() -> io::Error {
        io::Error::from_raw_os_error(32)
    }

    /// A rename that fails `failures` times with a sharing violation, then
    /// succeeds; returns the result, the attempts and the pauses taken.
    fn locked_rename(failures: usize, budget: Duration) -> (io::Result<()>, usize, Vec<Duration>) {
        let mut attempts = 0;
        let mut pauses = Vec::new();
        let result = retry_with(
            budget,
            windows_lock,
            |delay| pauses.push(delay),
            || {
                attempts += 1;
                if attempts <= failures {
                    Err(sharing_violation())
                } else {
                    Ok(())
                }
            },
        );
        (result, attempts, pauses)
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_brief_lock_is_waited_out() {
        let (result, attempts, pauses) = locked_rename(2, SHORT);
        assert!(result.is_ok());
        assert_eq!(attempts, 3);
        assert_eq!(pauses, vec![ms(100), ms(200)]);
    }

    #[test]
    fn the_short_budget_gives_up_after_300_ms_of_pauses() {
        let (result, attempts, pauses) = locked_rename(3, SHORT);
        assert_eq!(result.unwrap_err().raw_os_error(), Some(32));
        assert_eq!(attempts, 3);
        assert_eq!(pauses.iter().sum::<Duration>(), ms(300));
    }

    #[test]
    fn the_long_budget_keeps_the_updaters_ten_retries() {
        let (result, attempts, pauses) = locked_rename(10, LONG);
        assert!(result.is_ok());
        assert_eq!(attempts, 11);
        assert_eq!(
            pauses,
            [100, 200, 400, 500, 500, 500, 500, 500, 500, 500].map(ms)
        );
        let (result, attempts, _) = locked_rename(11, LONG);
        assert!(result.is_err());
        assert_eq!(attempts, 11);
    }

    #[test]
    fn other_errors_are_not_retried() {
        let mut attempts = 0;
        let result: io::Result<()> = retry_with(
            LONG,
            windows_lock,
            |_| panic!("no pause expected"),
            || {
                attempts += 1;
                Err(io::Error::from(io::ErrorKind::NotFound))
            },
        );
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn a_zero_budget_never_pauses() {
        let (result, attempts, pauses) = locked_rename(1, Duration::ZERO);
        assert!(result.is_err());
        assert_eq!(attempts, 1);
        assert!(pauses.is_empty());
    }

    #[test]
    fn only_windows_lock_errors_count_as_transient() {
        assert!(windows_lock(&io::Error::from_raw_os_error(5)));
        assert!(windows_lock(&io::Error::from_raw_os_error(33)));
        assert!(!windows_lock(&io::Error::from_raw_os_error(2)));
        assert_eq!(transient(&sharing_violation()), cfg!(windows));
    }
}

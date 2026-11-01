//! A process started by launchd gets a soft limit of 256 open files, and a
//! Terminal run hides that. Each account's TDLib client holds its binlog, its
//! SQLite files and a socket per data center, so a few accounts with media open
//! come near it, and past it every `open` and `socket` fails.

/// `FD_SETSIZE`: `select()`, which WebRTC's socket server uses on macOS, can't
/// take a descriptor at or above this.
#[cfg(unix)]
const TARGET: libc::rlim_t = 1024;

/// The soft limit to set, when `soft` is below [`TARGET`].
#[cfg(unix)]
fn raised_soft_limit(soft: libc::rlim_t, hard: libc::rlim_t) -> Option<libc::rlim_t> {
    (soft < TARGET).then_some(TARGET.min(hard))
}

/// Raises the soft limit on open files towards [`TARGET`].
pub fn raise_open_file_limit() {
    #[cfg(unix)]
    {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `limit` is a valid rlimit for the call to fill in.
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
            return;
        }
        if let Some(soft) = raised_soft_limit(limit.rlim_cur, limit.rlim_max) {
            limit.rlim_cur = soft;
            // SAFETY: `limit` is a valid rlimit that outlives the call.
            unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) };
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn a_low_limit_rises_to_the_target() {
        assert_eq!(raised_soft_limit(256, libc::RLIM_INFINITY), Some(1024));
    }

    #[test]
    fn a_low_hard_limit_caps_the_rise() {
        assert_eq!(raised_soft_limit(256, 512), Some(512));
    }

    #[test]
    fn a_limit_at_or_above_the_target_stays() {
        assert_eq!(raised_soft_limit(1024, libc::RLIM_INFINITY), None);
        assert_eq!(raised_soft_limit(1_048_576, libc::RLIM_INFINITY), None);
    }

    fn limits() -> libc::rlimit {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `limit` is a valid rlimit for the call to fill in.
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) }, 0);
        limit
    }

    #[test]
    fn the_process_limit_rises_from_launchds_default() {
        const CHILD: &str = "QUILL_FD_LIMIT_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "fd_limit::tests::the_process_limit_rises_from_launchds_default",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let launchd = libc::rlimit {
            rlim_cur: 256,
            ..limits()
        };
        // SAFETY: `launchd` is a valid rlimit that outlives the call.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &launchd) }, 0);
        raise_open_file_limit();
        assert_eq!(limits().rlim_cur, 1024);
    }
}

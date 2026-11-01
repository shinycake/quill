//! The OS watchers against a real kernel. CI runs the Linux one in a
//! fresh network namespace and the Windows one on a Windows runner.

#[cfg(target_os = "linux")]
mod linux {
    use super::super::backend::{listen, subscribe};
    use std::io;
    use std::ops::ControlFlow;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    const CHILD: &str = "QUILL_NET_WATCH_IN_NAMESPACE";

    #[test]
    #[ignore = "needs user namespaces; CI turns them on and runs it"]
    fn a_link_coming_up_wakes_the_watcher_in_a_fresh_network_namespace() {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "net_watch::platform::watcher_tests::linux::in_the_namespace",
                "--ignored",
                "--test-threads=1",
            ])
            .env(CHILD, "1");
        // Root in the new user namespace, so `exec` keeps its capabilities.
        // SAFETY: plain calls with no arguments.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        let uid_map = format!("0 {uid} 1");
        let gid_map = format!("0 {gid} 1");
        // SAFETY: the forked child is single threaded, as a new user
        // namespace requires, and makes only system calls on memory the
        // parent prepared.
        unsafe {
            child.pre_exec(move || {
                if libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNET) != 0 {
                    return Err(io::Error::last_os_error());
                }
                write_proc(c"/proc/self/setgroups", b"deny")?;
                write_proc(c"/proc/self/uid_map", uid_map.as_bytes())?;
                write_proc(c"/proc/self/gid_map", gid_map.as_bytes())
            });
        }
        let output = child
            .output()
            .expect("no user namespace: set kernel.apparmor_restrict_unprivileged_userns=0");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "the namespace test did not run"
        );
    }

    /// `open` and `write` only: this runs between `fork` and `exec`.
    fn write_proc(path: &std::ffi::CStr, text: &[u8]) -> io::Result<()> {
        // SAFETY: `path` is NUL-terminated and `text` is live for the write.
        unsafe {
            let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let written = libc::write(fd, text.as_ptr().cast(), text.len());
            libc::close(fd);
            if written < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    #[test]
    #[ignore = "run by the test above inside its namespace"]
    fn in_the_namespace() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        assert!(
            !super::super::super::route_now().available(),
            "a namespace with no interface up has no way out"
        );
        let watcher = subscribe().unwrap();
        let reader = subscribe().unwrap();
        bring_loopback_up();

        let mut wakes = 0;
        listen(&watcher, || {
            wakes += 1;
            ControlFlow::Break(())
        });
        assert_eq!(wakes, 1);

        let families = address_families(&reader);
        assert!(families.contains(&libc::AF_INET), "{families:?}");
        // A kernel built without IPv6 has no `::1` to add.
        if std::path::Path::new("/proc/sys/net/ipv6").exists() {
            assert!(families.contains(&libc::AF_INET6), "{families:?}");
        }
    }

    fn bring_loopback_up() {
        // SAFETY: constant arguments; the result is checked.
        let raw = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
        assert!(raw >= 0, "{}", io::Error::last_os_error());
        // SAFETY: `raw` is a socket this call just opened.
        let socket = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: all-zero is a valid `ifreq`.
        let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
        for (to, from) in request.ifr_name.iter_mut().zip(b"lo") {
            *to = *from as libc::c_char;
        }
        // SAFETY: `request` names an interface and is live for both calls;
        // the flags are the union member these two requests use.
        unsafe {
            assert_eq!(
                libc::ioctl(socket.as_raw_fd(), libc::SIOCGIFFLAGS, &raw mut request),
                0
            );
            request.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
            assert_eq!(
                libc::ioctl(socket.as_raw_fd(), libc::SIOCSIFFLAGS, &raw const request),
                0,
                "{}",
                io::Error::last_os_error()
            );
        }
    }

    /// The address family of every `RTM_NEWADDR` already queued. The kernel
    /// queues them before the `ioctl` returns.
    fn address_families(socket: &OwnedFd) -> Vec<i32> {
        let header = size_of::<libc::nlmsghdr>();
        let mut families = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            // SAFETY: `buffer` is writable for the length passed.
            let read = unsafe {
                libc::recv(
                    socket.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    libc::MSG_DONTWAIT,
                )
            };
            let Ok(read) = usize::try_from(read) else {
                break;
            };
            let mut rest = buffer.get(..read).unwrap();
            while rest.len() >= header {
                let length = u32::from_ne_bytes(rest[..4].try_into().unwrap()) as usize;
                let kind = u16::from_ne_bytes(rest[4..6].try_into().unwrap());
                if kind == libc::RTM_NEWADDR {
                    families.push(i32::from(rest[header]));
                }
                let step = length.next_multiple_of(4).max(header);
                rest = rest.get(step..).unwrap_or_default();
            }
        }
        families
    }
}

#[cfg(windows)]
mod windows {
    use super::super::backend::{Listener, subscribe};
    use std::sync::{Condvar, Mutex};

    static HEARD: Mutex<u32> = Mutex::new(0);
    static WAKE: Condvar = Condvar::new();
    static COUNTER: Listener = Listener(heard);

    fn heard() {
        *HEARD.lock().unwrap() += 1;
        WAKE.notify_all();
    }

    #[test]
    fn interface_address_and_route_notifications_register_for_both_ip_versions() {
        let subscription = subscribe(&COUNTER, true).unwrap();
        let mut heard = HEARD.lock().unwrap();
        while *heard < 3 {
            heard = WAKE.wait(heard).unwrap();
        }
        drop(heard);
        drop(subscription);
    }
}

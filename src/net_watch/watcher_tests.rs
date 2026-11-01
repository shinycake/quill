//! The OS watchers against a real kernel. CI runs the Linux one in a
//! fresh network namespace and the Windows one on a Windows runner.

#[test]
fn a_refused_notification_keeps_the_ones_that_registered() {
    let kept = super::keep_working(["interface", "address", "route"], |kind| {
        if *kind == "address" {
            Err(std::io::Error::other("refused"))
        } else {
            Ok(*kind)
        }
    });
    assert_eq!(kept, ["interface", "route"]);
}

#[cfg(target_os = "linux")]
mod linux {
    use super::super::backend::{heard, listen, subscribe};
    use crate::net_watch::link::Link;
    use crate::net_watch::netlink::{self, Request};
    use crate::net_watch::{Hop, Path};
    use std::io;
    use std::net::Ipv4Addr;
    use std::ops::ControlFlow;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    const CHILD: &str = "QUILL_NET_WATCH_IN_NAMESPACE";

    #[test]
    #[ignore = "needs user namespaces; CI turns them on and runs it"]
    fn a_link_coming_up_wakes_the_watcher_in_a_fresh_network_namespace() {
        run_in_a_fresh_namespace("in_the_namespace");
    }

    #[test]
    #[ignore = "needs user namespaces; CI turns them on and runs it"]
    fn the_hop_to_telegram_follows_the_policy_rules_in_a_fresh_network_namespace() {
        run_in_a_fresh_namespace("in_the_namespace_the_hop_follows_the_policy_rules");
    }

    /// Runs the test `name` of this module again in a new user and network
    /// namespace.
    fn run_in_a_fresh_namespace(name: &str) {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                &format!("net_watch::platform::watcher_tests::linux::{name}"),
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
        listen(&watcher, |_| {
            wakes += 1;
            ControlFlow::Break(())
        });
        assert_eq!(wakes, 1);

        let families = address_families(&reader);
        assert!(families.contains(&libc::AF_INET), "{families:?}");
        // A kernel built without IPv6 has no `::1` to add. CI's has.
        if std::path::Path::new("/proc/sys/net/ipv6").exists() {
            assert!(families.contains(&libc::AF_INET6), "{families:?}");
        } else {
            assert!(
                std::env::var_os("CI").is_none(),
                "this runner has no IPv6, so the IPv6 events went untested"
            );
        }
    }

    #[test]
    #[ignore = "run by the namespace test above"]
    fn in_the_namespace_the_hop_follows_the_policy_rules() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        let epoch = || crate::locks::lock_or_recover(super::super::super::tracker()).epoch;
        let before = epoch();
        let watcher = subscribe().unwrap();
        let socket = netlink::open(0).unwrap();
        // A bridge with no ports: a link with a neighbour table, which
        // loopback has not, and no way out of the namespace.
        let link = add_bridge(&socket, c"quill0");
        let ip = |last| Ipv4Addr::new(192, 0, 2, last);
        add_address(&socket, link, ip(2));
        // The main table says .1; a rule sends Telegram's range to .9, as a
        // VPN or a split tunnel does.
        add_route(&socket, link, libc::RT_TABLE_MAIN, None, ip(1));
        let telegram = (Ipv4Addr::new(149, 154, 0, 0), 16);
        add_route(&socket, link, 100, Some(telegram), ip(9));
        add_rule(&socket, telegram, 100);
        let hop = |gateway_hardware| {
            Path::Via(Hop {
                local: ip(2).into(),
                link: Link {
                    interface: link,
                    gateway: Some(ip(9).into()),
                    gateway_hardware,
                },
            })
        };
        assert_eq!(super::super::super::route_now().v4, hop(None));
        // The watcher hears the new route and moves the epoch TDLib reads.
        // Every change above is queued on `watcher` already, and a new
        // bridge's own forwarding entries arrive as neighbour news, which
        // the filter drops: read on to the first change it lets through.
        listen(&watcher, |batch| {
            heard(batch);
            if netlink::only_other_neighbours(batch, |_| false) {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        });
        assert!(epoch() > before);
        assert_eq!(
            crate::locks::lock_or_recover(super::super::super::tracker())
                .route
                .v4,
            hop(None)
        );
        // The gateway's link-layer address, once the kernel knows it.
        let router = [0x02, 0, 0, 0, 0, 0x09];
        add_neighbour(&socket, link, ip(9), router);
        assert_eq!(super::super::super::route_now().v4, hop(Some(router)));
    }

    /// `ip neigh add <address> lladdr <hardware> dev <interface> nud permanent`.
    fn add_neighbour(socket: &OwnedFd, interface: u32, address: Ipv4Addr, hardware: [u8; 6]) {
        let mut header = vec![libc::AF_INET as u8, 0, 0, 0];
        header.extend_from_slice(&interface.to_ne_bytes());
        header.extend_from_slice(&libc::NUD_PERMANENT.to_ne_bytes());
        header.extend_from_slice(&[0, 0]);
        let flags = (libc::NLM_F_CREATE | libc::NLM_F_REPLACE) as u16;
        Request::new(libc::RTM_NEWNEIGH, flags, &header)
            .attribute(libc::NDA_DST, &address.octets())
            .attribute(libc::NDA_LLADDR, &hardware)
            .send(socket)
            .unwrap();
    }

    /// `ip link add <name> up type bridge`, and its index.
    fn add_bridge(socket: &OwnedFd, name: &std::ffi::CStr) -> u32 {
        const IFLA_IFNAME: u16 = 3;
        const IFLA_LINKINFO: u16 = 18;
        const IFLA_INFO_KIND: u16 = 1;
        let up = libc::IFF_UP as u32;
        let mut header = vec![libc::AF_UNSPEC as u8, 0, 0, 0, 0, 0, 0, 0];
        header.extend_from_slice(&up.to_ne_bytes());
        header.extend_from_slice(&up.to_ne_bytes());
        let mut kind = Vec::new();
        kind.extend_from_slice(&(4u16 + 6).to_ne_bytes());
        kind.extend_from_slice(&IFLA_INFO_KIND.to_ne_bytes());
        kind.extend_from_slice(b"bridge\0\0");
        let flags = (libc::NLM_F_CREATE | libc::NLM_F_EXCL) as u16;
        Request::new(libc::RTM_NEWLINK, flags, &header)
            .attribute(IFLA_IFNAME, name.to_bytes_with_nul())
            .attribute(IFLA_LINKINFO, &kind)
            .send(socket)
            .unwrap();
        // SAFETY: a NUL-terminated name.
        let index = unsafe { libc::if_nametoindex(name.as_ptr()) };
        assert_ne!(index, 0);
        index
    }

    fn add_address(socket: &OwnedFd, interface: u32, local: Ipv4Addr) {
        let mut header = vec![libc::AF_INET as u8, 32, 0, 0];
        header.extend_from_slice(&interface.to_ne_bytes());
        let flags = (libc::NLM_F_CREATE | libc::NLM_F_EXCL) as u16;
        Request::new(libc::RTM_NEWADDR, flags, &header)
            .attribute(libc::IFA_LOCAL, &local.octets())
            .attribute(libc::IFA_ADDRESS, &local.octets())
            .send(socket)
            .unwrap();
    }

    /// A route on `interface` through `gateway`, taken as on the link.
    fn add_route(
        socket: &OwnedFd,
        interface: u32,
        table: u8,
        to: Option<(Ipv4Addr, u8)>,
        gateway: Ipv4Addr,
    ) {
        const ONLINK: u32 = 4;
        let length = to.map_or(0, |(_, length)| length);
        let mut header = vec![
            libc::AF_INET as u8,
            length,
            0,
            0,
            table,
            libc::RTPROT_BOOT,
            libc::RT_SCOPE_UNIVERSE,
            libc::RTN_UNICAST,
        ];
        header.extend_from_slice(&ONLINK.to_ne_bytes());
        let flags = (libc::NLM_F_CREATE | libc::NLM_F_EXCL) as u16;
        let mut request = Request::new(libc::RTM_NEWROUTE, flags, &header)
            .attribute(libc::RTA_GATEWAY, &gateway.octets())
            .attribute(libc::RTA_OIF, &interface.to_ne_bytes());
        if let Some((to, _)) = to {
            request = request.attribute(libc::RTA_DST, &to.octets());
        }
        request.send(socket).unwrap();
    }

    /// `ip rule add to <to> table <table> priority 100`.
    fn add_rule(socket: &OwnedFd, (to, length): (Ipv4Addr, u8), table: u8) {
        const TO_TABLE: u8 = 1;
        const FRA_DST: u16 = 1;
        const FRA_PRIORITY: u16 = 6;
        let mut header = vec![libc::AF_INET as u8, length, 0, 0, table, 0, 0, TO_TABLE];
        header.extend_from_slice(&0u32.to_ne_bytes());
        let flags = (libc::NLM_F_CREATE | libc::NLM_F_EXCL) as u16;
        Request::new(libc::RTM_NEWRULE, flags, &header)
            .attribute(FRA_DST, &to.octets())
            .attribute(FRA_PRIORITY, &100u32.to_ne_bytes())
            .send(socket)
            .unwrap();
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
    use super::super::backend::{Change, Kind, Listener, subscribe};
    use std::net::Ipv6Addr;
    use std::sync::{Condvar, Mutex};
    use windows_sys::Win32::Foundation::NO_ERROR;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        CreateUnicastIpAddressEntry, DeleteUnicastIpAddressEntry, InitializeUnicastIpAddressEntry,
        MIB_UNICASTIPADDRESS_ROW, MibAddInstance,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET6;

    static HEARD: Mutex<Vec<Change>> = Mutex::new(Vec::new());
    static WAKE: Condvar = Condvar::new();
    static RECORDER: Listener = Listener(heard);

    fn heard(change: Change) {
        HEARD.lock().unwrap().push(change);
        WAKE.notify_all();
    }

    /// An address on the loopback interface (index 1 on Windows), removed
    /// on drop. Adding one takes an administrator, which the runner is.
    struct LoopbackAddress(MIB_UNICASTIPADDRESS_ROW);

    impl LoopbackAddress {
        fn add(ip: Ipv6Addr) -> Self {
            let mut row = MIB_UNICASTIPADDRESS_ROW::default();
            // SAFETY: `row` is live and writable.
            unsafe { InitializeUnicastIpAddressEntry(&raw mut row) };
            row.Address.Ipv6.sin6_family = AF_INET6;
            row.Address.Ipv6.sin6_addr.u.Byte = ip.octets();
            row.InterfaceIndex = 1;
            row.OnLinkPrefixLength = 128;
            // SAFETY: `row` is a live, filled-in row.
            let status = unsafe { CreateUnicastIpAddressEntry(&raw const row) };
            assert_eq!(status, NO_ERROR, "adding {ip} to loopback");
            Self(row)
        }
    }

    impl Drop for LoopbackAddress {
        fn drop(&mut self) {
            // SAFETY: the row this address was added with.
            unsafe { DeleteUnicastIpAddressEntry(&raw const self.0) };
        }
    }

    /// N21: `NotifyAddrChange` never heard this. The CI step's own time
    /// limit ends the wait if no notification comes.
    #[test]
    fn an_ipv6_address_added_on_loopback_wakes_the_watcher() {
        let subscription = subscribe(&RECORDER);
        let added = Change {
            kind: Kind::Address,
            family: AF_INET6,
            what: MibAddInstance,
        };
        let address = LoopbackAddress::add(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0x5175, 0x1));
        let mut heard = HEARD.lock().unwrap();
        while !heard.contains(&added) {
            heard = WAKE.wait(heard).unwrap();
        }
        drop(heard);
        drop(address);
        drop(subscription);
    }
}

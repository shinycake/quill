//! Which interface and gateway carry traffic to Telegram. Two networks can
//! hand out the same local address and even the same gateway address (two
//! home routers on 192.168.0.1), and TDLib's connections do not survive the
//! move between them. The gateway's link-layer address tells them apart.

use std::net::IpAddr;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Link {
    /// The OS interface index; 0 when unknown.
    pub(super) interface: u32,
    pub(super) gateway: Option<IpAddr>,
    /// The gateway's link-layer address; `None` until the system has
    /// resolved it, which is no change of network ([`super::Path::settled`]).
    pub(super) gateway_hardware: Option<[u8; 6]>,
}

/// The link `local` sends to `to` from.
pub(super) fn of(local: IpAddr, to: IpAddr) -> Link {
    backend::of(local, to)
}

/// Makes the system look up `gateway`'s link-layer address now rather than
/// on the next packet that happens to pass it: one empty datagram to the
/// discard port. Windows already resolves in [`of`].
pub(super) fn resolve(gateway: IpAddr) {
    if cfg!(windows) {
        return;
    }
    let any: IpAddr = if gateway.is_ipv6() {
        std::net::Ipv6Addr::UNSPECIFIED.into()
    } else {
        std::net::Ipv4Addr::UNSPECIFIED.into()
    };
    if let Ok(socket) = std::net::UdpSocket::bind((any, 0)) {
        let _ = socket.send_to(&[], (gateway, 9));
    }
}

#[cfg(target_os = "macos")]
mod unix {
    use std::ffi::CStr;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    /// The name and index of the interface that holds `local`.
    pub(super) fn interface_of(local: IpAddr) -> Option<(String, u32)> {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        // SAFETY: `list` is a live out-pointer; freed below on success.
        if unsafe { libc::getifaddrs(&raw mut list) } != 0 {
            return None;
        }
        let mut found = None;
        let mut entry = list;
        while !entry.is_null() {
            // SAFETY: a non-null node of the list `getifaddrs` returned.
            let item = unsafe { &*entry };
            if !item.ifa_name.is_null() && address(item.ifa_addr) == Some(local) {
                // SAFETY: a non-null, NUL-terminated name in the same list.
                let name = unsafe { CStr::from_ptr(item.ifa_name) };
                // SAFETY: the same name, live for the call.
                let index = unsafe { libc::if_nametoindex(item.ifa_name) };
                found = Some((name.to_string_lossy().into_owned(), index));
                break;
            }
            entry = item.ifa_next;
        }
        // SAFETY: the list from the successful call above, freed once.
        unsafe { libc::freeifaddrs(list) };
        found
    }

    fn address(raw: *const libc::sockaddr) -> Option<IpAddr> {
        if raw.is_null() {
            return None;
        }
        // SAFETY: a non-null address from `getifaddrs`; its family says
        // which larger struct it is.
        unsafe {
            match i32::from((*raw).sa_family) {
                libc::AF_INET => {
                    let v4 = &*raw.cast::<libc::sockaddr_in>();
                    Some(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr)).into())
                }
                libc::AF_INET6 => {
                    let v6 = &*raw.cast::<libc::sockaddr_in6>();
                    Some(Ipv6Addr::from(v6.sin6_addr.s6_addr).into())
                }
                _ => None,
            }
        }
    }
}

/// The kernel's own route lookup toward `to`, which goes through the
/// policy rules (VPN tables, per-connection tables, split tunnels) as a
/// real packet does.
#[cfg(target_os = "linux")]
mod backend {
    use super::Link;
    use crate::net_watch::netlink;
    use std::net::IpAddr;

    pub(super) fn of(_local: IpAddr, to: IpAddr) -> Link {
        let Ok(socket) = netlink::open(0) else {
            return Link::default();
        };
        let Some((interface, gateway)) = netlink::route_to(&socket, to) else {
            return Link::default();
        };
        Link {
            interface,
            gateway,
            gateway_hardware: gateway
                .and_then(|gateway| netlink::neighbour(&socket, interface, gateway)),
        }
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use super::Link;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;
    use std::net::IpAddr;

    #[link(name = "SystemConfiguration", kind = "framework")]
    unsafe extern "C" {
        fn SCDynamicStoreCopyValue(store: *const c_void, key: CFStringRef) -> CFDictionaryRef;
    }

    pub(super) fn of(local: IpAddr, _to: IpAddr) -> Link {
        let Some((name, interface)) = super::unix::interface_of(local) else {
            return Link::default();
        };
        let Some(global) = store_value(if local.is_ipv4() {
            "State:/Network/Global/IPv4"
        } else {
            "State:/Network/Global/IPv6"
        }) else {
            return Link {
                interface,
                ..Link::default()
            };
        };
        // Only the primary service's router is known here.
        if text(&global, "PrimaryInterface").as_deref() != Some(name.as_str()) {
            return Link {
                interface,
                ..Link::default()
            };
        }
        // configd may write a link-local router with its zone.
        let gateway: Option<IpAddr> = text(&global, "Router").and_then(|router| {
            router
                .split('%')
                .next()
                .and_then(|router| router.parse().ok())
        });
        let gateway_hardware = match gateway {
            // configd records the router's address once ARP has resolved
            // it, and IPv4 only (`kSCPropNetIPv4ARPResolvedHardwareAddress`).
            Some(IpAddr::V4(_)) => text(&global, "PrimaryService")
                .and_then(|service| store_value(&format!("State:/Network/Service/{service}/IPv4")))
                .and_then(|service| text(&service, "ARPResolvedHardwareAddress"))
                .and_then(|text| super::hardware_from_text(&text)),
            Some(IpAddr::V6(router)) => super::super::llinfo::hardware(interface, router),
            None => None,
        };
        Link {
            interface,
            gateway,
            gateway_hardware,
        }
    }

    fn store_value(key: &str) -> Option<CFDictionary<CFString, CFType>> {
        let key = CFString::new(key);
        // SAFETY: a null store uses a session of its own; `key` is live.
        let value = unsafe { SCDynamicStoreCopyValue(std::ptr::null(), key.as_concrete_TypeRef()) };
        if value.is_null() {
            return None;
        }
        // SAFETY: a non-null dictionary from a Copy call, owned from here on.
        Some(unsafe { CFDictionary::wrap_under_create_rule(value) })
    }

    fn text(dictionary: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
        dictionary
            .find(CFString::new(key))
            .and_then(|value| value.downcast::<CFString>())
            .map(|text| text.to_string())
    }
}

/// `0:1b:63:a:b:c` as configd writes it, leading zeros left out.
#[cfg(any(target_os = "macos", test))]
fn hardware_from_text(text: &str) -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    let mut parts = text.split(':');
    for byte in &mut out {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 2 {
            return None;
        }
        *byte = u8::from_str_radix(part, 16).ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// One neighbour resolve in flight per IP version, so a slow ARP answer
/// does not hold up the IPv6 router's, nor the other way round.
#[cfg(any(windows, test))]
struct Resolving {
    v4: std::sync::atomic::AtomicBool,
    v6: std::sync::atomic::AtomicBool,
}

/// A resolve in flight; its IP version is free again on drop, including
/// when the thread that holds it never starts.
#[cfg(any(windows, test))]
struct Resolve(&'static std::sync::atomic::AtomicBool);

#[cfg(any(windows, test))]
impl Resolving {
    const fn new() -> Self {
        Self {
            v4: std::sync::atomic::AtomicBool::new(false),
            v6: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn begin(&'static self, gateway: IpAddr) -> Option<Resolve> {
        let flag = if gateway.is_ipv6() { &self.v6 } else { &self.v4 };
        (!flag.swap(true, std::sync::atomic::Ordering::AcqRel)).then_some(Resolve(flag))
    }
}

#[cfg(any(windows, test))]
impl Drop for Resolve {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(windows)]
mod backend {
    use super::Link;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use windows_sys::Win32::Foundation::NO_ERROR;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetBestRoute2, GetIpNetEntry2, MIB_IPFORWARD_ROW2, MIB_IPNET_ROW2, ResolveIpNetEntry2,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, NlnsProbe, SOCKADDR_INET};

    pub(super) fn of(_local: IpAddr, to: IpAddr) -> Link {
        let destination = socket_address(to);
        let mut route = MIB_IPFORWARD_ROW2::default();
        let mut source = SOCKADDR_INET::default();
        // SAFETY: null asks for any interface; the three structs are live
        // for the call.
        let status = unsafe {
            GetBestRoute2(
                std::ptr::null(),
                0,
                std::ptr::null(),
                &raw const destination,
                0,
                &raw mut route,
                &raw mut source,
            )
        };
        if status != NO_ERROR {
            return Link::default();
        }
        let gateway = ip_of(&route.NextHop).filter(|hop| !hop.is_unspecified());
        Link {
            interface: route.InterfaceIndex,
            gateway,
            gateway_hardware: gateway.and_then(|gateway| hardware(route.InterfaceIndex, gateway)),
        }
    }

    /// The gateway's entry in the neighbour cache. Windows has no
    /// notification for that cache, so a missing entry is resolved in the
    /// background and the network looked at again once it is there.
    fn hardware(interface: u32, gateway: IpAddr) -> Option<[u8; 6]> {
        let mut row = MIB_IPNET_ROW2 {
            Address: socket_address(gateway),
            InterfaceIndex: interface,
            ..MIB_IPNET_ROW2::default()
        };
        // SAFETY: `row` is live and names the entry to read.
        let found = unsafe { GetIpNetEntry2(&raw mut row) } == NO_ERROR && row.State >= NlnsProbe;
        if !found {
            resolve_later(row);
            return None;
        }
        if row.PhysicalAddressLength != 6 {
            return None;
        }
        row.PhysicalAddress.get(..6)?.try_into().ok()
    }

    static RESOLVING: super::Resolving = super::Resolving::new();

    fn resolve_later(mut row: MIB_IPNET_ROW2) {
        let Some(gateway) = ip_of(&row.Address) else {
            return;
        };
        let Some(resolving) = RESOLVING.begin(gateway) else {
            return;
        };
        let _ = crate::thread_guard::spawn_guarded("quill-net-neighbour", move || {
            // SAFETY: `row` is live and names the entry; a null source lets
            // Windows pick one. The call blocks until ARP or ND answers.
            let resolved =
                unsafe { ResolveIpNetEntry2(&raw mut row, std::ptr::null()) } == NO_ERROR;
            drop(resolving);
            if resolved {
                super::super::network_changed();
            }
        });
    }

    fn socket_address(ip: IpAddr) -> SOCKADDR_INET {
        let mut out = SOCKADDR_INET::default();
        match ip {
            IpAddr::V4(v4) => {
                out.Ipv4.sin_family = AF_INET;
                out.Ipv4.sin_addr.S_un.S_addr = u32::from_ne_bytes(v4.octets());
            }
            IpAddr::V6(v6) => {
                out.Ipv6.sin6_family = AF_INET6;
                out.Ipv6.sin6_addr.u.Byte = v6.octets();
            }
        }
        out
    }

    fn ip_of(address: &SOCKADDR_INET) -> Option<IpAddr> {
        // SAFETY: every member starts with the family, which says which
        // member the OS filled.
        unsafe {
            match address.si_family {
                AF_INET => {
                    Some(Ipv4Addr::from(address.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()).into())
                }
                AF_INET6 => Some(Ipv6Addr::from(address.Ipv6.sin6_addr.u.Byte).into()),
                _ => None,
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod backend {
    use super::Link;
    use std::net::IpAddr;

    pub(super) fn of(_local: IpAddr, _to: IpAddr) -> Link {
        Link::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_ipv4_router_still_resolving_does_not_hold_up_the_ipv6_one() {
        static RESOLVING: super::Resolving = super::Resolving::new();
        let v4: std::net::IpAddr = "192.168.0.1".parse().unwrap();
        let v6: std::net::IpAddr = "fe80::1".parse().unwrap();
        let arp = RESOLVING.begin(v4).expect("the IPv4 resolve starts");
        assert!(RESOLVING.begin(v4).is_none(), "one IPv4 resolve at a time");
        let nd = RESOLVING.begin(v6).expect("the IPv6 resolve starts as well");
        drop(arp);
        assert!(RESOLVING.begin(v4).is_some(), "free again once it ends");
        drop(nd);
    }

    #[test]
    fn a_hardware_address_reads_with_or_without_leading_zeros() {
        let router = Some([0x00, 0x1b, 0x63, 0x0a, 0xbc, 0xde]);
        assert_eq!(super::hardware_from_text("0:1b:63:a:bc:de"), router);
        assert_eq!(super::hardware_from_text("00:1b:63:0a:bc:de"), router);
        assert_eq!(super::hardware_from_text("0:1b:63:a:bc"), None);
        assert_eq!(super::hardware_from_text("0:1b:63:a:bc:de:f"), None);
        assert_eq!(super::hardware_from_text("0:1b:63:a:bc:xyz"), None);
    }
}

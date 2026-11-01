//! Which interface and gateway carry traffic to Telegram. Two networks can
//! hand out the same local address (two home routers on 192.168.0.x), and
//! TDLib's connections do not survive the move between them.

use std::net::IpAddr;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Link {
    /// The OS interface index; 0 when unknown.
    pub(super) interface: u32,
    pub(super) gateway: Option<IpAddr>,
}

/// The link `local` sends to `to` from.
pub(super) fn of(local: IpAddr, to: IpAddr) -> Link {
    backend::of(local, to)
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
        Link { interface, gateway }
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
        Link {
            interface,
            gateway: router(&name, local.is_ipv4()),
        }
    }

    /// The primary service's router, when `name` is its interface.
    fn router(name: &str, v4: bool) -> Option<IpAddr> {
        let key = CFString::new(if v4 {
            "State:/Network/Global/IPv4"
        } else {
            "State:/Network/Global/IPv6"
        });
        // SAFETY: a null store uses a session of its own; `key` is live.
        let value = unsafe { SCDynamicStoreCopyValue(std::ptr::null(), key.as_concrete_TypeRef()) };
        if value.is_null() {
            return None;
        }
        // SAFETY: a non-null dictionary from a Copy call, owned from here on.
        let global: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_create_rule(value) };
        let text = |key: &str| {
            global
                .find(CFString::new(key))
                .and_then(|value| value.downcast::<CFString>())
                .map(|text| text.to_string())
        };
        if text("PrimaryInterface")? != name {
            return None;
        }
        text("Router")?.parse().ok()
    }
}

#[cfg(windows)]
mod backend {
    use super::Link;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use windows_sys::Win32::Foundation::NO_ERROR;
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetBestRoute2, MIB_IPFORWARD_ROW2};
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, SOCKADDR_INET};

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
        Link {
            interface: route.InterfaceIndex,
            gateway: ip_of(&route.NextHop).filter(|hop| !hop.is_unspecified()),
        }
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

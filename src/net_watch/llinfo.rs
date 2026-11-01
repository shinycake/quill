//! A router's link-layer address from the BSD routing table's neighbour
//! entries (`RTF_LLINFO`), which hold NDP answers as well as ARP ones.
//! configd records only the IPv4 router's (`net_watch::link`).

use std::net::Ipv6Addr;

/// `AF_INET6` and `AF_LINK` on macOS and the other BSDs.
const AF_INET6: u8 = 30;
const AF_LINK: u8 = 18;
const RTA_DST: i32 = 0x1;
const RTA_GATEWAY: i32 = 0x2;

#[cfg(target_os = "macos")]
const _: () = assert!(libc::AF_INET6 == AF_INET6 as i32 && libc::AF_LINK == AF_LINK as i32);

/// Where the fields this file reads sit in a `rt_msghdr`.
#[derive(Clone, Copy)]
pub(super) struct Header {
    pub(super) size: usize,
    pub(super) addrs_at: usize,
}

#[cfg(target_os = "macos")]
const HEADER: Header = Header {
    size: size_of::<libc::rt_msghdr>(),
    addrs_at: std::mem::offset_of!(libc::rt_msghdr, rtm_addrs),
};

/// The IPv6 neighbour entry for `router` on `interface`, read from the
/// kernel's table.
#[cfg(target_os = "macos")]
pub(super) fn hardware(interface: u32, router: Ipv6Addr) -> Option<[u8; 6]> {
    let mut mib = [
        libc::CTL_NET,
        libc::PF_ROUTE,
        0,
        libc::AF_INET6,
        libc::NET_RT_FLAGS,
        libc::RTF_LLINFO,
    ];
    let mut size = 0usize;
    // SAFETY: `mib` is live for the call; a null buffer asks for the size.
    let sized = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            std::ptr::null_mut(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if sized != 0 || size == 0 {
        return None;
    }
    // Room for entries the kernel adds between the two calls.
    let mut table = vec![0u8; size.saturating_add(size / 4)];
    let mut size = table.len();
    // SAFETY: `table` is writable for `size` bytes and both are live.
    let read = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            table.as_mut_ptr().cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if read != 0 {
        return None;
    }
    find(table.get(..size)?, HEADER, interface, router)
}

/// The link-layer address of `router`'s entry on `interface` (0 for any)
/// in a `NET_RT_FLAGS` dump.
pub(super) fn find(
    mut table: &[u8],
    header: Header,
    interface: u32,
    router: Ipv6Addr,
) -> Option<[u8; 6]> {
    let router = unscoped(router.octets());
    while let Some(length) = table.get(..2) {
        let length = usize::from(u16::from_ne_bytes(length.try_into().ok()?));
        let message = table.get(..length)?;
        if length == 0 {
            return None;
        }
        table = table.get(length..).unwrap_or_default();
        if let Some(found) = entry(message, header, interface, router) {
            return Some(found);
        }
    }
    None
}

fn entry(message: &[u8], header: Header, interface: u32, router: [u8; 16]) -> Option<[u8; 6]> {
    let addrs = message.get(header.addrs_at..header.addrs_at.checked_add(4)?)?;
    let addrs = i32::from_ne_bytes(addrs.try_into().ok()?);
    if addrs & (RTA_DST | RTA_GATEWAY) != RTA_DST | RTA_GATEWAY {
        return None;
    }
    let destination = message.get(header.size..)?;
    let destination_length = usize::from(*destination.first()?);
    let gateway = destination.get(rounded(destination_length)..)?;
    if *destination.get(1)? != AF_INET6 {
        return None;
    }
    let address: [u8; 16] = destination.get(8..24)?.try_into().ok()?;
    if unscoped(address) != router {
        return None;
    }
    if *gateway.get(1)? != AF_LINK {
        return None;
    }
    let index = u32::from(u16::from_ne_bytes(gateway.get(2..4)?.try_into().ok()?));
    if interface != 0 && index != interface {
        return None;
    }
    let name_length = usize::from(*gateway.get(5)?);
    if *gateway.get(6)? != 6 {
        return None;
    }
    let start = name_length.checked_add(8)?;
    gateway.get(start..start.checked_add(6)?)?.try_into().ok()
}

/// The kernel writes a link-local address with its interface index in
/// bytes 2 and 3 (the KAME scope); configd writes it without.
fn unscoped(mut address: [u8; 16]) -> [u8; 16] {
    let [first, second, ..] = address;
    if first == 0xfe
        && second & 0xc0 == 0x80
        && let Some(scope) = address.get_mut(2..4)
    {
        scope.fill(0);
    }
    address
}

/// A socket address takes a multiple of 4 bytes in a routing message.
fn rounded(length: usize) -> usize {
    if length == 0 {
        4
    } else {
        (length.saturating_sub(1) | 3).saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::{AF_INET6, AF_LINK, Header, find};
    use std::net::Ipv6Addr;

    /// macOS's `rt_msghdr`: 92 bytes, `rtm_addrs` at 12.
    const HEADER: Header = Header {
        size: 92,
        addrs_at: 12,
    };

    /// One `RTF_LLINFO` entry as the kernel writes it.
    fn entry(address: Ipv6Addr, scope: u16, interface: u16, hardware: [u8; 6]) -> Vec<u8> {
        let mut message = vec![0u8; HEADER.size];
        message[12..16].copy_from_slice(&3i32.to_ne_bytes());
        let mut destination = vec![0u8; 28];
        destination[0] = 28;
        destination[1] = AF_INET6;
        destination[8..24].copy_from_slice(&address.octets());
        destination[10..12].copy_from_slice(&scope.to_be_bytes());
        message.extend(destination);
        let name = b"en0";
        let mut gateway = vec![0u8; 20];
        gateway[0] = 20;
        gateway[1] = AF_LINK;
        gateway[2..4].copy_from_slice(&interface.to_ne_bytes());
        gateway[5] = name.len() as u8;
        gateway[6] = 6;
        gateway[8..11].copy_from_slice(name);
        gateway[11..17].copy_from_slice(&hardware);
        message.extend(gateway);
        let length = message.len() as u16;
        message[..2].copy_from_slice(&length.to_ne_bytes());
        message
    }

    #[test]
    fn two_ipv6_routers_on_one_address_read_as_two_networks() {
        let router: Ipv6Addr = "fe80::1".parse().unwrap();
        let home = [0x00, 0x1b, 0x63, 0x0a, 0xbc, 0xde];
        let office = [0x3c, 0x22, 0xfb, 0x01, 0x02, 0x03];
        let other: Ipv6Addr = "fe80::2".parse().unwrap();
        let at_home = [entry(other, 4, 4, office), entry(router, 4, 4, home)].concat();
        let at_office = entry(router, 4, 4, office);
        assert_eq!(find(&at_home, HEADER, 4, router), Some(home));
        assert_eq!(find(&at_office, HEADER, 4, router), Some(office));
        assert_eq!(find(&at_home, HEADER, 5, router), None, "another interface");
        assert_eq!(find(&at_home[..40], HEADER, 4, router), None, "cut short");
    }
}

//! Requests to the kernel's routing tables on a `NETLINK_ROUTE` socket.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const HEADER: usize = size_of::<libc::nlmsghdr>();
/// `rtmsg`: family, the two prefix lengths, tos, table, protocol, scope,
/// type, then 32 bits of flags.
const ROUTE_HEADER: usize = 12;

/// A `NETLINK_ROUTE` socket that hears `groups` (0 for requests only).
pub(super) fn open(groups: u32) -> io::Result<OwnedFd> {
    // SAFETY: constant arguments; the result is checked before use.
    let raw = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_RAW | libc::SOCK_CLOEXEC,
            libc::NETLINK_ROUTE,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a socket this call just opened and nothing else owns.
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    // SAFETY: all-zero is a valid `sockaddr_nl`.
    let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    address.nl_family = libc::AF_NETLINK as libc::sa_family_t;
    address.nl_groups = groups;
    // SAFETY: `address` is a live `sockaddr_nl` of the size passed.
    let bound = unsafe {
        libc::bind(
            socket.as_raw_fd(),
            (&raw const address).cast(),
            size_of::<libc::sockaddr_nl>() as libc::socklen_t,
        )
    };
    if bound < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(socket)
}

pub(super) struct Message<'a> {
    pub(super) kind: u16,
    pub(super) body: &'a [u8],
}

/// The messages in one read.
pub(super) fn messages(mut bytes: &[u8]) -> impl Iterator<Item = Message<'_>> {
    std::iter::from_fn(move || {
        let length = usize::try_from(u32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?)).ok()?;
        let kind = u16::from_ne_bytes(bytes.get(4..6)?.try_into().ok()?);
        let body = bytes.get(HEADER..length)?;
        bytes = bytes.get(length.next_multiple_of(4)..).unwrap_or_default();
        Some(Message { kind, body })
    })
}

/// The attributes after the first `header` bytes of a message body.
pub(super) fn attributes(body: &[u8], header: usize) -> impl Iterator<Item = (u16, &[u8])> {
    let mut rest = body.get(header.next_multiple_of(4)..).unwrap_or_default();
    std::iter::from_fn(move || {
        let length = usize::from(u16::from_ne_bytes(rest.get(..2)?.try_into().ok()?));
        // The top two bits are the nested and byte-order flags.
        let kind = u16::from_ne_bytes(rest.get(2..4)?.try_into().ok()?) & 0x3fff;
        let value = rest.get(4..length)?;
        rest = rest.get(length.next_multiple_of(4)..).unwrap_or_default();
        Some((kind, value))
    })
}

/// One request and the messages the kernel answers it with.
pub(super) struct Request(Vec<u8>);

impl Request {
    /// A request of `kind` whose body starts with `header`.
    pub(super) fn new(kind: u16, flags: u16, header: &[u8]) -> Self {
        // A dump ends with `NLMSG_DONE`; anything else asks for an ack.
        // `NLM_F_DUMP` is two bits, one of which `NLM_F_EXCL` shares.
        let dump = libc::NLM_F_DUMP as u16;
        let ack = if flags & dump != dump {
            libc::NLM_F_ACK as u16
        } else {
            0
        };
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(&0u32.to_ne_bytes());
        bytes.extend_from_slice(&kind.to_ne_bytes());
        bytes.extend_from_slice(&(flags | libc::NLM_F_REQUEST as u16 | ack).to_ne_bytes());
        bytes.extend_from_slice(&1u32.to_ne_bytes());
        bytes.extend_from_slice(&0u32.to_ne_bytes());
        bytes.extend_from_slice(header);
        pad(&mut bytes);
        Self(bytes)
    }

    pub(super) fn attribute(mut self, kind: u16, value: &[u8]) -> Self {
        let length = u16::try_from(4 + value.len()).unwrap_or(u16::MAX);
        self.0.extend_from_slice(&length.to_ne_bytes());
        self.0.extend_from_slice(&kind.to_ne_bytes());
        self.0.extend_from_slice(value);
        pad(&mut self.0);
        self
    }

    /// The kind and body of every message in the answer.
    pub(super) fn send(mut self, socket: &OwnedFd) -> io::Result<Vec<(u16, Vec<u8>)>> {
        let length = u32::try_from(self.0.len()).unwrap_or(u32::MAX);
        if let Some(head) = self.0.get_mut(..4) {
            head.copy_from_slice(&length.to_ne_bytes());
        }
        // SAFETY: the request is live and readable for the length passed.
        let sent =
            unsafe { libc::send(socket.as_raw_fd(), self.0.as_ptr().cast(), self.0.len(), 0) };
        if sent < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut answer = Vec::new();
        let mut buffer = vec![0u8; 32 * 1024];
        loop {
            // SAFETY: `buffer` is writable for the length passed.
            let read = unsafe {
                libc::recv(
                    socket.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    0,
                )
            };
            let Ok(read) = usize::try_from(read) else {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            };
            for message in messages(buffer.get(..read).unwrap_or_default()) {
                match i32::from(message.kind) {
                    libc::NLMSG_DONE => return Ok(answer),
                    libc::NLMSG_ERROR => {
                        let code = message
                            .body
                            .get(..4)
                            .and_then(|code| code.try_into().ok())
                            .map_or(0, i32::from_ne_bytes);
                        return if code == 0 {
                            Ok(answer)
                        } else {
                            Err(io::Error::from_raw_os_error(code.saturating_neg()))
                        };
                    }
                    _ => answer.push((message.kind, message.body.to_vec())),
                }
            }
            if read == 0 {
                return Ok(answer);
            }
        }
    }
}

fn pad(bytes: &mut Vec<u8>) {
    bytes.resize(bytes.len().next_multiple_of(4), 0);
}

pub(super) fn family(ip: IpAddr) -> u8 {
    match ip {
        IpAddr::V4(_) => libc::AF_INET as u8,
        IpAddr::V6(_) => libc::AF_INET6 as u8,
    }
}

pub(super) fn octets(ip: IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(v4) => v4.octets().to_vec(),
        IpAddr::V6(v6) => v6.octets().to_vec(),
    }
}

pub(super) fn ip_from(bytes: &[u8]) -> Option<IpAddr> {
    if let Ok(v4) = <[u8; 4]>::try_from(bytes) {
        return Some(Ipv4Addr::from(v4).into());
    }
    <[u8; 16]>::try_from(bytes)
        .ok()
        .map(|v6| Ipv6Addr::from(v6).into())
}

/// The interface and next hop the kernel picks for a packet to `to`, as
/// `ip route get` reports it: through the policy rules, not only the main
/// table.
pub(super) fn route_to(socket: &OwnedFd, to: IpAddr) -> Option<(u32, Option<IpAddr>)> {
    let full = if to.is_ipv4() { 32 } else { 128 };
    let header: [u8; ROUTE_HEADER] = [family(to), full, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let answer = Request::new(libc::RTM_GETROUTE, 0, &header)
        .attribute(libc::RTA_DST, &octets(to))
        .send(socket)
        .ok()?;
    let (_, body) = answer
        .iter()
        .find(|(kind, _)| *kind == libc::RTM_NEWROUTE)?;
    let mut interface = None;
    let mut gateway = None;
    for (kind, value) in attributes(body, ROUTE_HEADER) {
        match kind {
            libc::RTA_OIF => {
                interface = value.try_into().ok().map(u32::from_ne_bytes);
            }
            libc::RTA_GATEWAY => gateway = ip_from(value),
            // An IPv4 route through an IPv6 next hop: a family, then the address.
            libc::RTA_VIA => gateway = value.get(2..).and_then(ip_from),
            _ => {}
        }
    }
    Some((interface?, gateway))
}

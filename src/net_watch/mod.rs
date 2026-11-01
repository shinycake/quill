//! What Quill tells TDLib about the network.
//!
//! TDLib holds on to its connections until it notices silence, which takes
//! over two minutes for a client that is not `online`. `setNetworkType` is
//! what makes it drop them (`StateManager::on_network`), and
//! `networkTypeNone` is what turns "Connecting" into "Waiting for network…".
//! tdesktop gets the same from `base::NetworkReachability`, restarting
//! MTProto on `availableChanges` (`mtp_instance.cpp`).
#![deny(clippy::indexing_slicing)]

mod link;
#[cfg(target_os = "linux")]
mod netlink;
mod platform;
#[cfg(test)]
mod tests;

use crate::locks::lock_or_recover;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, UdpSocket};
use std::sync::{Mutex, OnceLock};

const TELEGRAM_V4: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(149, 154, 167, 51), 443));
const TELEGRAM_V6: SocketAddr = SocketAddr::V6(SocketAddrV6::new(
    Ipv6Addr::new(0x2001, 0x67c, 0x4e8, 0xf002, 0, 0, 0, 0xa),
    443,
    0,
    0,
));

/// A kernel built or booted without IPv6 answers this to the socket call.
#[cfg(unix)]
const NO_SUCH_IP_VERSION: i32 = libc::EAFNOSUPPORT;
#[cfg(windows)]
const NO_SUCH_IP_VERSION: i32 = 10047; // WSAEAFNOSUPPORT

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Path {
    Via(Hop),
    NoRoute,
    Unknown,
}

/// Where traffic to Telegram leaves this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hop {
    local: IpAddr,
    link: link::Link,
}

impl Path {
    /// `self`, keeping the gateway's link-layer address from `before` when
    /// the system has not resolved it yet: an entry the cache dropped, or
    /// one not learned yet, is no new network. A different address is.
    fn settled(self, before: Self) -> Self {
        match (self, before) {
            (Self::Via(mut now), Self::Via(then))
                if now.link.gateway_hardware.is_none()
                    && Hop {
                        link: link::Link {
                            gateway_hardware: None,
                            ..then.link
                        },
                        ..then
                    } == now =>
            {
                now.link.gateway_hardware = then.link.gateway_hardware;
                Self::Via(now)
            }
            _ => self,
        }
    }

    fn of(lookup: io::Result<SocketAddr>, link: impl FnOnce(IpAddr) -> link::Link) -> Self {
        match lookup {
            Ok(local) if local.ip().is_unspecified() => Self::NoRoute,
            Ok(local) => Self::Via(Hop {
                local: local.ip(),
                link: link(local.ip()),
            }),
            Err(error)
                if error.raw_os_error() == Some(NO_SUCH_IP_VERSION)
                    || matches!(
                        error.kind(),
                        io::ErrorKind::NetworkUnreachable
                            | io::ErrorKind::HostUnreachable
                            | io::ErrorKind::AddrNotAvailable
                    ) =>
            {
                Self::NoRoute
            }
            Err(_) => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Route {
    v4: Path,
    v6: Path,
}

impl Route {
    fn settled(self, before: Self) -> Self {
        Self {
            v4: self.v4.settled(before.v4),
            v6: self.v6.settled(before.v6),
        }
    }

    #[cfg(target_os = "linux")]
    fn gateways(self) -> impl Iterator<Item = IpAddr> {
        [self.v4, self.v6]
            .into_iter()
            .filter_map(|path| match path {
                Path::Via(hop) => hop.link.gateway,
                _ => None,
            })
    }

    /// Offline takes proof on both IP versions: TDLib stops trying on
    /// `networkTypeNone`, so an odd failure must not read as "no network".
    fn available(self) -> bool {
        self.v4 != Path::NoRoute || self.v6 != Path::NoRoute
    }
}

/// `connect` on UDP only asks the routing table.
fn local_address(bind: IpAddr, to: SocketAddr) -> io::Result<SocketAddr> {
    let socket = UdpSocket::bind(SocketAddr::new(bind, 0))?;
    socket.connect(to)?;
    socket.local_addr()
}

fn route_now() -> Route {
    let path = |bind: IpAddr, to: SocketAddr| {
        Path::of(local_address(bind, to), |local| link::of(local, to.ip()))
    };
    Route {
        v4: path(Ipv4Addr::UNSPECIFIED.into(), TELEGRAM_V4),
        v6: path(Ipv6Addr::UNSPECIFIED.into(), TELEGRAM_V6),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkState {
    /// Moves whenever TDLib should hear about the network again.
    pub epoch: u64,
    pub available: bool,
}

#[derive(Debug)]
struct Tracker {
    route: Route,
    epoch: u64,
}

impl Tracker {
    fn new(route: Route) -> Self {
        Self { route, epoch: 0 }
    }

    fn observe(&mut self, route: Route) -> bool {
        let route = route.settled(self.route);
        let moved = route != self.route;
        self.route = route;
        self.epoch += u64::from(moved);
        moved
    }

    /// Connections do not survive a sleep, even when the route is the same.
    fn woke(&mut self, route: Route) {
        self.route = route.settled(self.route);
        self.epoch += 1;
    }

    /// `networkTypeNone` stops TDLib, so leaving that state must not hang on
    /// one event arriving: while offline every read looks again.
    fn state(&mut self, look: impl FnOnce() -> Route) -> NetworkState {
        if !self.route.available() {
            self.observe(look());
        }
        NetworkState {
            epoch: self.epoch,
            available: self.route.available(),
        }
    }
}

fn tracker() -> &'static Mutex<Tracker> {
    static TRACKER: OnceLock<Mutex<Tracker>> = OnceLock::new();
    TRACKER.get_or_init(|| {
        if let Err(error) = crate::thread_guard::spawn_guarded("quill-net-watch", platform::watch) {
            eprintln!("quill: no network watcher: {error}");
        }
        Mutex::new(Tracker::new(route_now()))
    })
}

/// The network as last seen. The first call starts the watcher.
pub fn latest() -> NetworkState {
    lock_or_recover(tracker()).state(route_now)
}

pub fn system_woke() {
    let route = route_now();
    lock_or_recover(tracker()).woke(route);
    crate::wake::UPDATES.poke();
    crate::system_proxy::refresh();
}

/// Whether `ip` is a gateway Telegram traffic leaves through.
#[cfg(target_os = "linux")]
fn is_gateway(ip: IpAddr) -> bool {
    lock_or_recover(tracker())
        .route
        .gateways()
        .any(|gateway| gateway == ip)
}

fn network_changed() {
    let route = route_now();
    let moved = lock_or_recover(tracker()).observe(route);
    if moved {
        crate::wake::UPDATES.poke();
    }
    if moved || platform::EVENTS_COVER_PROXY {
        crate::system_proxy::refresh();
    }
}

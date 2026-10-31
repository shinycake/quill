//! What Quill tells TDLib about the network.
//!
//! TDLib holds on to its connections until it notices silence, which takes
//! over two minutes for a client that is not `online`. `setNetworkType` is
//! what makes it drop them (`StateManager::on_network`), and
//! `networkTypeNone` is what turns "Connecting" into "Waiting for network…".
//! tdesktop gets the same from `base::NetworkReachability`, restarting
//! MTProto on `availableChanges` (`mtp_instance.cpp`).
//!
//! The state is the [`Route`] to Telegram: the local address the system
//! would use for each IP version, which the same `connect` finds on every
//! platform. Looking a route up sends nothing. A backend in [`platform`] only
//! says "look again", on a thread that blocks in the OS with no timer, and
//! the route decides whether anything happened: most events are not about
//! the way to Telegram.
#![deny(clippy::indexing_slicing)]

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Path {
    Via(IpAddr),
    NoRoute,
    /// The lookup failed in a way that says nothing about the network.
    Unknown,
}

impl Path {
    fn of(lookup: io::Result<SocketAddr>) -> Self {
        match lookup {
            Ok(local) if local.ip().is_unspecified() => Self::NoRoute,
            Ok(local) => Self::Via(local.ip()),
            Err(error)
                if matches!(
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
    /// Offline takes proof: TDLib stops trying on `networkTypeNone`, so an
    /// odd failure must not read as "no network".
    fn available(self) -> bool {
        let paths = [self.v4, self.v6];
        paths.iter().any(|path| matches!(path, Path::Via(_))) || !paths.contains(&Path::NoRoute)
    }
}

/// The local address a socket to `to` would use. `connect` on UDP only asks
/// the routing table.
fn local_address(bind: IpAddr, to: SocketAddr) -> io::Result<SocketAddr> {
    let socket = UdpSocket::bind(SocketAddr::new(bind, 0))?;
    socket.connect(to)?;
    socket.local_addr()
}

fn route_now() -> Route {
    Route {
        v4: Path::of(local_address(Ipv4Addr::UNSPECIFIED.into(), TELEGRAM_V4)),
        v6: Path::of(local_address(Ipv6Addr::UNSPECIFIED.into(), TELEGRAM_V6)),
    }
}

/// What the connection drivers read on each poll.
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
        let moved = route != self.route;
        self.route = route;
        self.epoch += u64::from(moved);
        moved
    }

    /// Connections do not survive a sleep, even when the route is the same.
    fn woke(&mut self, route: Route) {
        self.route = route;
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
        // A failed spawn only loses the events: wake-ups still arrive.
        let _ = crate::thread_guard::spawn_guarded("quill-net-watch", platform::watch);
        Mutex::new(Tracker::new(route_now()))
    })
}

/// The network as last seen. The first call starts the watcher.
pub fn latest() -> NetworkState {
    // Polls in tests must not depend on the machine's network.
    if cfg!(any(test, feature = "test-support")) {
        return NetworkState {
            epoch: 0,
            available: true,
        };
    }
    lock_or_recover(tracker()).state(route_now)
}

pub fn system_woke() {
    let route = route_now();
    lock_or_recover(tracker()).woke(route);
    crate::wake::UPDATES.poke();
}

/// A backend's "look again". Runs on the watcher's thread.
fn network_changed() {
    let route = route_now();
    if lock_or_recover(tracker()).observe(route) {
        crate::wake::UPDATES.poke();
    }
}

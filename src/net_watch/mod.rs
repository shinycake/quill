//! What Quill tells TDLib about the network.
//!
//! TDLib holds on to its connections until it notices silence, which takes
//! over two minutes for a client that is not `online`. `setNetworkType` is
//! what makes it drop them (`StateManager::on_network`), and
//! `networkTypeNone` is what turns "Connecting" into "Waiting for network…".
//! tdesktop gets the same from `base::NetworkReachability`, restarting
//! MTProto on `availableChanges` (`mtp_instance.cpp`).
#![deny(clippy::indexing_slicing)]

#[cfg(any(windows, test))]
mod coalesce;
mod link;
#[cfg(any(target_os = "macos", test))]
mod llinfo;
#[cfg(target_os = "linux")]
mod netlink;
mod platform;
#[cfg(test)]
mod tests;

use crate::locks::lock_or_recover;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, UdpSocket};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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

impl Hop {
    /// Whether this hop is `then` with the gateway's hardware address not
    /// resolved yet, so `then`'s address stands for it.
    fn carries(self, then: Self) -> bool {
        match (self.link.gateway, then.link.gateway) {
            (link::Gateway::Unresolved(ip), link::Gateway::Resolved(then_ip, _)) => {
                ip == then_ip
                    && Hop {
                        link: link::Link {
                            gateway: self.link.gateway,
                            ..then.link
                        },
                        ..then
                    } == self
            }
            _ => false,
        }
    }
}

impl Path {
    /// `self`, keeping the gateway's link-layer address from `before` when
    /// the system has not resolved it yet: an entry the cache dropped, or
    /// one not learned yet, is no new network. A different address is.
    fn settled(self, before: Self) -> Self {
        match (self, before) {
            (Self::Via(mut now), Self::Via(then)) if now.carries(then) => {
                now.link.gateway = then.link.gateway;
                Self::Via(now)
            }
            _ => self,
        }
    }

    /// The gateway whose link-layer address [`Self::settled`] carries over
    /// from `before`: until the system resolves it, a new router on the
    /// same address is not seen.
    fn carried(self, before: Self) -> Option<IpAddr> {
        match (self, before) {
            (Self::Via(now), Self::Via(then)) if now.carries(then) => now.link.gateway.ip(),
            _ => None,
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

    /// The interface that reaches `gateway`; 0 when none does.
    fn interface_to(self, gateway: IpAddr) -> u32 {
        [self.v4, self.v6]
            .into_iter()
            .find_map(|path| match path {
                Path::Via(hop) if hop.link.gateway.ip() == Some(gateway) => {
                    Some(hop.link.interface)
                }
                _ => None,
            })
            .unwrap_or(0)
    }

    #[cfg(target_os = "linux")]
    fn gateways(self) -> impl Iterator<Item = IpAddr> {
        [self.v4, self.v6]
            .into_iter()
            .filter_map(|path| match path {
                Path::Via(hop) => hop.link.gateway.ip(),
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

/// How long a gateway that never answered stays asked for: the next network
/// event after this asks again.
const RESOLVE_AGAIN_AFTER: Duration = Duration::from_secs(60);

/// Where one IP version's gateway stands in having its link-layer address
/// resolved. It leaves `Asking` when the route no longer carries the old
/// address over (the system resolved it, or the gateway changed), or
/// [`RESOLVE_AGAIN_AFTER`] after the ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Neighbour {
    /// Nothing is awaited.
    Unasked,
    /// This gateway was asked for at `since`; not asked again until it
    /// changes, or the wait runs out.
    Asking { gateway: IpAddr, since: Instant },
}

impl Neighbour {
    /// Whether `gateway` is still awaited at `now`.
    fn awaits(self, gateway: IpAddr, now: Instant) -> bool {
        matches!(self, Self::Asking { gateway: asked, since }
            if asked == gateway && now.saturating_duration_since(since) < RESOLVE_AGAIN_AFTER)
    }
}

#[derive(Debug)]
struct Tracker {
    route: Route,
    epoch: u64,
    /// Per IP version (v4, v6): the gateway [`Self::gateways_to_resolve`]
    /// last named, so each is asked once until it resolves: a router that
    /// never answers would otherwise turn every failed lookup's event into
    /// another lookup.
    neighbours: [Neighbour; 2],
    /// When a read last looked at the routing table because the route read
    /// offline.
    looked_at: Option<Instant>,
}

/// While offline a read looks again, but at most this often: each look is
/// socket calls (and a netlink or configd round trip), and every account's
/// poll reads the state.
const OFFLINE_LOOK_EVERY: Duration = Duration::from_secs(1);

impl Tracker {
    fn new(route: Route) -> Self {
        Self {
            route,
            epoch: 0,
            neighbours: [Neighbour::Unasked; 2],
            looked_at: None,
        }
    }

    /// The gateways to resolve now, before `route` is observed: those whose
    /// link-layer address would be carried over, each named once.
    fn gateways_to_resolve(&mut self, route: Route, now: Instant) -> Vec<IpAddr> {
        let carried = [
            route.v4.carried(self.route.v4),
            route.v6.carried(self.route.v6),
        ];
        let fresh = carried
            .iter()
            .zip(self.neighbours)
            .filter_map(|(carried, asked)| carried.filter(|gateway| !asked.awaits(*gateway, now)))
            .collect();
        let before = self.neighbours;
        self.neighbours = [0, 1].map(|version| {
            let (carried, asked) = (carried.get(version).copied().flatten(), before.get(version));
            match (carried, asked) {
                (Some(gateway), Some(asked)) if asked.awaits(gateway, now) => *asked,
                (Some(gateway), _) => Neighbour::Asking {
                    gateway,
                    since: now,
                },
                (None, _) => Neighbour::Unasked,
            }
        });
        fresh
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
    /// one event arriving: while offline a read looks again, once per
    /// [`OFFLINE_LOOK_EVERY`]. Whether this read should, marking it so.
    fn look_due(&mut self, now: Instant) -> bool {
        if self.route.available() {
            return false;
        }
        let due = self
            .looked_at
            .is_none_or(|at| now.saturating_duration_since(at) >= OFFLINE_LOOK_EVERY);
        if due {
            self.looked_at = Some(now);
        }
        due
    }

    /// The state, after the look [`Self::look_due`] asked for (`probed`).
    fn state(&mut self, probed: Option<Route>) -> NetworkState {
        if let Some(route) = probed
            && !self.route.available()
        {
            self.observe(route);
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
    let due = lock_or_recover(tracker()).look_due(Instant::now());
    // The look is socket calls: never under the mutex the watcher needs.
    let probed = due.then(route_now);
    lock_or_recover(tracker()).state(probed)
}

pub fn system_woke() {
    let route = route_now();
    lock_or_recover(tracker()).woke(route);
    // Pokes the poll loops too: the clients that follow the OS proxy read
    // it again at their next tick.
    crate::system_proxy::network_changed();
}

/// Whether `ip` is a gateway Telegram traffic leaves through.
#[cfg(target_os = "linux")]
fn is_gateway(ip: IpAddr) -> bool {
    lock_or_recover(tracker())
        .route
        .gateways()
        .any(|gateway| gateway == ip)
}

/// Whether `gateway`'s link-layer address is still awaited.
#[cfg(target_os = "macos")]
fn still_resolving(gateway: IpAddr) -> bool {
    lock_or_recover(tracker())
        .neighbours
        .iter()
        .any(|asked| asked.awaits(gateway, Instant::now()))
}

fn network_changed() {
    let route = route_now();
    let mut tracker = lock_or_recover(tracker());
    let resolve = tracker.gateways_to_resolve(route, Instant::now());
    let moved = tracker.observe(route);
    drop(tracker);
    // The answer comes back as a neighbour change (Linux), from the
    // background resolve `link::of` starts (Windows), or from looks at the
    // NDP table (macOS IPv6). configd reports it on macOS IPv4.
    for gateway in resolve {
        link::resolve(gateway, route.interface_to(gateway));
    }
    if moved {
        crate::wake::UPDATES.poke();
    }
    if moved || platform::EVENTS_COVER_PROXY {
        crate::system_proxy::network_changed();
    }
}

use super::link::{Gateway, Link};
use super::{
    Hop, NO_SUCH_IP_VERSION, OFFLINE_LOOK_EVERY, Path, RESOLVE_AGAIN_AFTER, Route, Tracker,
};
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

const HOME: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 10));
const ROUTER: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1));
const WIFI: Hop = Hop {
    local: HOME,
    link: Link {
        interface: 2,
        gateway: Gateway::Resolved(ROUTER, HOME_ROUTER),
    },
};
const HOME_ROUTER: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const CAFE: Hop = Hop {
    local: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7)),
    link: Link {
        interface: 2,
        gateway: Gateway::Unresolved(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
    },
};

fn path(lookup: io::Result<SocketAddr>) -> Path {
    Path::of(lookup, |_| WIFI.link)
}

fn on(v4: Path, v6: Path) -> Route {
    Route { v4, v6 }
}

fn failed(kind: io::ErrorKind) -> io::Result<SocketAddr> {
    Err(kind.into())
}

#[test]
fn only_a_routing_answer_counts_as_no_route() {
    assert_eq!(path(Ok(SocketAddr::new(HOME, 5000))), Path::Via(WIFI));
    let unspecified = Ipv4Addr::UNSPECIFIED.into();
    assert_eq!(path(Ok(SocketAddr::new(unspecified, 5000))), Path::NoRoute);
    for kind in [
        io::ErrorKind::NetworkUnreachable,
        io::ErrorKind::HostUnreachable,
        io::ErrorKind::AddrNotAvailable,
    ] {
        assert_eq!(path(failed(kind)), Path::NoRoute);
    }
    assert_eq!(path(failed(io::ErrorKind::PermissionDenied)), Path::Unknown);
}

#[test]
fn offline_takes_proof() {
    assert!(on(Path::Via(WIFI), Path::NoRoute).available());
    assert!(on(Path::NoRoute, Path::Via(WIFI)).available());
    assert!(on(Path::Unknown, Path::Unknown).available());
    assert!(!on(Path::NoRoute, Path::NoRoute).available());
    assert!(
        on(Path::Unknown, Path::NoRoute).available(),
        "a firewall refusing UDP on a network with no IPv6"
    );
    assert!(on(Path::NoRoute, Path::Unknown).available());
}

#[test]
fn a_system_with_no_ipv6_has_no_route_over_it() {
    let v6 = path(Err(io::Error::from_raw_os_error(NO_SUCH_IP_VERSION)));
    assert!(!on(Path::NoRoute, v6).available());
}

#[test]
fn only_a_new_route_moves_the_epoch() {
    let wifi = on(Path::Via(WIFI), Path::NoRoute);
    let mut tracker = Tracker::new(wifi);
    assert!(!tracker.observe(wifi), "an event about something else");
    assert_eq!(tracker.epoch, 0);

    assert!(tracker.observe(on(Path::Via(CAFE), Path::NoRoute)));
    assert_eq!(tracker.epoch, 1);

    assert!(tracker.observe(on(Path::NoRoute, Path::NoRoute)));
    assert_eq!(tracker.epoch, 2);
}

#[test]
fn a_new_network_that_gives_the_same_address_moves_the_epoch() {
    let mut tracker = Tracker::new(on(Path::Via(WIFI), Path::NoRoute));
    let other_router = Hop {
        link: Link {
            gateway: Gateway::Unresolved(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 254))),
            ..WIFI.link
        },
        ..WIFI
    };
    assert!(tracker.observe(on(Path::Via(other_router), Path::NoRoute)));
    let ethernet = Hop {
        link: Link {
            interface: 3,
            ..other_router.link
        },
        ..other_router
    };
    assert!(tracker.observe(on(Path::Via(ethernet), Path::NoRoute)));
    assert_eq!(tracker.epoch, 2);
}

#[test]
fn waking_moves_the_epoch_on_the_same_route() {
    let wifi = on(Path::Via(WIFI), Path::NoRoute);
    let mut tracker = Tracker::new(wifi);
    tracker.woke(wifi);
    assert_eq!(tracker.epoch, 1);
}

#[test]
fn an_online_read_does_not_look_and_an_offline_read_does() {
    let wifi = on(Path::Via(WIFI), Path::NoRoute);
    let offline = on(Path::NoRoute, Path::NoRoute);
    let mut tracker = Tracker::new(wifi);
    let start = std::time::Instant::now();
    assert!(!tracker.look_due(start), "the route is known");
    let state = tracker.state(None);
    assert_eq!((state.epoch, state.available), (0, true));

    tracker.observe(offline);
    assert!(tracker.look_due(start));
    let state = tracker.state(Some(offline));
    assert_eq!((state.epoch, state.available), (1, false));

    assert!(tracker.look_due(start + OFFLINE_LOOK_EVERY));
    let state = tracker.state(Some(wifi));
    assert_eq!((state.epoch, state.available), (2, true));
}

/// Sweep 04-001: reads while offline do not each probe the routing table.
#[test]
fn offline_reads_look_at_most_once_per_interval() {
    let mut tracker = Tracker::new(on(Path::NoRoute, Path::NoRoute));
    let start = std::time::Instant::now();
    assert!(tracker.look_due(start));
    for millis in [0, 10, 500, 999] {
        assert!(
            !tracker.look_due(start + std::time::Duration::from_millis(millis)),
            "{millis} ms after a look"
        );
    }
    assert!(tracker.look_due(start + OFFLINE_LOOK_EVERY));
    assert!(!tracker.look_due(start + OFFLINE_LOOK_EVERY));
}

#[test]
fn a_router_on_the_same_address_with_another_link_address_moves_the_epoch() {
    let mut tracker = Tracker::new(on(Path::Via(WIFI), Path::NoRoute));
    let with = |gateway_hardware| Hop {
        link: Link {
            gateway: Gateway::new(Some(ROUTER), gateway_hardware),
            ..WIFI.link
        },
        ..WIFI
    };
    assert!(
        !tracker.observe(on(Path::Via(with(None)), Path::NoRoute)),
        "the new router is not resolved yet"
    );
    let next_door = Some([0x02, 0, 0, 0, 0, 0x02]);
    assert!(tracker.observe(on(Path::Via(with(next_door)), Path::NoRoute)));
    assert!(!tracker.observe(on(Path::Via(with(next_door)), Path::NoRoute)));
    assert!(
        !tracker.observe(on(Path::Via(with(None)), Path::NoRoute)),
        "the cache dropped the entry"
    );
    assert!(tracker.observe(on(Path::Via(WIFI), Path::NoRoute)));
    assert_eq!(tracker.epoch, 2);
}

#[test]
fn an_unresolved_router_on_another_address_is_still_a_new_network() {
    let mut tracker = Tracker::new(on(Path::Via(WIFI), Path::NoRoute));
    assert!(tracker.observe(on(Path::Via(CAFE), Path::NoRoute)));
    assert_eq!(tracker.route.v4, Path::Via(CAFE), "nothing carried over");
}

#[test]
fn a_new_router_on_the_same_address_is_looked_up_at_once_and_only_once() {
    let mut tracker = Tracker::new(on(Path::Via(WIFI), Path::NoRoute));
    let unresolved = on(
        Path::Via(Hop {
            link: Link {
                gateway: Gateway::Unresolved(ROUTER),
                ..WIFI.link
            },
            ..WIFI
        }),
        Path::NoRoute,
    );
    let now = Instant::now();
    assert_eq!(tracker.gateways_to_resolve(unresolved, now), vec![ROUTER]);
    assert!(!tracker.observe(unresolved));
    assert_eq!(
        tracker.gateways_to_resolve(unresolved, now),
        Vec::<IpAddr>::new(),
        "the failed lookup's own event asks no more"
    );
    tracker.observe(unresolved);
    assert_eq!(
        tracker.gateways_to_resolve(on(Path::Via(WIFI), Path::NoRoute), now),
        Vec::<IpAddr>::new()
    );
    tracker.observe(on(Path::Via(WIFI), Path::NoRoute));
    assert_eq!(
        tracker.gateways_to_resolve(unresolved, now),
        vec![ROUTER],
        "a later loss asks again"
    );
    let cafe = on(Path::Via(CAFE), Path::NoRoute);
    assert_eq!(
        tracker.gateways_to_resolve(cafe, now),
        Vec::<IpAddr>::new(),
        "already a new network"
    );
}

#[test]
fn a_router_that_never_answers_is_asked_again_after_the_wait() {
    let mut tracker = Tracker::new(on(Path::Via(WIFI), Path::NoRoute));
    let unresolved = on(
        Path::Via(Hop {
            link: Link {
                gateway: Gateway::Unresolved(ROUTER),
                ..WIFI.link
            },
            ..WIFI
        }),
        Path::NoRoute,
    );
    let start = Instant::now();
    assert_eq!(tracker.gateways_to_resolve(unresolved, start), vec![ROUTER]);
    assert!(
        tracker
            .gateways_to_resolve(unresolved, start + RESOLVE_AGAIN_AFTER / 2)
            .is_empty(),
        "still waiting"
    );
    assert_eq!(
        tracker.gateways_to_resolve(unresolved, start + RESOLVE_AGAIN_AFTER),
        vec![ROUTER],
        "the wait ran out"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_lost_listener_is_started_again_but_not_in_a_rush() {
    use super::{WATCH_RESTART_AFTER, Watcher};
    use std::cell::Cell;
    let starts = Cell::new(0);
    let spawn = || {
        starts.set(starts.get() + 1);
        Ok(std::thread::spawn(|| Some(())))
    };
    let start = Instant::now();
    let mut watcher = Watcher::Stopped;
    watcher.ensure_with(start, |_| false, spawn);
    assert_eq!(starts.get(), 1, "the first call starts it");
    watcher.ensure_with(start + WATCH_RESTART_AFTER, |_| false, spawn);
    assert_eq!(starts.get(), 1, "a listener that runs is left alone");
    watcher.ensure_with(start + WATCH_RESTART_AFTER / 2, |_| true, spawn);
    assert_eq!(starts.get(), 1, "a lost one is not started in a rush");
    watcher.ensure_with(start + WATCH_RESTART_AFTER, |_| true, spawn);
    assert_eq!(starts.get(), 2, "a lost one is replaced");
    let mut refused = Watcher::Stopped;
    refused.ensure_with(start, |_| true, || Err(io::Error::other("no thread")));
    refused.ensure_with(start, |_| true, spawn);
    assert_eq!(starts.get(), 2, "a refused start waits, not every call");
    refused.ensure_with(start + WATCH_RESTART_AFTER, |_| true, spawn);
    assert_eq!(starts.get(), 3, "a refused start is tried again later");
}

/// Sweep r4-04 fix 3: a listener that keeps ending at once (a sandbox refuses
/// the socket) is started on a growing wait, up to a cap; one that ran a
/// while starts the wait over.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_listener_that_cannot_start_is_retried_on_a_growing_wait() {
    use super::{WATCH_RESTART_AFTER, WATCH_RESTART_MAX, WATCH_STABLE_FOR, Watcher};
    use std::cell::Cell;
    let starts = Cell::new(0);
    let spawn = || {
        starts.set(starts.get() + 1);
        Ok(std::thread::spawn(|| Some(())))
    };
    let mut now = Instant::now();
    let mut watcher = Watcher::Stopped;
    watcher.ensure_with(now, |_| true, spawn);
    let mut wait = WATCH_RESTART_AFTER;
    for round in 2..=12 {
        let wait_now = wait.min(WATCH_RESTART_MAX);
        watcher.ensure_with(now + wait_now / 2, |_| true, spawn);
        assert_eq!(starts.get(), round - 1, "too soon, round {round}");
        now += wait_now;
        watcher.ensure_with(now, |_| true, spawn);
        assert_eq!(starts.get(), round, "round {round}");
        wait = wait.saturating_mul(2);
    }
    assert_eq!(wait.min(WATCH_RESTART_MAX), WATCH_RESTART_MAX);
    // A listener that ran a while and then ended starts again at once.
    now += WATCH_RESTART_MAX + WATCH_STABLE_FOR;
    watcher.ensure_with(now, |_| true, spawn);
    assert_eq!(starts.get(), 13);
    watcher.ensure_with(now + WATCH_RESTART_AFTER, |_| true, spawn);
    assert_eq!(starts.get(), 14, "back to the first wait");
}

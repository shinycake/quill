use super::{Path, Route, Tracker};
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const WIFI: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 10));
const CAFE: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));

fn on(v4: Path, v6: Path) -> Route {
    Route { v4, v6 }
}

fn failed(kind: io::ErrorKind) -> io::Result<SocketAddr> {
    Err(kind.into())
}

#[test]
fn only_a_routing_answer_counts_as_no_route() {
    assert_eq!(Path::of(Ok(SocketAddr::new(WIFI, 5000))), Path::Via(WIFI));
    let unspecified = Ipv4Addr::UNSPECIFIED.into();
    assert_eq!(
        Path::of(Ok(SocketAddr::new(unspecified, 5000))),
        Path::NoRoute
    );
    for kind in [
        io::ErrorKind::NetworkUnreachable,
        io::ErrorKind::HostUnreachable,
        io::ErrorKind::AddrNotAvailable,
    ] {
        assert_eq!(Path::of(failed(kind)), Path::NoRoute);
    }
    assert_eq!(
        Path::of(failed(io::ErrorKind::PermissionDenied)),
        Path::Unknown
    );
}

#[test]
fn offline_takes_proof() {
    assert!(on(Path::Via(WIFI), Path::NoRoute).available());
    assert!(on(Path::NoRoute, Path::Via(WIFI)).available());
    assert!(on(Path::Unknown, Path::Unknown).available());
    assert!(!on(Path::NoRoute, Path::NoRoute).available());
    assert!(
        !on(Path::NoRoute, Path::Unknown).available(),
        "a system with no IPv6 at all"
    );
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
    let looked = std::cell::Cell::new(false);
    let state = tracker.state(|| {
        looked.set(true);
        wifi
    });
    assert!(!looked.get(), "the route is known");
    assert_eq!((state.epoch, state.available), (0, true));

    tracker.observe(offline);
    let state = tracker.state(|| offline);
    assert_eq!((state.epoch, state.available), (1, false));

    let state = tracker.state(|| wifi);
    assert_eq!((state.epoch, state.available), (2, true));
}

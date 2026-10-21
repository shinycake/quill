use super::*;

fn request(id: i32, proxy_id: i32) -> CarrierRequest {
    CarrierRequest {
        id,
        proxy_id,
        url: CarrierUrl::new("https://relay.example.com/?bridge=secret#android=n".into()),
        origin: "https://relay.example.com".into(),
        path_prefix: "/".into(),
        bridge_object_name: "TelegramWebProxy".into(),
        temporary: proxy_id == 0,
    }
}

fn hosting() -> Carriers {
    Carriers {
        hosting: true,
        ..Carriers::default()
    }
}

#[test]
fn without_a_host_every_carrier_is_refused() {
    let mut carriers = Carriers::default();
    assert_eq!(
        carriers.opened(request(1, 3)),
        OpenVerdict::Refused { proxy_id: 3 }
    );
    assert_eq!(
        carriers.work,
        vec![CarrierWork::Report {
            carrier_id: 1,
            error: CarrierError::Unsupported
        }]
    );
    assert!(carriers.open.is_empty());
}

#[test]
fn temporary_carriers_are_refused_with_or_without_a_host() {
    let mut carriers = hosting();
    assert_eq!(carriers.opened(request(2, 0)), OpenVerdict::Temporary);
    assert_eq!(
        carriers.work,
        vec![CarrierWork::Report {
            carrier_id: 2,
            error: CarrierError::Refused
        }]
    );
}

#[test]
fn a_hosted_carrier_opens_and_closes_telling_the_page() {
    let mut carriers = hosting();
    assert_eq!(
        carriers.opened(request(5, 1)),
        OpenVerdict::Hosting { proxy_id: 1 }
    );
    assert!(matches!(carriers.work.as_slice(), [CarrierWork::Open(r)] if r.id == 5));
    carriers.work.clear();
    assert!(carriers.accepts_data(5));
    assert!(!carriers.accepts_data(6));
    assert_eq!(
        carriers.open.get(&5).map(|c| c.phase),
        Some(CarrierPhase::Live)
    );
    // Whether the page posted is the helper's to know (it may have, in a
    // message still on its way), so TDLib's close always tells the page.
    carriers.closed(5);
    assert_eq!(
        carriers.work,
        vec![CarrierWork::Close {
            carrier_id: 5,
            notify_page: true
        }]
    );
    assert!(!carriers.accepts_data(5));
    // A repeat close does nothing.
    carriers.work.clear();
    carriers.closed(5);
    assert!(carriers.work.is_empty());
}

#[test]
fn a_carrier_fails_once() {
    let mut carriers = hosting();
    carriers.opened(request(5, 1));
    carriers.work.clear();
    assert_eq!(carriers.failed(5, CarrierError::Terminated), Some(1));
    assert_eq!(
        carriers.work,
        vec![
            CarrierWork::Report {
                carrier_id: 5,
                error: CarrierError::Terminated
            },
            CarrierWork::Close {
                carrier_id: 5,
                notify_page: false
            },
        ]
    );
    carriers.work.clear();
    assert_eq!(carriers.failed(5, CarrierError::Terminated), None);
    assert!(!carriers.accepts_data(5));
    // TDLib's close after the failure: the view is already gone.
    carriers.closed(5);
    assert!(carriers.work.is_empty());
    assert_eq!(carriers.failed(9, CarrierError::Terminated), None);
}

#[test]
fn a_repeated_open_starts_nothing_new() {
    let mut carriers = hosting();
    carriers.opened(request(5, 1));
    carriers.work.clear();
    assert_eq!(
        carriers.opened(request(5, 1)),
        OpenVerdict::Hosting { proxy_id: 1 }
    );
    assert!(carriers.work.is_empty());
}

#[test]
fn errors_have_tdlib_shapes() {
    assert_eq!(
        CarrierError::Unsupported.to_json()["@type"],
        "webProxyCarrierErrorUnsupported"
    );
    assert_eq!(
        CarrierError::Refused.to_json()["@type"],
        "webProxyCarrierErrorRefused"
    );
    assert_eq!(
        CarrierError::Terminated.to_json()["@type"],
        "webProxyCarrierErrorTerminated"
    );
    let load = CarrierError::LoadFailed("tls".into()).to_json();
    assert_eq!(load["@type"], "webProxyCarrierErrorLoadFailed");
    assert_eq!(load["message"], "tls");
}

#[test]
fn the_url_is_hidden_from_debug_output() {
    let shown = format!("{:?}", request(1, 1));
    assert!(!shown.contains("secret"));
    assert!(shown.contains("<hidden>"));
    let work = format!("{:?}", CarrierWork::Open(request(1, 1)));
    assert!(!work.contains("secret"), "{work}");
}

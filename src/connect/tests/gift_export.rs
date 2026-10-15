//! Connect-driver tests: the transfer picker's two requests, `transferGift`
//! to a picked contact and `getUpgradedGiftWithdrawalUrl` from the password
//! box (recorded JSON, nothing live; nothing is transferred or exported).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::gift_manage::{ExportError, GiftFlow, GiftTransferred, TransferPicker, TransferStage};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;
type Parts = (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

const PASSWORD: &str = "CANARY-pw-4417";

fn parts() -> Parts {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.my_user_id = Some(7);
    (driver, recorder, sink, seq)
}

fn feed(p: &mut Parts, json: Value) {
    let (driver, _, sink, seq) = p;
    driver
        .ingest(copy_and_parse(&json.to_string(), seq, sink).unwrap())
        .unwrap();
}

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| serde_json::from_str::<Value>(json).is_ok_and(|v| v["@type"] == type_name))
        .count()
}

fn user(p: &mut Parts, id: i64, first: &str, kind: Value) {
    feed(
        p,
        json!({"@type": "updateUser", "user": {"@type": "user", "id": id, "first_name": first,
            "last_name": "", "usernames": null, "phone_number": "",
            "status": {"@type": "userStatusRecently"}, "is_contact": true, "type": kind}}),
    );
}

fn gift(id: &str, export_date: i64) -> Value {
    json!({"@type": "receivedGift", "received_gift_id": id, "date": 5, "is_saved": true,
        "can_be_transferred": true, "transfer_star_count": 0, "export_date": export_date,
        "gift": {"@type": "sentGiftUpgraded", "gift": {
            "@type": "upgradedGift", "title": "Plush Pepe", "name": "PlushPepe-4",
            "number": 4, "owner_id": {"@type": "messageSenderUser", "user_id": 7}}}})
}

/// My gifts list with an unlocked gift `open` and a locked one `later`.
fn open_my_gifts(p: &mut Parts) {
    let extra =
        p.0.open_received_gifts(MessageSender::User { user_id: 7 })
            .unwrap();
    let now = crate::local_time::now_unix();
    feed(
        p,
        json!({"@type": "receivedGifts", "@extra": extra.as_extra(), "total_count": 2,
            "next_offset": "", "gifts": [gift("open", now - 60), gift("later", now + 86_400)]}),
    );
    user(p, 21, "Maya", json!({"@type": "userTypeRegular"}));
    user(
        p,
        22,
        "Helper",
        json!({"@type": "userTypeBot", "can_join_groups": true}),
    );
}

fn set_stage(p: &mut Parts, id: &str, stage: TransferStage) {
    let mut picker = TransferPicker::new(id, "Plush Pepe #4", "PlushPepe-4");
    picker.stage = stage;
    p.0.session.payments.gifts_manage.flow = Some(GiftFlow::Transfer(picker));
}

fn password_stage() -> TransferStage {
    TransferStage::Password {
        sending: false,
        error: None,
    }
}

fn stage(p: &Parts) -> Option<TransferStage> {
    p.0.session
        .payments
        .gifts_manage
        .transfer()
        .map(|picker| picker.stage.clone())
}

#[test]
fn a_transfer_goes_only_to_the_contact_the_confirmation_names() {
    let mut p = parts();
    open_my_gifts(&mut p);
    // No confirmation on screen: nothing is sent.
    set_stage(&mut p, "open", TransferStage::Pick);
    assert!(p.0.transfer_picked_gift().is_err());
    // Not to a bot, not to myself.
    for (user_id, name) in [(22, "Helper"), (7, "Me")] {
        set_stage(
            &mut p,
            "open",
            TransferStage::ConfirmPeer {
                user_id,
                name: name.into(),
            },
        );
        assert!(p.0.transfer_picked_gift().is_err());
    }
    assert_eq!(count(&p.1, "transferGift"), 0);
    set_stage(
        &mut p,
        "open",
        TransferStage::ConfirmPeer {
            user_id: 21,
            name: "Maya".into(),
        },
    );
    let extra = p.0.transfer_picked_gift().unwrap();
    let sent = sent_request(&p.1, "transferGift");
    assert_eq!(sent["received_gift_id"], "open");
    assert_eq!(sent["new_owner_id"]["@type"], "messageSenderUser");
    assert_eq!(sent["new_owner_id"]["user_id"], 21);
    assert_eq!(sent["star_count"], 0);
    feed(&mut p, json!({"@type": "ok", "@extra": extra.as_extra()}));
    let manage = &p.0.session.payments.gifts_manage;
    assert!(manage.flow.is_none());
    assert_eq!(
        manage.transferred,
        Some(GiftTransferred {
            gift_name: "Plush Pepe #4".into(),
            recipient: "Maya".into(),
            user_id: 21,
        })
    );
}

#[test]
fn the_export_url_is_asked_only_from_the_password_box() {
    let mut p = parts();
    open_my_gifts(&mut p);
    for early in [
        TransferStage::Pick,
        TransferStage::ConfirmExport,
        TransferStage::Locked { hours: 3 },
    ] {
        set_stage(&mut p, "open", early);
        assert!(p.0.export_picked_gift(PASSWORD).is_err());
    }
    // Not with an empty password, not for a gift still locked.
    set_stage(&mut p, "open", password_stage());
    assert!(p.0.export_picked_gift("").is_err());
    set_stage(&mut p, "later", password_stage());
    assert!(p.0.export_picked_gift(PASSWORD).is_err());
    assert_eq!(count(&p.1, "getUpgradedGiftWithdrawalUrl"), 0);

    set_stage(&mut p, "open", password_stage());
    let extra = p.0.export_picked_gift(PASSWORD).unwrap();
    let sent = sent_request(&p.1, "getUpgradedGiftWithdrawalUrl");
    assert_eq!(sent["received_gift_id"], "open");
    assert_eq!(sent["password"], PASSWORD);
    assert_eq!(
        stage(&p),
        Some(TransferStage::Password {
            sending: true,
            error: None
        })
    );
    // A second Submit while the first is out sends nothing.
    assert!(p.0.export_picked_gift(PASSWORD).is_err());
    assert_eq!(count(&p.1, "getUpgradedGiftWithdrawalUrl"), 1);
    // The password lives nowhere in the session.
    let state = format!("{:?}", p.0.session.payments.gifts_manage);
    assert!(!state.contains(PASSWORD));
    feed(
        &mut p,
        json!({"@type": "httpUrl", "@extra": extra.as_extra(),
            "url": "https://fragment.com/gift/PlushPepe-4?export=1"}),
    );
    let manage = &p.0.session.payments.gifts_manage;
    assert!(manage.flow.is_none());
    assert_eq!(
        manage.export_url.as_deref(),
        Some("https://fragment.com/gift/PlushPepe-4?export=1")
    );
}

fn fail_export(message: &str, code: i32) -> Option<TransferStage> {
    let mut p = parts();
    open_my_gifts(&mut p);
    set_stage(&mut p, "open", password_stage());
    let extra = p.0.export_picked_gift(PASSWORD).unwrap();
    feed(
        &mut p,
        json!({"@type": "error", "@extra": extra.as_extra(), "code": code, "message": message}),
    );
    assert!(p.0.session.payments.gifts_manage.export_url.is_none());
    let state = format!("{:?}", p.0.session.payments.gifts_manage);
    assert!(!state.contains(PASSWORD));
    stage(&p)
}

#[test]
fn password_errors_land_in_the_password_box() {
    assert_eq!(
        fail_export("PASSWORD_HASH_INVALID", 400),
        Some(TransferStage::Password {
            sending: false,
            error: Some(ExportError::WrongPassword)
        })
    );
    assert_eq!(
        fail_export("PASSWORD_MISSING", 400),
        Some(TransferStage::SecurityCheck { later: false })
    );
    assert_eq!(
        fail_export("PASSWORD_TOO_FRESH_86400", 400),
        Some(TransferStage::SecurityCheck { later: true })
    );
    assert_eq!(
        fail_export("SESSION_TOO_FRESH_3600", 400),
        Some(TransferStage::SecurityCheck { later: true })
    );
    assert_eq!(
        fail_export("Too Many Requests: retry after 30", 429),
        Some(TransferStage::Password {
            sending: false,
            error: Some(ExportError::Flood)
        })
    );
    // tdesktop `setPasswordFail`: a changed password reads as a wrong
    // one; an expired check as a server error.
    assert_eq!(
        fail_export("SRP_PASSWORD_CHANGED", 400),
        Some(TransferStage::Password {
            sending: false,
            error: Some(ExportError::WrongPassword)
        })
    );
    assert_eq!(
        fail_export("SRP_ID_INVALID", 400),
        Some(TransferStage::Password {
            sending: false,
            error: Some(ExportError::ServerError)
        })
    );
    // Anything else: the box stays, ready for another try, and says why
    // under the field (a note would hide behind the picker).
    let other = fail_export("STARGIFT_EXPORT_IN_PROGRESS", 400);
    assert!(
        matches!(
            &other,
            Some(TransferStage::Password {
                sending: false,
                error: Some(ExportError::Other(reason)),
            }) if !reason.is_empty()
        ),
        "{other:?}"
    );
}

fn password_state(p: &mut Parts, has_password: bool) {
    // The reload sent with the probe.
    let extra = sent_request(&p.1, "getPasswordState")["@extra"].clone();
    feed(
        p,
        json!({"@type": "passwordState", "@extra": extra,
            "has_password": has_password, "password_hint": "", "has_recovery_email_address": false,
            "has_passport_data": false, "recovery_email_address_code_info": null,
            "login_email_address_pattern": "", "pending_reset_date": 0}),
    );
}

/// "Open Fragment" on the unlocked gift: the probe is out.
fn probe(p: &mut Parts) -> crate::ids::RequestId {
    set_stage(p, "open", TransferStage::ConfirmExport);
    p.0.probe_gift_export().unwrap()
}

#[test]
fn open_fragment_sends_the_password_less_probe_only_from_its_confirmation() {
    let mut p = parts();
    open_my_gifts(&mut p);
    for early in [TransferStage::Pick, password_stage()] {
        set_stage(&mut p, "open", early);
        assert!(p.0.probe_gift_export().is_err());
    }
    // Not for a gift still locked.
    set_stage(&mut p, "later", TransferStage::ConfirmExport);
    assert!(p.0.probe_gift_export().is_err());
    assert_eq!(count(&p.1, "getUpgradedGiftWithdrawalUrl"), 0);

    probe(&mut p);
    let sent = sent_request(&p.1, "getUpgradedGiftWithdrawalUrl");
    assert_eq!(sent["received_gift_id"], "open");
    assert_eq!(sent["password"], "");
    assert_eq!(count(&p.1, "getPasswordState"), 1);
    assert_eq!(stage(&p), Some(TransferStage::Probing));
    // A second click while it is out sends nothing.
    assert!(p.0.probe_gift_export().is_err());
    assert_eq!(count(&p.1, "getUpgradedGiftWithdrawalUrl"), 1);
}

#[test]
fn a_url_answer_to_the_probe_is_never_opened() {
    let mut p = parts();
    open_my_gifts(&mut p);
    let extra = probe(&mut p);
    feed(
        &mut p,
        json!({"@type": "httpUrl", "@extra": extra.as_extra(),
            "url": "https://fragment.com/gift/PlushPepe-4?export=1"}),
    );
    let manage = &p.0.session.payments.gifts_manage;
    assert!(manage.export_url.is_none());
    assert_eq!(stage(&p), Some(password_stage()));
}

fn fail_probe(message: &str, has_password: Option<bool>) -> Option<TransferStage> {
    let mut p = parts();
    open_my_gifts(&mut p);
    let extra = probe(&mut p);
    feed(
        &mut p,
        json!({"@type": "error", "@extra": extra.as_extra(), "code": 400, "message": message}),
    );
    if let Some(has_password) = has_password {
        password_state(&mut p, has_password);
    }
    assert!(p.0.session.payments.gifts_manage.export_url.is_none());
    stage(&p)
}

#[test]
fn the_probe_picks_the_security_check_or_the_password_box() {
    // tdesktop `PrePasswordErrorBox`, as the server would answer.
    assert_eq!(
        fail_probe("PASSWORD_MISSING", None),
        Some(TransferStage::SecurityCheck { later: false })
    );
    assert_eq!(
        fail_probe("PASSWORD_TOO_FRESH_86400", None),
        Some(TransferStage::SecurityCheck { later: true })
    );
    // TDLib answers the empty password itself with PASSWORD_HASH_INVALID:
    // the password box, unless the reloaded state says there is none.
    assert_eq!(
        fail_probe("PASSWORD_HASH_INVALID", None),
        Some(password_stage())
    );
    assert_eq!(
        fail_probe("PASSWORD_HASH_INVALID", Some(true)),
        Some(password_stage())
    );
    assert_eq!(
        fail_probe("PASSWORD_HASH_INVALID", Some(false)),
        Some(TransferStage::SecurityCheck { later: false })
    );
}

#[test]
fn a_second_transfer_click_sends_nothing() {
    let mut p = parts();
    open_my_gifts(&mut p);
    set_stage(
        &mut p,
        "open",
        TransferStage::ConfirmPeer {
            user_id: 21,
            name: "Maya".into(),
        },
    );
    p.0.transfer_picked_gift().unwrap();
    assert!(p.0.transfer_picked_gift().is_err());
    assert_eq!(count(&p.1, "transferGift"), 1);
    // Nor an export while the transfer is out.
    set_stage(&mut p, "open", TransferStage::ConfirmExport);
    assert!(p.0.probe_gift_export().is_err());
    assert_eq!(count(&p.1, "getUpgradedGiftWithdrawalUrl"), 0);
}

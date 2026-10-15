//! The transfer picker's core: the unlock countdown against tdesktop's
//! formula, the stages and the people it lists.
use crate::gift_manage::{
    ExportState, TransferPicker, TransferStage, UnlockIn, fragment_gift_url, transfer_candidates,
    unlock_hours,
};
use crate::premium_hub::parse_received_gift;
use serde_json::{Value, json};

const NOW: i64 = 1_700_000_000;

/// tdesktop `MakeExportOption`: `max((left + 1800) / 3600, 1)`, then
/// `days = hours / 24`, written out the way the C++ does it.
fn tdesktop(when: i64, now: i64) -> (i64, i64) {
    let left = if when > now { when - now } else { 0 };
    let hours = if left > 0 {
        std::cmp::max((left + 1800) / 3600, 1)
    } else {
        0
    };
    (hours, hours / 24)
}

#[test]
fn the_countdown_rounds_like_tdesktop() {
    for left in [
        1, 59, 1_799, 1_800, 1_801, 3_599, 3_600, 5_399, 5_400, 86_399, 84_600, 86_400, 88_199,
        88_200, 172_800, 604_800, 2_592_000,
    ] {
        let (hours, days) = tdesktop(NOW + left, NOW);
        assert_eq!(unlock_hours(left), hours, "left {left}");
        assert_eq!(
            ExportState::of(NOW + left, NOW),
            ExportState::Locked { hours },
            "left {left}"
        );
        let expected = if days > 0 {
            UnlockIn::Days(days)
        } else {
            UnlockIn::Hours(hours)
        };
        assert_eq!(UnlockIn::of(hours), expected, "left {left}");
    }
    // Under half an hour still reads "1 hour"; 23.5 h reads "24 hours"
    // rounded, which tdesktop shows as "1 day".
    assert_eq!(unlock_hours(60), 1);
    assert_eq!(
        UnlockIn::of(unlock_hours(23 * 3_600 + 1_800)),
        UnlockIn::Days(1)
    );
    assert_eq!(
        UnlockIn::of(unlock_hours(23 * 3_600 + 1_799)),
        UnlockIn::Hours(23)
    );
    assert_eq!(
        UnlockIn::of(unlock_hours(47 * 3_600 + 1_799)),
        UnlockIn::Days(1)
    );
}

#[test]
fn export_state_needs_a_date_and_opens_when_it_passes() {
    assert_eq!(ExportState::of(0, NOW), ExportState::Unavailable);
    assert_eq!(ExportState::of(-5, NOW), ExportState::Unavailable);
    assert_eq!(ExportState::of(NOW, NOW), ExportState::Ready);
    assert_eq!(ExportState::of(NOW - 100, NOW), ExportState::Ready);
    assert_eq!(
        ExportState::of(i64::MAX, i64::MIN),
        ExportState::Locked {
            hours: unlock_hours(i64::MAX)
        }
    );
}

fn received(extra: Value) -> crate::premium_hub::ReceivedGift {
    let mut v = json!({
        "@type": "receivedGift", "received_gift_id": "g1", "date": 5, "is_saved": true,
        "can_be_transferred": true, "export_date": NOW + 7_200,
        "gift": {"@type": "sentGiftUpgraded", "gift": {
            "@type": "upgradedGift", "title": "Plush Pepe", "name": "PlushPepe-1204",
            "number": 1204, "owner_id": {"@type": "messageSenderUser", "user_id": 7}}},
    });
    if let (Some(base), Some(extra)) = (v.as_object_mut(), extra.as_object()) {
        for (k, val) in extra {
            base.insert(k.clone(), val.clone());
        }
    }
    parse_received_gift(&v).unwrap_or_else(|| panic!("fixture parses"))
}

#[test]
fn the_picker_walks_its_stages() {
    let gift = received(json!({}));
    assert_eq!(gift.manage.export_date, (NOW + 7_200) as i32);
    let mut picker = TransferPicker::for_gift(&gift);
    assert_eq!(picker.gift_name, "Plush Pepe #1,204");
    assert_eq!(picker.slug, "PlushPepe-1204");
    assert_eq!(picker.stage, TransferStage::Pick);

    // Locked: the explanation, OK goes back.
    picker.tap_export(gift.export_state(NOW));
    assert_eq!(picker.stage, TransferStage::Locked { hours: 2 });
    picker.open_fragment();
    assert_eq!(picker.stage, TransferStage::Locked { hours: 2 });
    picker.back();
    assert_eq!(picker.stage, TransferStage::Pick);

    // Unlocked: confirm, then the password box.
    picker.tap_export(gift.export_state(NOW + 7_200));
    assert_eq!(picker.stage, TransferStage::ConfirmExport);
    picker.choose(5, "Maya");
    assert_eq!(picker.stage, TransferStage::ConfirmExport);
    picker.open_fragment();
    assert_eq!(
        picker.stage,
        TransferStage::Password {
            sending: false,
            error: None
        }
    );
    // While the request is out nothing goes back.
    picker.stage = TransferStage::Password {
        sending: true,
        error: None,
    };
    assert!(picker.exporting());
    picker.back();
    assert!(picker.exporting());
    picker.stage = TransferStage::Pick;

    // A contact: the confirmation names them.
    picker.choose(5, "Maya");
    assert_eq!(
        picker.stage,
        TransferStage::ConfirmPeer {
            user_id: 5,
            name: "Maya".into()
        }
    );
    picker.back();

    // No export date: the row is not there and tapping does nothing.
    picker.tap_export(ExportState::Unavailable);
    assert_eq!(picker.stage, TransferStage::Pick);
}

#[test]
fn a_gift_in_a_ton_wallet_opens_action_locked() {
    let gift = received(json!({
        "can_be_transferred": false,
        "gift": {"@type": "sentGiftUpgraded", "gift": {
            "@type": "upgradedGift", "title": "Plush Pepe", "name": "PlushPepe-9",
            "number": 9, "owner_address": "EQabc"}},
    }));
    assert!(gift.held_outside_telegram());
    let picker = TransferPicker::for_gift(&gift);
    assert_eq!(picker.stage, TransferStage::ActionLocked);
    assert_eq!(
        fragment_gift_url(&picker.slug),
        "https://fragment.com/gift/PlushPepe-9?collection=my"
    );
    assert!(!received(json!({})).held_outside_telegram());
}

#[test]
fn the_list_skips_me_bots_service_and_deleted_accounts() {
    let rows = [
        (7, "Robin Lee", "online", false),
        (5, "Maya Chen", "last seen recently", false),
        (9, "Weather Bot", "bot", true),
        (777_000, "Telegram", "service notifications", false),
        (11, " ", "", false),
        (12, "Ben Ito", "last seen yesterday", false),
    ];
    let all = transfer_candidates(rows, Some(7), "");
    let ids: Vec<i64> = all.iter().map(|c| c.user_id).collect();
    assert_eq!(ids, vec![5, 12]);
    let found = transfer_candidates(rows, Some(7), "ma");
    assert_eq!(found.len(), 1);
    assert_eq!(
        found.first().map(|c| c.status.as_str()),
        Some("last seen recently")
    );
    // Every word must start a part of the name.
    assert!(transfer_candidates(rows, Some(7), "chen m").len() == 1);
    assert!(transfer_candidates(rows, Some(7), "aya").is_empty());
}

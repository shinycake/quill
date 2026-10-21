//! Share theme tests (`parity:profile-share-theme`): parsing both
//! variants, the request JSON, the backdrop refresh rule, who may save,
//! the gifts list and the box's save, serve and revert steps.
use crate::ids::RequestId;
use crate::peer_qr::appearance::{DEFAULT_CENTER, DEFAULT_EDGE, ShareThemeBackdrop, SwatchKey};
use crate::peer_qr::color::Rgb;
use crate::peer_qr::requests::{GiftsOwner, get_share_theme_gifts, set_profile_share_theme};
use crate::peer_qr::state::{
    PeerQrBox, QrAction, QrEffect, QrEnv, QrPanel, QrPeer, QrSubject, QrToast, SaveTicket,
};
use crate::peer_qr::theme::{
    BACKDROPS_REFRESH_SECS, Backdrops, GiftsLayout, SaveRights, ShareTheme, ThemeChoice,
    ThemeEmoji, ThemeGift, ThemeGifts, can_save, gifts_layout, parse_share_theme,
};
use serde_json::{Value, json};

fn plain(emoji: &str) -> ThemeEmoji {
    ThemeEmoji::Plain(emoji.into())
}

fn gift(id: i64, received: &str) -> ThemeGift {
    ThemeGift {
        id,
        received_id: received.into(),
        name: format!("Plush Pepe #{id}"),
        center: Rgb::hex(0x5A_8CF0),
        edge: Rgb::hex(0x2B_4FB5),
        symbol_color: Rgb::hex(0x12_3456),
        model: None,
        symbol: None,
        files: Vec::new(),
    }
}

fn backdrop(id: i32, center: Rgb, edge: Rgb) -> ShareThemeBackdrop {
    ShareThemeBackdrop {
        id,
        name: String::new(),
        center,
        edge,
    }
}

fn env(can_save: bool) -> QrEnv {
    QrEnv {
        now: 1_000,
        price: 0,
        temporary_only: false,
        can_save,
    }
}

fn own_box() -> PeerQrBox {
    PeerQrBox::new(QrSubject::Peer(QrPeer::User(1)), 3)
}

fn saves(effects: &[QrEffect]) -> Vec<ThemeChoice> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            QrEffect::SaveTheme { choice, .. } => Some(choice.clone()),
            _ => None,
        })
        .collect()
}

fn ticket_of(effects: &[QrEffect]) -> SaveTicket {
    effects
        .iter()
        .find_map(|effect| match effect {
            QrEffect::SaveTheme { ticket, .. } => Some(*ticket),
            _ => None,
        })
        .unwrap()
}

#[test]
fn an_emoji_theme_parses_its_backdrop_and_up_to_six_distinct_emoji() {
    let value = json!({"@type": "profileShareThemeEmoji", "emojis": [
            {"@type": "emojiTypeRegular", "emoji": "🐱"},
            {"@type": "emojiTypeRegular", "emoji": "🐱"},
            {"@type": "emojiTypeCustomEmoji", "custom_emoji_id": "42"},
            {"@type": "emojiTypeRegular", "emoji": "🐶"},
            {"@type": "emojiTypeRegular", "emoji": "🦊"},
            {"@type": "emojiTypeRegular", "emoji": "🐼"},
            {"@type": "emojiTypeRegular", "emoji": "🐸"},
            {"@type": "emojiTypeRegular", "emoji": "🐵"}],
        "use_default_background": false, "background_backdrop_id": 12});
    let ShareTheme::Emoji { backdrop, emoji } = parse_share_theme(Some(&value)) else {
        panic!("not an emoji theme");
    };
    assert_eq!(backdrop, Some(12));
    assert_eq!(
        emoji,
        vec![
            plain("🐱"),
            ThemeEmoji::Custom(42),
            plain("🐶"),
            plain("🦊"),
            plain("🐼"),
            plain("🐸")
        ]
    );
}

#[test]
fn the_default_background_ignores_the_backdrop_id() {
    let value = json!({"@type": "profileShareThemeEmoji", "emojis": [
            {"@type": "emojiTypeRegular", "emoji": "🐱"}],
        "use_default_background": true, "background_backdrop_id": 12});
    assert_eq!(
        parse_share_theme(Some(&value)),
        ShareTheme::Emoji {
            backdrop: None,
            emoji: vec![plain("🐱")]
        }
    );
    let empty = json!({"@type": "profileShareThemeEmoji", "emojis": [],
        "use_default_background": true, "background_backdrop_id": 0});
    assert_eq!(parse_share_theme(Some(&empty)), ShareTheme::Default);
    assert_eq!(parse_share_theme(Some(&Value::Null)), ShareTheme::Default);
    assert_eq!(parse_share_theme(None), ShareTheme::Default);
}

#[test]
fn a_gift_theme_parses_the_upgraded_gift() {
    let value = json!({"@type": "profileShareThemeUpgradedGift", "gift": {
        "@type": "upgradedGift", "id": "9001", "title": "Plush Pepe", "name": "PlushPepe-4",
        "number": 4,
        "backdrop": {"@type": "upgradedGiftBackdrop", "id": 1, "name": "Ocean",
            "colors": {"@type": "upgradedGiftBackdropColors", "center_color": 5934320,
                "edge_color": 2838453, "symbol_color": 1193046, "text_color": 16777215}}}});
    let ShareTheme::Gift(gift) = parse_share_theme(Some(&value)) else {
        panic!("not a gift theme");
    };
    assert_eq!(gift.id, 9001);
    assert_eq!(gift.received_id, "", "full info names only the gift");
    assert_eq!(gift.center, Rgb::hex(0x5A_8CF0));
    assert_eq!(gift.edge, Rgb::hex(0x2B_4FB5));
    assert_eq!(gift.symbol_color, Rgb::hex(0x12_3456));
    let regular = json!({"@type": "profileShareThemeUpgradedGift", "gift": {"@type": "gift"}});
    assert_eq!(parse_share_theme(Some(&regular)), ShareTheme::Default);
}

#[test]
fn set_profile_share_theme_sends_each_input_variant() {
    let extra = RequestId(5);
    let request = |choice: &ThemeChoice| -> Value {
        serde_json::from_str(&set_profile_share_theme(extra, -1007, choice).unwrap()).unwrap()
    };
    let backdrop = request(&ThemeChoice::Emoji {
        color: SwatchKey::Backdrop(12),
        emoji: vec![plain("🐱"), ThemeEmoji::Custom(42)],
    });
    assert_eq!(backdrop["@type"], "setProfileShareTheme");
    assert_eq!(backdrop["@extra"], extra.as_extra());
    assert_eq!(backdrop["chat_id"], -1007);
    assert_eq!(
        backdrop["theme"],
        json!({"@type": "inputProfileShareThemeEmoji", "emojis": [
                {"@type": "emojiTypeRegular", "emoji": "🐱"},
                {"@type": "emojiTypeCustomEmoji", "custom_emoji_id": "42"}],
            "use_default_background": false, "background_backdrop_id": 12})
    );
    let default_background = request(&ThemeChoice::Emoji {
        color: SwatchKey::Default,
        emoji: vec![plain("🐱")],
    });
    assert_eq!(default_background["theme"]["use_default_background"], true);
    assert_eq!(request(&ThemeChoice::default())["theme"], Value::Null);
    let upgraded = request(&ThemeChoice::Gift(Box::new(gift(9, "r9"))));
    assert_eq!(
        upgraded["theme"],
        json!({"@type": "inputProfileShareThemeUpgradedGift", "received_gift_id": "r9"})
    );
    let unsendable = ThemeChoice::Gift(Box::new(gift(9, "")));
    assert_eq!(set_profile_share_theme(extra, 1, &unsendable), None);
}

#[test]
fn the_gifts_request_lists_only_owned_upgraded_gifts() {
    let request: Value = serde_json::from_str(&get_share_theme_gifts(
        RequestId(6),
        GiftsOwner::Chat(-1007),
        "next",
    ))
    .unwrap();
    assert_eq!(request["@type"], "getReceivedGifts");
    assert_eq!(
        request["owner_id"],
        json!({"@type": "messageSenderChat", "chat_id": -1007})
    );
    for flag in [
        "exclude_unlimited",
        "exclude_upgradable",
        "exclude_non_upgradable",
        "exclude_hosted",
    ] {
        assert_eq!(request[flag], true, "{flag}");
    }
    assert_eq!(request["exclude_upgraded"], false);
    assert_eq!(request["offset"], "next");
    assert_eq!(request["limit"], 50);
}

#[test]
fn backdrops_refresh_an_hour_after_the_last_request() {
    let mut backdrops = Backdrops::default();
    assert!(backdrops.due(0));
    backdrops.start(100);
    assert!(
        !backdrops.due(100 + BACKDROPS_REFRESH_SECS),
        "one at a time"
    );
    backdrops.accept(vec![backdrop(1, DEFAULT_CENTER, DEFAULT_EDGE)]);
    assert!(!backdrops.due(100 + BACKDROPS_REFRESH_SECS - 1));
    assert!(backdrops.due(100 + BACKDROPS_REFRESH_SECS));
    backdrops.start(5_000);
    assert_eq!(backdrops.list.len(), 1, "kept during a refresh");
    backdrops.fail();
    assert!(backdrops.due(5_001), "a failure asks again at once");
    assert_eq!(backdrops.list.len(), 1);
}

#[test]
fn can_save_per_peer_kind() {
    assert!(can_save(SaveRights::Me));
    assert!(!can_save(SaveRights::OtherUser));
    assert!(!can_save(SaveRights::Other));
    for (has_username, can_change_info, expected) in [
        (true, true, true),
        (true, false, false),
        (false, true, false),
        (false, false, false),
    ] {
        assert_eq!(
            can_save(SaveRights::Chat {
                has_username,
                can_change_info
            }),
            expected
        );
    }
}

#[test]
fn the_gifts_list_dedupes_and_picks_a_layout() {
    let mut gifts = ThemeGifts::default();
    assert_eq!(gifts_layout(&gifts), GiftsLayout::Loading);
    gifts.accept(Vec::new(), String::new(), true);
    assert_eq!(gifts_layout(&gifts), GiftsLayout::Empty);
    gifts.accept(vec![gift(1, "a"), gift(2, "b")], String::new(), true);
    assert_eq!(gifts_layout(&gifts), GiftsLayout::Wide(2));
    gifts.accept(vec![gift(1, "a"), gift(3, "c")], "more".into(), false);
    assert_eq!(gifts.list.len(), 3);
    assert_eq!(gifts_layout(&gifts), GiftsLayout::Grid, "more to come");
    gifts.accept(vec![gift(4, "d")], String::new(), true);
    assert_eq!(gifts.list.len(), 1, "a first page replaces the list");
}

#[test]
fn a_backdrop_with_the_default_colors_reads_as_the_default_circle() {
    let list = [
        backdrop(1, DEFAULT_CENTER, DEFAULT_EDGE),
        backdrop(2, Rgb::hex(0x5A_8CF0), Rgb::hex(0x2B_4FB5)),
    ];
    let theme = |id| ShareTheme::Emoji {
        backdrop: Some(id),
        emoji: vec![plain("🐱")],
    };
    assert_eq!(
        theme(1).choice(&list),
        ThemeChoice::Emoji {
            color: SwatchKey::Default,
            emoji: vec![plain("🐱")]
        }
    );
    assert_eq!(
        theme(2).choice(&list),
        ThemeChoice::Emoji {
            color: SwatchKey::Backdrop(2),
            emoji: vec![plain("🐱")]
        }
    );
}

#[test]
fn picks_preview_and_save_once_the_panel_closes() {
    let mut qr = own_box();
    qr.apply(QrAction::Served(ThemeChoice::default()), env(true));
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    assert!(
        qr.apply(QrAction::PickColor(SwatchKey::Backdrop(4)), env(true))
            .is_empty()
    );
    assert!(
        qr.apply(QrAction::PickColor(SwatchKey::Backdrop(5)), env(true))
            .is_empty()
    );
    let effects = qr.apply(QrAction::ClosePanel, env(true));
    let picked = ThemeChoice::Emoji {
        color: SwatchKey::Backdrop(5),
        emoji: Vec::new(),
    };
    assert_eq!(saves(&effects), vec![picked.clone()]);
    // The same pick again is not sent twice while it is saving.
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    assert!(qr.apply(QrAction::ClosePanel, env(true)).is_empty());
    let ticket = ticket_of(&effects);
    qr.apply(
        QrAction::SaveAnswer {
            ticket,
            result: Ok(picked.clone()),
        },
        env(true),
    );
    assert_eq!(qr.served, Some(picked));
    assert!(qr.saving.is_none());
    assert!(qr.apply(QrAction::Closing, env(true)).is_empty());
}

#[test]
fn closing_the_box_saves_an_open_panels_pick() {
    let mut qr = own_box();
    qr.apply(QrAction::OpenPanel(QrPanel::Emoji), env(true));
    qr.apply(QrAction::ToggleEmoji("🐱".into()), env(true));
    let effects = qr.apply(QrAction::Closing, env(true));
    assert_eq!(
        saves(&effects),
        vec![ThemeChoice::Emoji {
            color: SwatchKey::Default,
            emoji: vec![plain("🐱")]
        }]
    );
}

#[test]
fn picking_the_saved_theme_again_saves_nothing() {
    let mut qr = own_box();
    let served = ThemeChoice::Emoji {
        color: SwatchKey::Backdrop(4),
        emoji: vec![plain("🐱")],
    };
    qr.apply(QrAction::Served(served.clone()), env(true));
    assert_eq!(qr.choice(), served, "the saved theme is shown");
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    qr.apply(QrAction::PickColor(SwatchKey::Default), env(true));
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(4)), env(true));
    assert!(qr.apply(QrAction::ClosePanel, env(true)).is_empty());
}

#[test]
fn a_pick_is_not_overwritten_by_a_later_served_theme() {
    let mut qr = own_box();
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(7)), env(true));
    qr.apply(
        QrAction::Served(ThemeChoice::Emoji {
            color: SwatchKey::Backdrop(2),
            emoji: Vec::new(),
        }),
        env(true),
    );
    assert_eq!(qr.color, SwatchKey::Backdrop(7));
}

#[test]
fn a_gift_replaces_the_emoji_and_an_emoji_replaces_the_gift() {
    let mut qr = own_box();
    qr.apply(QrAction::OpenPanel(QrPanel::Emoji), env(true));
    qr.apply(QrAction::ToggleEmoji("🐱".into()), env(true));
    qr.apply(QrAction::ClosePanel, env(true));
    let effects = qr.apply(QrAction::OpenPanel(QrPanel::Gifts), env(true));
    assert_eq!(effects, vec![QrEffect::LoadGifts { more: false }]);
    assert_eq!(
        qr.apply(QrAction::MoreGifts, env(true)),
        vec![QrEffect::LoadGifts { more: true }]
    );
    qr.apply(QrAction::PickGift(Box::new(gift(9, "r9"))), env(true));
    assert!(qr.emoji.is_empty());
    let appearance = qr.appearance(&[]);
    assert_eq!(appearance.gift, Some(9));
    assert_eq!(appearance.center, Rgb::hex(0x5A_8CF0));
    let effects = qr.apply(QrAction::ClosePanel, env(true));
    assert_eq!(
        saves(&effects),
        vec![ThemeChoice::Gift(Box::new(gift(9, "r9")))]
    );
    assert!(
        qr.apply(QrAction::MoreGifts, env(true)).is_empty(),
        "panel closed"
    );
    qr.apply(QrAction::OpenPanel(QrPanel::Emoji), env(true));
    qr.apply(QrAction::ToggleEmoji("🐶".into()), env(true));
    assert!(qr.gift.is_none());
    assert_eq!(qr.color, SwatchKey::Default);
}

#[test]
fn a_failed_save_is_toasted_and_the_pick_stays() {
    let mut qr = own_box();
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(3)), env(true));
    let effects = qr.apply(QrAction::ClosePanel, env(true));
    let ticket = ticket_of(&effects);
    let effects = qr.apply(
        QrAction::SaveAnswer {
            ticket,
            result: Err("Too many attempts.".into()),
        },
        env(true),
    );
    assert_eq!(
        effects,
        vec![QrEffect::Toast(QrToast::Failed(
            "Too many attempts.".into()
        ))]
    );
    assert_eq!(qr.color, SwatchKey::Backdrop(3));
    let silent = qr.apply(
        QrAction::SaveAnswer {
            ticket,
            result: Err(String::new()),
        },
        env(true),
    );
    assert!(silent.is_empty());
    // Another box's answer is ignored.
    let other = SaveTicket {
        box_id: qr.id + 1,
        serial: ticket.serial,
    };
    let effects = qr.apply(
        QrAction::SaveAnswer {
            ticket: other,
            result: Err("x".into()),
        },
        env(true),
    );
    assert!(effects.is_empty());
}

#[test]
fn losing_the_right_to_save_shows_the_saved_theme_again() {
    let mut qr = own_box();
    let served = ThemeChoice::Emoji {
        color: SwatchKey::Backdrop(2),
        emoji: Vec::new(),
    };
    qr.apply(QrAction::Served(served.clone()), env(true));
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(8)), env(true));
    qr.apply(QrAction::Sync, env(false));
    assert_eq!(qr.panel, None);
    assert_eq!(qr.choice(), served);
    assert!(qr.apply(QrAction::Closing, env(false)).is_empty());
}

#[test]
fn someone_elses_box_shows_their_theme_and_takes_no_picks() {
    let mut qr = PeerQrBox::new(QrSubject::Peer(QrPeer::User(42)), 4);
    let served = ThemeChoice::Gift(Box::new(gift(5, "")));
    qr.apply(QrAction::Served(served.clone()), env(false));
    assert_eq!(qr.choice(), served);
    assert!(
        qr.apply(QrAction::OpenPanel(QrPanel::Gifts), env(false))
            .is_empty()
    );
    assert_eq!(qr.panel, None);
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(1)), env(false));
    assert_eq!(qr.choice(), served);
}

fn backdrop_pick(id: i32) -> ThemeChoice {
    ThemeChoice::Emoji {
        color: SwatchKey::Backdrop(id),
        emoji: Vec::new(),
    }
}

fn pick_and_close(qr: &mut PeerQrBox, key: SwatchKey) -> Vec<QrEffect> {
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    qr.apply(QrAction::PickColor(key), env(true));
    qr.apply(QrAction::ClosePanel, env(true))
}

#[test]
fn a_failed_save_is_sent_again_only_after_another_pick() {
    let mut qr = own_box();
    qr.apply(QrAction::Served(ThemeChoice::default()), env(true));
    let effects = pick_and_close(&mut qr, SwatchKey::Backdrop(3));
    let ticket = ticket_of(&effects);
    qr.apply(
        QrAction::SaveAnswer {
            ticket,
            result: Err("Too many attempts.".into()),
        },
        env(true),
    );
    // Opening and closing a panel, or the box, does not resend it.
    qr.apply(QrAction::OpenPanel(QrPanel::Emoji), env(true));
    assert!(saves(&qr.apply(QrAction::ClosePanel, env(true))).is_empty());
    assert!(saves(&pick_and_close(&mut qr, SwatchKey::Backdrop(3))).is_empty());
    assert!(saves(&qr.apply(QrAction::Closing, env(true))).is_empty());
    // A different pick goes out, and so does the failed one picked again
    // after it.
    let effects = pick_and_close(&mut qr, SwatchKey::Backdrop(4));
    assert_eq!(saves(&effects), vec![backdrop_pick(4)]);
    qr.apply(
        QrAction::SaveAnswer {
            ticket: ticket_of(&effects),
            result: Ok(backdrop_pick(4)),
        },
        env(true),
    );
    assert_eq!(
        saves(&pick_and_close(&mut qr, SwatchKey::Backdrop(3))),
        vec![backdrop_pick(3)]
    );
}

#[test]
fn the_default_is_not_saved_while_the_saved_theme_is_unknown() {
    let mut qr = own_box();
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(true));
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(3)), env(true));
    qr.apply(QrAction::PickColor(SwatchKey::Default), env(true));
    assert!(saves(&qr.apply(QrAction::ClosePanel, env(true))).is_empty());
    // Once the saved theme is known, going back to the default saves.
    qr.apply(QrAction::Served(backdrop_pick(5)), env(true));
    assert_eq!(
        saves(&pick_and_close(&mut qr, SwatchKey::Default)),
        vec![ThemeChoice::default()]
    );
}

#[test]
fn a_gifts_request_for_another_owner_never_reuses_a_serial() {
    use crate::peer_qr::theme::{GIFTS_RETRY_SECS, ThemeOwner};
    let me = ThemeOwner::User(1);
    let channel = ThemeOwner::Supergroup(113);
    let mut gifts = ThemeGifts::default();
    let own = gifts.start(me, false, 0).unwrap();
    let theirs = gifts.start(channel, false, 0).unwrap();
    assert_ne!(own.serial, theirs.serial);
    assert!(!gifts.is_current(me, own.serial));
    assert!(gifts.is_current(channel, theirs.serial));
    // A failed next page waits, then may be asked for again.
    gifts.accept(vec![gift(1, "r1")], "page2".into(), true);
    assert!(gifts.start(channel, true, 0).is_some());
    gifts.fail(10);
    assert_eq!(gifts.list.len(), 1);
    assert!(!gifts.complete && !gifts.wants_more(10 + GIFTS_RETRY_SECS - 1));
    assert!(gifts.wants_more(10 + GIFTS_RETRY_SECS));
}

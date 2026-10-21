use crate::peer_qr::appearance::{
    DEFAULT_CENTER, DEFAULT_EDGE, PeerQrColors, ShareThemeBackdrop, Swatch, SwatchKey,
    gradient_stops, swatches,
};
use crate::peer_qr::color::{Rgb, contrast, mix_lab, to_lab};
use crate::peer_qr::free::{FreeCode, FreeMenuItem, FreeMode, FreeRequest, FreeResult, FreeTicket};
use crate::peer_qr::link::{
    OwnLinkInputs, OwnLinkKind, OwnLinkState, UserLinkData, UserLinkFetch, countdown_text,
    eligible, own_link, username_label,
};
use crate::peer_qr::pattern::{
    Falloff, LAYER_BAND, Metrics, POP_MS, Rect, drift_offset, overlaps, pick_index, pop_scale,
    shrink_scale, tiles,
};
use crate::peer_qr::state::{
    CustomQr, MAX_EMOJI, PeerQrBox, QrAction, QrEffect, QrEnv, QrPanel, QrPeer, QrSubject, QrToast,
    ShownLink,
};
use crate::telegram::requests::PrivacyWho;

fn own_box() -> PeerQrBox {
    PeerQrBox::new(QrSubject::Peer(QrPeer::User(1)), 7)
}

fn custom_box() -> PeerQrBox {
    PeerQrBox::new(
        QrSubject::Custom(CustomQr {
            link: "https://t.me/+invite".into(),
            title: None,
            about: None,
        }),
        8,
    )
}

fn env(now: i64) -> QrEnv {
    QrEnv {
        now,
        price: 50,
        temporary_only: false,
    }
}

fn code(url: &str, expires_at: i64) -> FreeCode {
    FreeCode {
        link: UserLinkData {
            url: url.into(),
            expires_at,
        },
        can_permanent: true,
    }
}

/// The ticket of `qr`'s request in flight.
fn ticket(qr: &PeerQrBox, request: FreeRequest) -> FreeTicket {
    FreeTicket {
        request,
        box_id: qr.id,
        serial: qr.free.serial,
    }
}

fn answer(qr: &PeerQrBox, request: FreeRequest, result: FreeResult) -> QrAction {
    QrAction::FreeAnswer {
        ticket: ticket(qr, request),
        result,
    }
}

fn inputs<'a>(contact: &'a UserLinkFetch) -> OwnLinkInputs<'a> {
    OwnLinkInputs {
        username: "",
        phone: "15551234567",
        phone_label: "+1 555 123 4567",
        show_phone: Some(PrivacyWho::Everybody),
        find_by_phone: Some(PrivacyWho::Everybody),
        contact,
        now: 1_000,
    }
}

#[test]
fn self_and_public_peers_are_eligible() {
    assert!(eligible(true, &[]));
    assert!(eligible(false, &["durov".into()]));
    assert!(!eligible(false, &[]));
    assert!(!eligible(false, &[String::new()]));
}

#[test]
fn username_label_is_uppercase_with_at() {
    assert_eq!(username_label("durov"), "@DUROV");
    assert_eq!(username_label("@Quill_App"), "@QUILL_APP");
}

#[test]
fn own_username_link_wins() {
    let contact = UserLinkFetch::Idle;
    let state = own_link(OwnLinkInputs {
        username: "idan",
        show_phone: None,
        find_by_phone: None,
        ..inputs(&contact)
    });
    let OwnLinkState::Ready(link) = state else {
        panic!("expected a link, got {state:?}");
    };
    assert_eq!(link.kind, OwnLinkKind::Username);
    assert_eq!(link.link, "https://t.me/idan");
    assert_eq!(link.label, "@IDAN");
    assert_eq!(link.expires_at, 0);
}

#[test]
fn own_link_waits_for_phone_privacy() {
    let contact = UserLinkFetch::Idle;
    let state = own_link(OwnLinkInputs {
        find_by_phone: None,
        ..inputs(&contact)
    });
    assert_eq!(state, OwnLinkState::Waiting);
}

#[test]
fn visible_number_encodes_the_phone_link() {
    let contact = UserLinkFetch::Idle;
    let OwnLinkState::Ready(link) = own_link(inputs(&contact)) else {
        panic!("expected a phone link");
    };
    assert_eq!(link.kind, OwnLinkKind::Phone);
    assert_eq!(link.link, "https://t.me/+15551234567");
    assert_eq!(link.label, "+1 555 123 4567");
}

#[test]
fn hidden_number_findable_by_everyone_still_uses_the_phone() {
    let contact = UserLinkFetch::Idle;
    let state = own_link(OwnLinkInputs {
        show_phone: Some(PrivacyWho::Nobody),
        ..inputs(&contact)
    });
    assert!(matches!(state, OwnLinkState::Ready(link) if link.kind == OwnLinkKind::Phone));
}

#[test]
fn hidden_number_asks_for_a_contact_link() {
    let hidden = |contact| {
        own_link(OwnLinkInputs {
            show_phone: Some(PrivacyWho::Nobody),
            find_by_phone: Some(PrivacyWho::Contacts),
            ..inputs(contact)
        })
    };
    assert_eq!(hidden(&UserLinkFetch::Idle), OwnLinkState::NeedsContactLink);
    assert_eq!(hidden(&UserLinkFetch::Loading), OwnLinkState::Waiting);
    assert_eq!(hidden(&UserLinkFetch::Failed), OwnLinkState::Unavailable);
    let live = UserLinkFetch::Ready(UserLinkData {
        url: "https://t.me/contact/abc".into(),
        expires_at: 1_075,
    });
    let OwnLinkState::Ready(link) = hidden(&live) else {
        panic!("expected a contact link");
    };
    assert_eq!(link.kind, OwnLinkKind::Contact);
    assert_eq!(link.label, "1:15");
    assert_eq!(link.expires_at, 1_075);
    let expired = UserLinkFetch::Ready(UserLinkData {
        url: "https://t.me/contact/abc".into(),
        expires_at: 900,
    });
    assert_eq!(hidden(&expired), OwnLinkState::NeedsContactLink);
}

#[test]
fn countdown_reads_minutes_and_seconds() {
    assert_eq!(countdown_text(59), "0:59");
    assert_eq!(countdown_text(600), "10:00");
    assert_eq!(countdown_text(-4), "0:00");
}

#[test]
fn default_backdrop_has_white_glyphs_and_dark_ink() {
    let colors = PeerQrColors::compute(DEFAULT_CENTER, DEFAULT_EDGE);
    assert_eq!(colors.foreground, Rgb::WHITE);
    assert!(contrast(colors.ink, Rgb::WHITE) >= 4.4);
    assert!(colors.control.alpha < 0.2);
}

#[test]
fn light_backdrop_ink_keeps_contrast_on_the_white_card() {
    let center = Rgb::hex(0xFFE7A0);
    let edge = Rgb::hex(0xF5C45C);
    let colors = PeerQrColors::compute(center, edge);
    assert!(contrast(colors.ink, Rgb::WHITE) >= 4.3);
    assert_ne!(colors.foreground, Rgb::WHITE);
    let hue = |c: Rgb| to_lab(c).hue();
    assert!((hue(colors.ink) - hue(edge)).abs() < 30.);
}

#[test]
fn gradient_runs_from_center_to_edge() {
    let stops = gradient_stops(DEFAULT_CENTER, DEFAULT_EDGE);
    assert_eq!(stops.first().copied(), Some(DEFAULT_CENTER));
    assert_eq!(stops.last().copied(), Some(DEFAULT_EDGE));
    assert_eq!(mix_lab(Rgb::BLACK, Rgb::BLACK, 0.5), Rgb::BLACK);
}

fn backdrop(id: i32, center: u32, edge: u32) -> ShareThemeBackdrop {
    ShareThemeBackdrop {
        id,
        name: format!("b{id}"),
        center: Rgb::hex(center),
        edge: Rgb::hex(edge),
    }
}

#[test]
fn default_swatch_comes_first_unless_served() {
    let list = swatches(&[backdrop(3, 0x5A_8CF0, 0x2B_4FB5)]);
    assert_eq!(
        list.iter().map(|s| s.key).collect::<Vec<_>>(),
        vec![SwatchKey::Default, SwatchKey::Backdrop(3)]
    );
    let served = swatches(&[
        backdrop(3, 0x5A_8CF0, 0x2B_4FB5),
        backdrop(9, DEFAULT_CENTER.to_hex(), DEFAULT_EDGE.to_hex()),
    ]);
    assert_eq!(
        served.iter().map(|s| s.key).collect::<Vec<_>>(),
        vec![SwatchKey::Backdrop(3), SwatchKey::Default]
    );
}

#[test]
fn picking_a_color_changes_the_appearance() {
    let list = swatches(&[backdrop(3, 0x5A_8CF0, 0x2B_4FB5)]);
    let mut qr = own_box();
    assert_eq!(qr.appearance(&list).center, DEFAULT_CENTER);
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(0));
    assert_eq!(qr.panel, Some(QrPanel::Colors));
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(3)), env(0));
    assert_eq!(qr.appearance(&list).center, Rgb::hex(0x5A_8CF0));
    qr.apply(QrAction::ClosePanel, env(0));
    assert_eq!(qr.panel, None);
    assert_eq!(qr.color, SwatchKey::Backdrop(3));
    assert_eq!(qr.appearance(&[Swatch::DEFAULT]).center, DEFAULT_CENTER);
}

#[test]
fn emoji_toggle_and_keep_the_last_six() {
    let mut qr = own_box();
    for emoji in ["😀", "🐱", "🍕", "🎉", "🚀", "🌈", "⭐"] {
        qr.apply(QrAction::ToggleEmoji(emoji.into()), env(0));
    }
    assert_eq!(qr.emoji.len(), MAX_EMOJI);
    assert_eq!(qr.emoji.first().map(String::as_str), Some("🐱"));
    qr.apply(QrAction::ToggleEmoji("🍕".into()), env(0));
    assert_eq!(qr.emoji, vec!["🐱", "🎉", "🚀", "🌈", "⭐"]);
    assert_eq!(qr.appearance(&[]).emoji, qr.emoji);
}

#[test]
fn custom_links_have_no_colors_or_emoji() {
    let mut qr = custom_box();
    qr.apply(QrAction::OpenPanel(QrPanel::Emoji), env(0));
    qr.apply(QrAction::PickColor(SwatchKey::Backdrop(3)), env(0));
    qr.apply(QrAction::ToggleEmoji("😀".into()), env(0));
    assert_eq!(qr, custom_box());
}

#[test]
fn second_panel_waits_for_the_first_to_close() {
    let mut qr = own_box();
    qr.apply(QrAction::OpenPanel(QrPanel::Emoji), env(0));
    qr.apply(QrAction::OpenPanel(QrPanel::Colors), env(0));
    assert_eq!(qr.panel, Some(QrPanel::Emoji));
}

#[test]
fn pill_requests_a_temporary_code_then_shows_it() {
    let mut qr = own_box();
    let effects = qr.apply(QrAction::PillClicked, env(100));
    assert_eq!(
        effects,
        vec![QrEffect::RequestFreeCode(ticket(
            &qr,
            FreeRequest::Temporary
        ))]
    );
    assert!(qr.apply(QrAction::PillClicked, env(100)).is_empty());
    let effects = qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        env(100),
    );
    assert_eq!(effects, vec![QrEffect::Toast(QrToast::CodeExpires(60))]);
    assert!(matches!(qr.free.mode, FreeMode::Temporary { .. }));
    let plain = ShownLink {
        link: "https://t.me/idan".into(),
        expires_at: 0,
        free_code: false,
    };
    let shown = qr.shown_link(Some(plain.clone()), env(100));
    assert_eq!(
        shown.as_ref().map(|s| s.link.as_str()),
        Some("https://t.me/free")
    );
    assert_eq!(qr.check_share(shown, 100).map(|s| s.expires_at), Ok(160));
    let near = qr.shown_link(Some(plain.clone()), env(158));
    assert_eq!(
        qr.check_share(near, 158),
        Err(Some(QrToast::Expiring { free_code: true }))
    );
    let effects = qr.apply(QrAction::Tick, env(160));
    assert_eq!(effects, vec![QrEffect::Toast(QrToast::CodeExpired)]);
    assert_eq!(qr.free.mode, FreeMode::Fee);
    assert_eq!(qr.shown_link(Some(plain.clone()), env(160)), Some(plain));
}

#[test]
fn unasked_and_empty_answers() {
    let mut qr = own_box();
    let unasked = qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        env(100),
    );
    assert!(unasked.is_empty());
    assert_eq!(qr.free.mode, FreeMode::Fee);
    qr.apply(QrAction::PillClicked, env(100));
    assert_eq!(qr.check_share(None, 100), Err(None));
    let empty = qr.apply(
        answer(&qr, FreeRequest::Temporary, Ok(code("", 160))),
        env(100),
    );
    assert_eq!(empty, vec![QrEffect::Toast(QrToast::Unavailable)]);
    assert_eq!(qr.free.requesting, None);
    assert_eq!(qr.check_share(None, 100), Err(Some(QrToast::Unavailable)));
}

#[test]
fn a_server_error_is_shown_and_keeps_the_code() {
    let mut qr = own_box();
    qr.apply(QrAction::PillClicked, env(100));
    qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        env(100),
    );
    qr.apply(QrAction::PillClicked, env(101));
    qr.apply(QrAction::MenuChose(FreeMenuItem::Reset), env(101));
    let effects = qr.apply(
        answer(
            &qr,
            FreeRequest::ResetTemporary,
            Err("Too many tries".into()),
        ),
        env(102),
    );
    assert_eq!(
        effects,
        vec![QrEffect::Toast(QrToast::Failed("Too many tries".into()))]
    );
    assert!(matches!(qr.free.mode, FreeMode::Temporary { .. }));
    assert_eq!(qr.free.requesting, None);
}

#[test]
fn an_empty_reset_answer_restores_the_fee() {
    let mut qr = own_box();
    qr.apply(QrAction::PillClicked, env(100));
    qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        env(100),
    );
    qr.apply(QrAction::MenuChose(FreeMenuItem::Reset), env(101));
    let effects = qr.apply(
        answer(&qr, FreeRequest::ResetTemporary, Ok(code("", 0))),
        env(101),
    );
    assert_eq!(effects, vec![QrEffect::Toast(QrToast::Unavailable)]);
    assert_eq!(qr.free.mode, FreeMode::Fee);
}

#[test]
fn answers_for_another_box_or_a_dropped_request_are_ignored() {
    let mut qr = own_box();
    qr.apply(QrAction::PillClicked, env(100));
    let other_box = QrAction::FreeAnswer {
        ticket: FreeTicket {
            box_id: qr.id + 1,
            ..ticket(&qr, FreeRequest::Temporary)
        },
        result: Ok(code("https://t.me/old", 160)),
    };
    assert!(qr.apply(other_box, env(100)).is_empty());
    assert_eq!(qr.free.requesting, Some(FreeRequest::Temporary));
    let dropped = ticket(&qr, FreeRequest::Temporary);
    qr.apply(
        QrAction::Sync,
        QrEnv {
            price: 0,
            ..env(100)
        },
    );
    assert_eq!(qr.free.requesting, None);
    qr.apply(QrAction::PillClicked, env(101));
    let late = QrAction::FreeAnswer {
        ticket: dropped,
        result: Ok(code("https://t.me/late", 160)),
    };
    assert!(qr.apply(late, env(101)).is_empty());
    assert_eq!(qr.free.mode, FreeMode::Fee);
}

#[test]
fn menu_switches_timer_resets_and_restores() {
    let mut qr = own_box();
    qr.apply(QrAction::PillClicked, env(100));
    qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        env(100),
    );
    qr.apply(QrAction::PillClicked, env(101));
    assert!(qr.free.menu_open);
    assert_eq!(
        qr.free.menu_items(false),
        vec![
            FreeMenuItem::Reset,
            FreeMenuItem::ToggleTimer,
            FreeMenuItem::RestoreFee
        ]
    );
    assert_eq!(
        qr.free.menu_items(true),
        vec![FreeMenuItem::Reset, FreeMenuItem::RestoreFee]
    );
    let effects = qr.apply(QrAction::MenuChose(FreeMenuItem::ToggleTimer), env(101));
    assert_eq!(
        effects,
        vec![QrEffect::RequestFreeCode(ticket(
            &qr,
            FreeRequest::Permanent
        ))]
    );
    assert!(!qr.free.menu_open);
    let effects = qr.apply(
        answer(
            &qr,
            FreeRequest::Permanent,
            Ok(code("https://t.me/forever", 0)),
        ),
        env(102),
    );
    assert_eq!(effects, vec![QrEffect::Toast(QrToast::CodePermanent)]);
    let effects = qr.apply(QrAction::MenuChose(FreeMenuItem::Reset), env(103));
    assert_eq!(
        effects,
        vec![QrEffect::RequestFreeCode(ticket(
            &qr,
            FreeRequest::ResetPermanent
        ))]
    );
    let effects = qr.apply(
        answer(
            &qr,
            FreeRequest::ResetPermanent,
            Ok(code("https://t.me/forever2", 0)),
        ),
        env(103),
    );
    assert_eq!(effects, vec![QrEffect::Toast(QrToast::CodeReset)]);
    let effects = qr.apply(QrAction::MenuChose(FreeMenuItem::RestoreFee), env(104));
    assert_eq!(effects, vec![QrEffect::Toast(QrToast::FeeRestored)]);
    assert_eq!(qr.free.mode, FreeMode::Fee);
}

#[test]
fn free_messages_turn_codes_off() {
    let mut qr = own_box();
    let free = QrEnv {
        price: 0,
        ..env(100)
    };
    assert!(qr.apply(QrAction::PillClicked, free).is_empty());
    qr.apply(QrAction::PillClicked, env(100));
    qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        env(100),
    );
    qr.apply(QrAction::Sync, free);
    assert_eq!(qr.free.mode, FreeMode::Fee);
}

#[test]
fn temporary_only_accounts_cannot_ask_for_permanent_codes() {
    let mut qr = own_box();
    let only = QrEnv {
        temporary_only: true,
        ..env(100)
    };
    qr.apply(QrAction::PillClicked, only);
    qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        only,
    );
    assert!(
        qr.apply(QrAction::MenuChose(FreeMenuItem::ToggleTimer), only)
            .is_empty()
    );
}

#[test]
fn a_link_turning_temporary_drops_the_permanent_code() {
    let mut qr = own_box();
    qr.apply(QrAction::PillClicked, env(100));
    qr.apply(
        answer(
            &qr,
            FreeRequest::Temporary,
            Ok(code("https://t.me/free", 160)),
        ),
        env(100),
    );
    qr.apply(QrAction::MenuChose(FreeMenuItem::ToggleTimer), env(101));
    qr.apply(
        answer(
            &qr,
            FreeRequest::Permanent,
            Ok(code("https://t.me/forever", 0)),
        ),
        env(101),
    );
    assert!(matches!(qr.free.mode, FreeMode::Permanent { .. }));
    let only = QrEnv {
        temporary_only: true,
        ..env(102)
    };
    qr.apply(QrAction::Sync, only);
    assert_eq!(qr.free.mode, FreeMode::Fee);
}

#[test]
fn picks_repeat_along_rows() {
    let picks: Vec<usize> = (0..6).map(|column| pick_index(0, column, 3)).collect();
    assert_eq!(picks, vec![0, 1, 2, 0, 1, 2]);
    assert_eq!(pick_index(5, -7, 4), pick_index(5, -3, 4));
    assert_eq!(pick_index(1, 1, 0), 0);
}

fn falloff() -> Falloff {
    Falloff {
        area: Rect::new(0., 0., 390., 520.),
        band: LAYER_BAND,
    }
}

#[test]
fn tiles_shrink_towards_the_edges() {
    let area = falloff();
    let all = tiles(&area, area.area.center(), &Metrics::LIVE, 3, 0.);
    assert!(all.len() > 20, "{} tiles", all.len());
    assert!(all.iter().all(|tile| tile.pick < 3));
    let (cx, cy) = area.area.center();
    let middle = all
        .iter()
        .min_by(|a, b| {
            let d = |t: &&crate::peer_qr::pattern::Tile| (t.x - cx).hypot(t.y - cy);
            d(a).total_cmp(&d(b))
        })
        .map(|tile| tile.size)
        .unwrap_or(0.);
    let edge = all
        .iter()
        .filter(|tile| tile.x < 20. || tile.y < 20.)
        .map(|tile| tile.size)
        .fold(f64::MAX, f64::min);
    assert!(edge < middle * 0.6, "edge {edge} middle {middle}");
    assert!(tiles(&area, (0., 0.), &Metrics::LIVE, 0, 0.).is_empty());
}

#[test]
fn tiles_touching_the_card_are_left_out() {
    let card = Rect::new(51., 76., 288., 308.);
    let (cx, cy) = card.center();
    assert!(overlaps(&card, cx, cy, 30.));
    assert!(overlaps(&card, 40., 100., 30.));
    assert!(!overlaps(&card, 30., 100., 30.));
    assert!(!overlaps(&card, 10., 10., 30.));
}

#[test]
fn layer_drifts_six_pixels_a_second_upwards() {
    let (x, y) = drift_offset(10_000);
    assert!((x.hypot(y) - 60.).abs() < 0.5, "{x} {y}");
    assert!(x < 0. && y < 0., "{x} {y}");
    assert_eq!(drift_offset(0), (0., 0.));
}

#[test]
fn tiles_pop_with_overshoot_and_shrink_away() {
    assert_eq!(pop_scale(0, 10), 0.);
    assert_eq!(pop_scale(10 + POP_MS, 10), 1.);
    assert!(pop_scale(10 + POP_MS / 2, 10) > 1.);
    assert_eq!(shrink_scale(0, 10), 1.);
    assert_eq!(shrink_scale(1_000, 10), 0.);
}

#[test]
fn card_matches_tdesktop_metrics() {
    use crate::peer_qr::layout::card_layout;
    let card = card_layout(288., true);
    assert_eq!((card.width, card.height, card.radius), (288., 308., 36.));
    assert_eq!(card.qr, Rect::new(43., 35., 202., 202.));
    assert_eq!(card.logo, Rect::new(126., 118., 35., 35.));
    assert_eq!(card.band, Rect::new(0., 237., 288., 71.));
    let square = card_layout(288., false);
    assert_eq!(square.height, 288.);
    assert_eq!(square.qr.y, 43.);
}

#[test]
fn opening_a_panel_shrinks_the_card_above_it() {
    use crate::peer_qr::layout::{box_layout, panel_height, placement};
    let layout = box_layout(true, 2);
    assert_eq!(layout.height, 499.);
    assert_eq!(layout.island, Rect::new(147., 443., 96., 40.));
    assert_eq!(panel_height(370.), 210.);
    let closed = placement(&layout, 0.);
    assert_eq!(closed.card, layout.card);
    assert_eq!(closed.panel, layout.island);
    let open = placement(&layout, 1.);
    assert_eq!(open.panel, Rect::new(10., 279., 370., 210.));
    assert_eq!(open.pill_top, 241.);
    assert!((open.scale - 212. / 308.).abs() < 1e-9);
    assert!((open.card.y + open.card.height - 228.).abs() < 1e-9);
}

#[test]
fn panel_easing_runs_from_zero_to_one_fast_first() {
    use crate::peer_qr::layout::{content_in, panel_ease};
    assert!(panel_ease(0.).abs() < 1e-6);
    assert!((panel_ease(1.) - 1.).abs() < 1e-6);
    assert!(panel_ease(0.25) > 0.6);
    assert_eq!(content_in(0.), 0.);
    assert_eq!(content_in(400.), 1.);
}

#[test]
fn a_long_name_goes_on_two_lines_in_a_smaller_font() {
    use crate::peer_qr::layout::fit_username;
    let measure = |text: &str, size: f64| text.chars().count() as f64 * size * 0.6;
    let line = |size: f64| size * 1.2;
    let short = fit_username("@QUILL", 252., 71., &measure, &line).unwrap();
    assert_eq!((short.size, short.lines.len()), (36., 1));
    let long = fit_username("@AVERYLONGUSERNAMEHERE", 252., 71., &measure, &line).unwrap();
    assert_eq!(long.lines.concat(), "@AVERYLONGUSERNAMEHERE");
    assert_eq!(long.lines.len(), 2);
    assert!(long.size <= 23.);
    assert!(fit_username("", 252., 71., &measure, &line).is_none());
}

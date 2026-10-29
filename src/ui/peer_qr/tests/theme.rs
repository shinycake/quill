//! The share theme in the box (`parity:profile-share-theme`): a picked
//! backdrop is saved when its panel closes, someone else's box shows
//! their saved theme and takes no picks, and the gifts panel lists your
//! gifts, saves the one picked and offers Browse Gifts when there are
//! none.
use super::super::demo::{
    BACKDROPS, DEMO_GIFTS_PAGE, FRIEND, GROUP_CHAT, ME, demo_gift, demo_gifts_page,
};
use super::*;
use quill::peer_qr::gift_tiles::gifts_panel_height;
use quill::peer_qr::layout::panel_height;
use quill::peer_qr::theme::{GiftsLayout, ShareTheme, ThemeChoice, ThemeEmoji, ThemeOwner};

fn served(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    owner: ThemeOwner,
) -> Option<ShareTheme> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.users_state.peer_qr.themes.get(&owner).cloned())
    })
}

fn keep_theme(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    owner: ThemeOwner,
    theme: ShareTheme,
) {
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.users_state.peer_qr.keep_theme(owner, theme);
        }
    });
}

fn set_gift_count(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, count: usize) {
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.users_state.peer_qr.gifts.list = (0..count).map(demo_gift).collect();
        }
    });
}

fn panel_bounds_height(vcx: &mut VisualTestContext) -> f64 {
    let bounds = vcx.debug_bounds("peer-qr-panel").expect("the panel");
    f64::from(f32::from(bounds.size.height))
}

#[gpui_kit::test]
fn a_backdrop_picked_in_your_box_is_saved_when_the_panel_closes(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let (id, _, _) = BACKDROPS[2];
    click(vcx, "peer-qr-colors");
    click(vcx, "peer-qr-swatch-3");
    assert_eq!(
        served(&app, vcx, ThemeOwner::User(ME)),
        None,
        "a pick previews until the panel closes"
    );
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(
        served(&app, vcx, ThemeOwner::User(ME)),
        Some(ShareTheme::Emoji {
            backdrop: Some(id),
            emoji: Vec::new()
        })
    );
    app.read_with(vcx, |app, _| {
        let open = app.peer_qr.open.as_ref().expect("the box stays");
        assert_eq!(open.state.saving, None);
        assert_eq!(
            open.state.served,
            Some(ThemeChoice::Emoji {
                color: SwatchKey::Backdrop(id),
                emoji: Vec::new()
            })
        );
    });
    // Closing the box with nothing new picked saves nothing more.
    keep_theme(&app, vcx, ThemeOwner::User(ME), ShareTheme::Default);
    close_box(&app, vcx);
    assert_eq!(
        served(&app, vcx, ThemeOwner::User(ME)),
        Some(ShareTheme::Default)
    );
}

#[gpui_kit::test]
fn someone_elses_box_shows_their_saved_theme(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    close_box(&app, vcx);
    let (id, center, edge) = BACKDROPS[2];
    keep_theme(
        &app,
        vcx,
        ThemeOwner::User(FRIEND),
        ShareTheme::Emoji {
            backdrop: Some(id),
            emoji: vec![ThemeEmoji::Plain("🐱".into())],
        },
    );
    app.update(vcx, |app, cx| {
        app.open_peer_qr_box(QrPeer::User(FRIEND), cx)
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let view = app.qr_view().expect("view");
        assert_eq!(view.appearance.center, Rgb::hex(center));
        assert_eq!(view.appearance.edge, Rgb::hex(edge));
        assert_eq!(view.appearance.emoji, vec![ThemeEmoji::Plain("🐱".into())]);
        assert!(!view.can_style);
    });
    for selector in ["peer-qr-colors", "peer-qr-emoji", "peer-qr-gifts"] {
        assert!(!shown(vcx, selector), "{selector}");
    }
    close_box(&app, vcx);
    let gift = demo_gift(3);
    keep_theme(
        &app,
        vcx,
        ThemeOwner::User(FRIEND),
        ShareTheme::Gift(Box::new(gift.clone())),
    );
    app.update(vcx, |app, cx| {
        app.open_peer_qr_box(QrPeer::User(FRIEND), cx)
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let view = app.qr_view().expect("view");
        assert_eq!(view.appearance.gift, Some(gift.id));
        assert_eq!(view.appearance.center, gift.center);
        assert!(
            app.qr_gift_model(&gift).is_some(),
            "the model still is made for the pattern"
        );
        assert!(app.qr_gift_symbol(&gift).is_some());
    });
    // Nothing of theirs is saved by closing.
    close_box(&app, vcx);
    assert_eq!(
        served(&app, vcx, ThemeOwner::User(FRIEND)),
        Some(ShareTheme::Gift(Box::new(gift)))
    );
}

#[gpui_kit::test]
fn the_gifts_panel_lists_your_gifts_and_saves_the_one_picked(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    click(vcx, "peer-qr-gifts");
    for index in 0..5 {
        let selector: &'static str = match index {
            0 => "peer-qr-gift-0",
            1 => "peer-qr-gift-1",
            2 => "peer-qr-gift-2",
            3 => "peer-qr-gift-3",
            _ => "peer-qr-gift-4",
        };
        assert!(shown(vcx, selector), "{selector}");
    }
    let width = BOX_WIDTH - 2. * PANEL_MARGIN;
    assert!(
        (panel_bounds_height(vcx) - panel_height(width)).abs() < 1.,
        "a grid opens full"
    );
    click(vcx, "peer-qr-gift-1");
    let picked = demo_gift(1);
    app.read_with(vcx, |app, _| {
        let open = app.peer_qr.open.as_ref().expect("open");
        assert_eq!(
            open.state.gift.as_ref().map(|gift| gift.id),
            Some(picked.id)
        );
        assert!(open.state.emoji.is_empty());
        assert_eq!(
            app.qr_view().and_then(|view| view.appearance.gift),
            Some(picked.id)
        );
    });
    assert_eq!(served(&app, vcx, ThemeOwner::User(ME)), None);
    close_box(&app, vcx);
    let Some(ShareTheme::Gift(saved)) = served(&app, vcx, ThemeOwner::User(ME)) else {
        panic!("the gift was not saved");
    };
    assert_eq!(saved.id, picked.id);
    assert_eq!(saved.received_id, "demo-gift-1");
}

#[gpui_kit::test]
fn a_short_gift_list_opens_the_panel_only_as_tall_as_its_tiles(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    set_gift_count(&app, vcx, 1);
    click(vcx, "peer-qr-gifts");
    let width = BOX_WIDTH - 2. * PANEL_MARGIN;
    let expected = gifts_panel_height(GiftsLayout::Wide(1), width, 1, true);
    assert!(expected < panel_height(width));
    assert!((panel_bounds_height(vcx) - expected).abs() < 1.);
    assert!(shown(vcx, "peer-qr-gift-0"));
}

#[gpui_kit::test]
fn with_no_gifts_the_panel_offers_browse_gifts_in_your_own_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    set_gift_count(&app, vcx, 0);
    click(vcx, "peer-qr-gifts");
    assert!(shown(vcx, "peer-qr-gifts-empty"));
    assert!(shown(vcx, "peer-qr-gifts-browse"));
    close_box(&app, vcx);
    // A group's box styles but cannot be sent gifts.
    app.update(vcx, |app, cx| {
        app.open_peer_qr_box(QrPeer::Chat(GROUP_CHAT), cx)
    });
    frames(vcx);
    click(vcx, "peer-qr-gifts");
    assert!(shown(vcx, "peer-qr-gifts-empty"));
    assert!(!shown(vcx, "peer-qr-gifts-browse"));
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::PeerQr)
    );
}

fn gifts_listed(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (usize, String) {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|session| {
                let gifts = &session.users_state.peer_qr.gifts;
                (gifts.list.len(), gifts.offset.clone())
            })
            .unwrap_or_default()
    })
}

fn wheel(vcx: &mut VisualTestContext, at: gpui_kit::Point<gpui_kit::Pixels>, dy: f32) {
    for (phase, dy) in [
        (gpui_kit::TouchPhase::Started, dy),
        (gpui_kit::TouchPhase::Moved, dy),
        (gpui_kit::TouchPhase::Ended, 0.),
    ] {
        vcx.simulate_event(gpui_kit::ScrollEvent {
            position: at,
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(dy))),
            modifiers: Modifiers::default(),
            touch_phase: phase,
            source: gpui_kit::ScrollSource::Pointer,
        });
    }
}

#[gpui_kit::test]
fn scrolling_the_gifts_grid_to_its_end_brings_the_next_page(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            let gifts = &mut session.users_state.peer_qr.gifts;
            let (list, offset) = demo_gifts_page(0, 30);
            gifts.list = list;
            gifts.complete = false;
            gifts.offset = offset;
        }
    });
    click(vcx, "peer-qr-gifts");
    assert_eq!(
        gifts_listed(&app, vcx),
        (DEMO_GIFTS_PAGE, format!("demo-{DEMO_GIFTS_PAGE}-30")),
        "the first rows fill the panel: no page asked for yet"
    );
    let before = vcx.debug_bounds("peer-qr-gift-3").expect("the second row");
    let at = vcx
        .debug_bounds("peer-qr-gift-1")
        .expect("the first row")
        .center();
    wheel(vcx, at, -40.);
    frames(vcx);
    let after = vcx.debug_bounds("peer-qr-gift-3").expect("still in view");
    assert!(after.origin.y < before.origin.y, "the grid scrolls");
    for _ in 0..12 {
        wheel(vcx, at, -200.);
        frames(vcx);
    }
    // Each end reached asks for the next page, until the last.
    assert_eq!(gifts_listed(&app, vcx), (30, String::new()));
    assert!(
        shown(vcx, "peer-qr-gift-29"),
        "the last page's tiles are drawn"
    );
    assert!(!shown(vcx, "peer-qr-gift-0"), "scrolled past the first row");
    click(vcx, "peer-qr-gift-29");
    app.read_with(vcx, |app, _| {
        let open = app.peer_qr.open.as_ref().expect("open");
        assert_eq!(
            open.state.gift.as_ref().map(|gift| gift.id),
            Some(demo_gift(29).id)
        );
    });
}

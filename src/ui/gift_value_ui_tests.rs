//! The gift value box in a real window (needs the `demo-capture` feature
//! for gpui-kit's test support): the Value row, "learn more", the box
//! with its table and links, OK, and the Telegram link to the resale list.
use crate::ui::app::QuillApp;
use crate::ui::gift_market_state::MarketBox;
use crate::ui::gift_value_demo::{AVERAGE, LAST, fixture};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::premium_hub::gift_value::{GiftValueInfo, GiftValueLookup};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    quill::gift_market::set_gift_market_persistence(false);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(1400.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-gift-value")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// Sets the demo's value lookup (`None` = nothing asked yet).
fn set_lookup(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    make: impl FnOnce(GiftValueLookup) -> Option<GiftValueLookup>,
) {
    app.update_in(vcx, |app, _, cx| {
        let lookup = app.demo_gift_value_lookup(None).and_then(make);
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.hub.gift_value = lookup;
        }
        cx.notify();
    });
    vcx.run_until_parked();
}

fn lookup_of(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<GiftValueLookup> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.payments.hub.gift_value.clone())
    })
}

/// Lets the dialog's open animation finish and lays out a fresh frame, so
/// the bounds measured next are where the element really is.
fn settle(vcx: &mut VisualTestContext) {
    // The dialog animation runs on the wall clock, which `advance_clock`
    // does not move; reduced motion draws it at rest, so the layout does not
    // depend on how fast this thread got to the click. `QuillApp` resets the
    // flag when it is built, so it is set here, after.
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    settle(vcx);
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not on screen"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    vcx.run_until_parked();
}

fn with_info(
    info: Option<GiftValueInfo>,
    open: bool,
) -> impl FnOnce(GiftValueLookup) -> Option<GiftValueLookup> {
    move |lookup| {
        Some(GiftValueLookup {
            info,
            open,
            ..lookup
        })
    }
}

#[gpui_kit::test]
fn the_box_shows_price_about_table_and_links_and_ok_closes_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_lookup(&app, vcx, with_info(fixture(LAST), true));
    for id in [
        "gift-value-box",
        "gift-value-price",
        "gift-value-about",
        "gift-value-tip-minimum-price",
        "gift-value-tip-average-price",
        "gift-value-on-telegram",
        "gift-value-on-fragment",
    ] {
        assert!(vcx.debug_bounds(id).is_some(), "{id}");
    }
    // The box covers the gift box it came from.
    assert!(vcx.debug_bounds("gift-value-row").is_none());

    click(vcx, "gift-value-ok");
    let lookup = lookup_of(&app, vcx).expect("lookup");
    assert!(!lookup.open && lookup.info.is_some());
    assert!(vcx.debug_bounds("gift-value-box").is_none());
    assert!(vcx.debug_bounds("gift-value-row").is_some());
}

#[gpui_kit::test]
fn learn_more_spins_while_loading_and_reopens_a_known_value(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_lookup(&app, vcx, |_| None);
    assert!(vcx.debug_bounds("gift-value-row").is_some());
    assert!(vcx.debug_bounds("gift-value-box").is_none());

    // Nothing known: the pill loads, the box stays closed.
    click(vcx, "gift-value-learn-more");
    let lookup = lookup_of(&app, vcx).expect("lookup");
    assert!(lookup.loading && !lookup.open);
    assert_eq!(lookup.name, "PlushPepe-1204");
    assert!(vcx.debug_bounds("gift-value-spinner").is_some());
    assert!(vcx.debug_bounds("gift-value-box").is_none());

    // Known and closed: the click opens the box again.
    set_lookup(&app, vcx, with_info(fixture(AVERAGE), false));
    click(vcx, "gift-value-learn-more");
    assert!(lookup_of(&app, vcx).is_some_and(|lookup| lookup.open));
    assert!(vcx.debug_bounds("gift-value-box").is_some());
    // The average answer: no last price, no Fragment listings.
    assert!(vcx.debug_bounds("gift-value-on-telegram").is_some());
    assert!(vcx.debug_bounds("gift-value-on-fragment").is_none());
}

#[gpui_kit::test]
fn a_failed_request_shows_under_the_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_lookup(&app, vcx, |lookup| {
        Some(GiftValueLookup {
            error: Some("Couldn't load the value: STARGIFT_SLUG_INVALID".into()),
            ..lookup
        })
    });
    assert!(vcx.debug_bounds("gift-value-error").is_some());
    assert!(vcx.debug_bounds("gift-value-box").is_none());
}

#[gpui_kit::test]
fn telegram_listings_open_the_resale_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_lookup(&app, vcx, with_info(fixture(LAST), true));
    click(vcx, "gift-value-on-telegram");
    assert!(lookup_of(&app, vcx).is_some_and(|lookup| !lookup.open));
    let top = app.read_with(vcx, |app, _| app.gift_market.top().cloned());
    assert_eq!(top, Some(MarketBox::Resale));
    assert!(vcx.debug_bounds("gift-value-box").is_none());
}

#[gpui_kit::test]
fn a_look_alike_fragment_address_asks_before_opening(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // "fragmеnt.com" with a Cyrillic "е": the link policy asks first.
    let url = "https://fragm\u{0435}nt.com/gifts/plushpepe";
    let link = quill::text::LinkTarget::Url {
        url: url.into(),
        label: None,
    };
    assert!(matches!(
        quill::link_policy::open_decision(&link),
        quill::link_policy::OpenDecision::Confirm { .. }
    ));
    let info = fixture(LAST).map(|info| GiftValueInfo {
        fragment_url: url.into(),
        ..info
    });
    set_lookup(&app, vcx, with_info(info, true));
    let note_before = app.read_with(vcx, |app, _| app.connection.status_note().to_string());
    click(vcx, "gift-value-on-fragment");
    let (confirm, note) = app.read_with(vcx, |app, _| {
        (
            app.message_ui.open_link_confirm.clone(),
            app.connection.status_note().to_string(),
        )
    });
    let confirm = confirm.expect("the Open this link? box");
    assert_eq!(confirm.url, url);
    assert!(!confirm.suspicious.is_empty());
    // Nothing was opened yet; the confirm box covers the value box.
    assert_eq!(note, note_before);
    assert!(vcx.debug_bounds("gift-value-box").is_none());
}

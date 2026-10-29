//! Send Location in the real window: the picker opens over the chat with
//! the map, "Send This Location" and the venues; the map pans by drag and
//! zooms by button and wheel; the search narrows the venues; a pick
//! sends and closes; Escape closes; a denied location says so.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, MouseButton, ScrollDelta, ScrollEvent, TestAppContext,
    VisualTestContext, point, px, size,
};
use quill::ids::ChatId;
use quill::location_picker::{
    AddressLine, GeoOutcome, HereState, LocationPicker, LocationSharing, MapView, VenueList,
};
use std::time::Duration;

const OPEN: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn wait(vcx: &mut VisualTestContext, time: Duration) {
    vcx.executor().advance_clock(time);
    frames(vcx);
}

fn find(vcx: &mut VisualTestContext, selector: &str) -> bool {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    vcx.debug_bounds(selector).is_some()
}

fn bounds_of(vcx: &mut VisualTestContext, selector: &str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    vcx.debug_bounds(selector).expect(selector)
}

fn click(vcx: &mut VisualTestContext, selector: &str) {
    let center = bounds_of(vcx, selector).center();
    vcx.simulate_click(center, Modifiers::default());
    frames(vcx);
}

fn dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn picker<T>(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    read: impl FnOnce(&LocationPicker) -> T,
) -> Option<T> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.location.picker())
            .map(read)
    })
}

fn view(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> MapView {
    picker(app, vcx, |picker| picker.view()).expect("the picker is open")
}

fn venue_count(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    picker(app, vcx, |picker| match picker.venues() {
        VenueList::Found(venues) => venues.len(),
        _ => 0,
    })
    .unwrap_or(0)
}

fn open_picker(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(OPEN);
        }
        app.open_location_picker(cx);
    });
    frames(vcx);
    wait(vcx, Duration::from_secs(1));
}

#[gpui_kit::test]
fn the_picker_opens_with_the_map_the_pin_s_place_and_the_venues(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    open_picker(&app, vcx);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::LocationPicker));
    assert!(find(vcx, "location-map"));
    assert!(find(vcx, "location-send"));
    assert!(find(vcx, "location-venues-source"));
    assert_eq!(venue_count(&app, vcx), 6);
    assert!(find(vcx, "location-venue-demo-0"));
    let tiles = picker(&app, vcx, |picker| picker.tiles().len()).unwrap_or(0);
    assert_eq!(tiles, 1, "the map image for the view arrived");
    let address = picker(&app, vcx, |picker| match picker.address_line() {
        AddressLine::Text(text) => text.to_string(),
        _ => String::new(),
    });
    assert_eq!(address.as_deref(), Some("(48.860611, 2.337644)"));
    assert!(find(vcx, "location-address"));
}

#[gpui_kit::test]
fn the_zoom_buttons_and_the_wheel_zoom_and_a_new_image_follows(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    open_picker(&app, vcx);
    let start = view(&app, vcx).zoom();
    click(vcx, "location-zoom-in");
    assert_eq!(view(&app, vcx).zoom(), start + 1);
    wait(vcx, Duration::from_secs(1));
    let zoomed = view(&app, vcx);
    let keys = picker(&app, vcx, |picker| {
        picker
            .tiles()
            .iter()
            .map(|(key, _)| key.zoom)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    assert!(
        keys.contains(&zoomed.zoom()),
        "an image at the new zoom arrived after the map settled: {keys:?}"
    );

    click(vcx, "location-zoom-out");
    assert_eq!(view(&app, vcx).zoom(), start);

    let map = bounds_of(vcx, "location-map").center();
    vcx.simulate_mouse_move(map, None, Modifiers::default());
    vcx.simulate_event(ScrollEvent {
        position: map,
        delta: ScrollDelta::Lines(point(0., 2.)),
        ..Default::default()
    });
    frames(vcx);
    assert_eq!(
        view(&app, vcx).zoom(),
        start + 2,
        "two wheel lines are two steps"
    );
}

#[gpui_kit::test]
fn dragging_moves_the_pin_and_offers_the_places_in_the_area(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    open_picker(&app, vcx);
    let before = view(&app, vcx).center();
    let from = bounds_of(vcx, "location-map").center();
    let none = Modifiers::default();
    vcx.simulate_mouse_move(from, None, none);
    vcx.simulate_mouse_down(from, MouseButton::Left, none);
    for step in 1..=4 {
        let at = point(
            from.x + px(30. * step as f32),
            from.y + px(10. * step as f32),
        );
        vcx.simulate_mouse_move(at, Some(MouseButton::Left), none);
    }
    frames(vcx);
    assert!(picker(&app, vcx, |picker| picker.is_moving()).unwrap_or(false));
    let after = view(&app, vcx).center();
    assert!(
        after.longitude() < before.longitude(),
        "dragged right, the pin went west"
    );
    assert!(
        after.latitude() > before.latitude(),
        "dragged down, the pin went north"
    );
    let release = point(from.x + px(120.), from.y + px(40.));
    vcx.simulate_mouse_up(release, MouseButton::Left, none);
    wait(vcx, Duration::from_secs(1));
    assert!(!picker(&app, vcx, |picker| picker.is_moving()).unwrap_or(true));
    assert!(find(vcx, "location-places-in-area"));

    click(vcx, "location-places-in-area");
    assert!(!find(vcx, "location-places-in-area"));
    assert_eq!(venue_count(&app, vcx), 6, "the venues around the new place");
    wait(vcx, Duration::from_secs(4));
    let address = picker(&app, vcx, |picker| match picker.address_line() {
        AddressLine::Text(text) => text.to_string(),
        _ => String::new(),
    })
    .unwrap_or_default();
    assert_eq!(
        address,
        after.coordinates_label(),
        "the address followed the pin"
    );
}

#[gpui_kit::test]
fn the_search_narrows_the_venues_and_closing_it_brings_them_back(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    open_picker(&app, vcx);
    click(vcx, "location-search-open");
    assert!(find(vcx, "location-search"));
    vcx.simulate_input("pont");
    frames(vcx);
    assert!(
        !find(vcx, "location-map"),
        "the search takes the map's place"
    );
    wait(vcx, Duration::from_secs(1));
    assert_eq!(venue_count(&app, vcx), 1);
    assert!(find(vcx, "location-venue-demo-2"));

    vcx.simulate_input("zzz");
    wait(vcx, Duration::from_secs(1));
    assert!(find(vcx, "location-venues-none"));

    click(vcx, "location-search-close");
    assert!(find(vcx, "location-map"));
    assert_eq!(venue_count(&app, vcx), 6);
}

#[gpui_kit::test]
fn picking_a_venue_closes_the_picker_and_escape_closes_it(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    open_picker(&app, vcx);
    click(vcx, "location-venue-demo-0");
    assert_eq!(dialog(&shell, vcx), None);
    assert!(picker(&app, vcx, |_| ()).is_none());

    open_picker(&app, vcx);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::LocationPicker));
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(dialog(&shell, vcx), None);
    assert!(picker(&app, vcx, |_| ()).is_none());
}

#[gpui_kit::test]
fn a_denied_location_says_so_and_waits_for_a_place(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update_in(vcx, |app, _, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        session.open_chat(OPEN);
        session.location = LocationSharing::default();
        session.location.open(OPEN, None);
        app.location_picker_geolocated(GeoOutcome::Denied, cx);
    });
    frames(vcx);
    assert_eq!(
        picker(&app, vcx, |picker| picker.here()),
        Some(HereState::Denied)
    );
    assert!(find(vcx, "location-here-state"));
    assert!(find(vcx, "location-venues-waiting"));
}

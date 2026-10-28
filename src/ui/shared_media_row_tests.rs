//! Shared-media rows in a real window (needs the `demo-capture` feature for
//! gpui-kit's test support): a click on a Links address goes through the
//! same safety check as a link in a chat, and a Music row plays in place.
use crate::ui::app::QuillApp;
use crate::ui::demos::shared_media::{link_message, tab_messages, text_link_message};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::state::SharedMediaTab;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-shared-media")),
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

fn show_link(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, url: &str) {
    let message = link_message(300, 0, &format!("Sign in: {url}"), url, "Sign in", "");
    app.update_in(vcx, |app, _, cx| {
        app.demo_fill_shared_media(SharedMediaTab::Links, vec![message], cx);
    });
    vcx.run_until_parked();
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not on screen"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn a_look_alike_address_asks_before_opening(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // "аpple.com" with a Cyrillic "а": the link policy asks first.
    let url = "https://\u{0430}pple.com/login";
    show_link(&app, vcx, url);
    click(vcx, "shared-media-link-0");
    let confirm = app
        .read_with(vcx, |app, _| app.message_ui.open_link_confirm.clone())
        .expect("the Open this link? box");
    assert_eq!(confirm.url, url);
    assert!(!confirm.suspicious.is_empty());
}

#[gpui_kit::test]
fn a_label_that_ends_the_address_still_asks(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // The line reads "apple.com" but leads to phish-apple.com.
    let url = "https://phish-apple.com";
    let message = text_link_message(301, "see apple.com", "apple.com", url);
    app.update_in(vcx, |app, _, cx| {
        app.demo_fill_shared_media(SharedMediaTab::Links, vec![message], cx);
    });
    vcx.run_until_parked();
    click(vcx, "shared-media-link-0");
    let confirm = app
        .read_with(vcx, |app, _| app.message_ui.open_link_confirm.clone())
        .expect("the Open this link? box");
    assert_eq!(confirm.url, url);
}

#[gpui_kit::test]
fn a_music_row_plays_in_place_and_a_second_press_cancels(cx: &mut TestAppContext) {
    use crate::ui::play_intent::PlayTarget;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.demo_fill_shared_media(
            SharedMediaTab::Music,
            tab_messages(SharedMediaTab::Music),
            cx,
        );
    });
    vcx.run_until_parked();
    // Not downloaded: the press asks for the file and waits to play it.
    click(vcx, "shared-media-item-220");
    app.read_with(vcx, |app, _| {
        let target = app
            .playback
            .intent
            .as_ref()
            .map(|intent| intent.target.clone());
        assert!(
            matches!(target, Some(PlayTarget::Audio { message, .. }) if message.0 == 220),
            "{target:?}"
        );
    });
    click(vcx, "shared-media-item-220");
    app.read_with(vcx, |app, _| {
        assert!(app.playback.intent.is_none(), "the second press cancels");
        let session = app.session().expect("the demo session");
        let file = app.playback.playing_audio;
        assert_eq!(file, None);
        assert!(
            session.media.downloads.user_count() == 0,
            "nothing downloads"
        );
    });
}

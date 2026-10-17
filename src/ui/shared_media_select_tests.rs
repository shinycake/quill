//! The gallery's multi-select in a real window (needs the `demo-capture`
//! feature for gpui-kit's test support). Fixtures only: nothing here talks to
//! Telegram, and no message is ever deleted.
use crate::ui::app::QuillApp;
use crate::ui::demos::shared_media::photo_message;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ids::MessageId;
use quill::state::{SelectedMedia, SharedMediaTab};
use quill::telegram::envelope::MessageActions;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
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

fn show_photos(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let messages = (0..4).map(|n| photo_message(1004 - n, n, "")).collect();
    app.update_in(vcx, |app, _, cx| {
        app.demo_fill_shared_media(SharedMediaTab::Media, messages, cx);
    });
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
}

fn allowed(forward: bool, delete: bool) -> MessageActions {
    MessageActions {
        can_be_forwarded: forward,
        can_be_deleted_only_for_self: delete,
        ..MessageActions::default()
    }
}

fn tick(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64, actions: MessageActions) {
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session
                .media
                .shared_media
                .toggle_selected(SelectedMedia::from_actions(MessageId(id), Some(&actions)));
        }
        cx.notify();
    });
    vcx.run_until_parked();
}

fn selected(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|s| s.media.shared_media.selected_ids())
            .unwrap_or_default()
            .iter()
            .map(|id| id.0)
            .collect()
    })
}

#[gpui_kit::test]
fn ticked_rows_get_a_header_and_a_click_toggles_instead_of_opening(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_photos(&app, vcx);
    assert!(vcx.debug_bounds("shared-media-selection-bar").is_none());
    tick(&app, vcx, 1004, allowed(true, true));
    assert!(vcx.debug_bounds("shared-media-selection-bar").is_some());
    let tile = vcx
        .debug_bounds("shared-media-tile-1003")
        .expect("a tile is on screen");
    vcx.simulate_click(tile.center(), Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(selected(&app, vcx), vec![1003, 1004]);
    let viewer_open = app.read_with(vcx, |app, _| app.viewer.state.current().is_some());
    assert!(
        !viewer_open,
        "a click while ticking does not open the viewer"
    );
    let tile = vcx.debug_bounds("shared-media-tile-1004").expect("tile");
    vcx.simulate_click(tile.center(), Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(selected(&app, vcx), vec![1003]);
}

#[gpui_kit::test]
fn clear_empties_the_selection_and_hides_the_header(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_photos(&app, vcx);
    tick(&app, vcx, 1004, allowed(true, true));
    tick(&app, vcx, 1002, allowed(false, true));
    let clear = vcx
        .debug_bounds("shared-media-clear")
        .expect("the header has Clear");
    vcx.simulate_click(clear.center(), Modifiers::default());
    vcx.run_until_parked();
    assert!(selected(&app, vcx).is_empty());
    assert!(vcx.debug_bounds("shared-media-selection-bar").is_none());
}

#[gpui_kit::test]
fn the_group_offers_only_what_every_ticked_message_allows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_photos(&app, vcx);
    tick(&app, vcx, 1004, allowed(true, true));
    tick(&app, vcx, 1003, allowed(false, true));
    let (forward, delete, revoke) = app.read_with(vcx, |app, _| {
        let state = app.session().map(|s| &s.media.shared_media);
        (
            state.is_some_and(|s| s.can_forward_selection()),
            state.is_some_and(|s| s.can_delete_selection()),
            state.is_some_and(|s| s.can_revoke_selection()),
        )
    });
    assert!(
        !forward,
        "one protected message blocks Forward for the group"
    );
    assert!(delete);
    assert!(!revoke, "no ticked message can be deleted for everyone");
}

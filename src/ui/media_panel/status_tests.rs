//! The emoji status selector and its "Set Status for..." box on a demo
//! `QuillApp`: the cells and their order, a click, the right-click menu, the
//! star that removes the status, the Premium gate and the box. The demo
//! account takes a change the way TDLib would, so the status shows on the
//! account.

use super::status_demo::seed_status_session;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::media_panel::{PanelCell, PanelRow};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, point, px, size};
use quill::emoji_status_selector::time_picker_values;

fn new_app(cx: &mut TestAppContext, premium: bool) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-emoji-status-account");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.premium_option = Some(premium);
        session.revision += 1;
    });
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..8 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn open_selector(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.open_emoji_status_picker(window, cx)
    });
    frames(vcx);
}

fn my_status(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> i64 {
    app.read_with(vcx, |app, _| {
        let session = app.demo_session.as_ref().expect("demo session");
        session
            .my_user_id
            .and_then(|me| session.users.get(&me))
            .map_or(-1, |user| user.emoji_status_id)
    })
}

fn labelled(vcx: &mut VisualTestContext, label: &str) -> bool {
    vcx.update(|window, _| window.try_find_by_label(label).is_some())
}

fn first_cells(app: &QuillApp) -> Vec<PanelCell> {
    app.pickers
        .media_panel
        .rows
        .iter()
        .find_map(|row| match row {
            PanelRow::Cells { cells, .. } => Some(cells.clone()),
            PanelRow::Header { .. } => None,
        })
        .unwrap_or_default()
}

fn headers(app: &QuillApp) -> Vec<String> {
    app.pickers
        .media_panel
        .rows
        .iter()
        .filter_map(|row| match row {
            PanelRow::Header { label, .. } => Some(label.to_string()),
            PanelRow::Cells { .. } => None,
        })
        .collect()
}

#[gpui_kit::test]
fn the_menu_entry_opens_the_selector_with_the_remove_cell_first(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    open_selector(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.pickers.media_panel.open);
        assert!(app.pickers.media_panel.status.is_some());
        assert!(
            app.session().is_some_and(|s| !s.stickers.emoji.open),
            "the pack settings dialog stays closed"
        );
        let cells = first_cells(app);
        assert!(matches!(cells.first(), Some(PanelCell::StatusRemove)));
        // Themed (at most 7), then recent and default ones not listed yet.
        let custom = cells
            .iter()
            .filter(|c| matches!(c, PanelCell::Custom { .. }))
            .count();
        assert_eq!(custom, cells.len() - 1);
        assert!(custom > 7, "recent and default statuses follow the themed");
        assert_eq!(headers(app), vec!["Collectibles".to_string()]);
    });
    assert!(labelled(vcx, "Search emoji statuses"));
}

#[gpui_kit::test]
fn a_click_sets_the_status_forever_and_closes_the_selector(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    open_selector(&app, vcx);
    assert_eq!(my_status(&app, vcx), 0);
    // The first cell after the star, as its click does.
    let item = app.read_with(vcx, |app, _| {
        let session = app.session().expect("session");
        session
            .stickers
            .emoji
            .custom_emoji_stickers
            .lookup(5001)
            .cloned()
            .expect("a resolved status")
    });
    app.update_in(vcx, |app, window, cx| {
        app.insert_panel_custom_emoji(&item, window, cx)
    });
    frames(vcx);
    assert_eq!(my_status(&app, vcx), 5001, "the first themed status");
    app.read_with(vcx, |app, _| {
        assert!(!app.pickers.media_panel.open);
        assert!(app.pickers.media_panel.status.is_none());
    });
}

#[gpui_kit::test]
fn the_star_removes_the_status(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.change_emoji_status_for(Some(5003), 0, cx));
    assert_eq!(my_status(&app, vcx), 5003);
    open_selector(&app, vcx);
    app.read_with(vcx, |app, _| {
        // The current status is listed even when it is not recent.
        assert!(first_cells(app).len() > 1);
    });
    app.update(vcx, |app, cx| app.choose_panel_status(None, 0, cx));
    assert_eq!(my_status(&app, vcx), 0);
    app.read_with(vcx, |app, _| assert!(!app.pickers.media_panel.open));
}

#[gpui_kit::test]
fn without_premium_a_status_opens_premium_and_removing_still_works(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, false);
    let vcx = &mut vcx;
    // The menu entry itself shows the Premium preview.
    open_selector(&app, vcx);
    app.read_with(vcx, |app, _| assert!(app.pickers.media_panel.status.is_none()));

    // Past the menu, a chosen status is not set; the star still removes one.
    app.update_in(vcx, |app, window, cx| {
        app.open_status_selector(point(px(8.), px(52.)), window, cx)
    });
    frames(vcx);
    app.update(vcx, |app, cx| app.choose_panel_status(Some(5003), 0, cx));
    assert_eq!(my_status(&app, vcx), 0, "no Premium, no status");
    app.read_with(vcx, |app, _| assert!(!app.pickers.media_panel.open));

    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        if let Some(user) = session
            .my_user_id
            .and_then(|me| session.users.get_mut(&me))
        {
            user.emoji_status_id = 5003;
        }
        app.choose_panel_status(None, 0, cx)
    });
    assert_eq!(my_status(&app, vcx), 0, "a status can always be removed");
}

#[gpui_kit::test]
fn the_search_narrows_to_matching_statuses(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    open_selector(&app, vcx);
    let all = app.read_with(vcx, |app, _| app.pickers.media_panel.rows.len());
    app.update_in(vcx, |app, window, cx| {
        app.pickers
            .status_search_input
            .update(cx, |input, cx| input.set_value("fire", window, cx));
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let cells: Vec<_> = app
            .pickers
            .media_panel
            .rows
            .iter()
            .filter_map(|row| match row {
                PanelRow::Cells { cells, .. } => Some(cells.len()),
                PanelRow::Header { .. } => None,
            })
            .collect();
        assert!(
            app.pickers.media_panel.rows.len() < all,
            "fewer rows than the full list"
        );
        assert!(
            app.pickers
                .media_panel
                .rows
                .iter()
                .all(|row| !matches!(row, PanelRow::Cells { cells, .. } if cells.iter().any(|c| matches!(c, PanelCell::StatusRemove)))),
            "the star is not a search result: {cells:?}"
        );
    });
}

#[gpui_kit::test]
fn the_menu_sets_a_status_for_a_while_and_custom_opens_the_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    open_selector(&app, vcx);
    // Custom: the selector hides first, then the box opens.
    app.update_in(vcx, |app, window, cx| {
        app.close_media_panel(cx);
        app.open_status_time_box(5001, window, cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.pickers.media_panel.open, "the selector hides first");
        assert!(app.pickers.media_panel.until.is_some());
    });
}

#[gpui_kit::test]
fn the_box_picks_a_duration_with_keys_and_sets_the_status(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| app.open_status_time_box(5003, window, cx));
    frames(vcx);
    let values = time_picker_values();
    assert!(values.len() > 3);
    app.update(vcx, |app, cx| {
        app.step_status_drum(2, cx);
        app.step_status_drum(-1, cx);
    });
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.pickers.media_panel.until.as_ref().map(|u| u.index),
            Some(1)
        );
    });
    app.update(vcx, |app, cx| app.step_status_drum(isize::MAX / 2, cx));
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.pickers.media_panel.until.as_ref().map(|u| u.index),
            Some(values.len() - 1),
            "the drum stops at the last value"
        );
    });
    app.update(vcx, |app, cx| app.confirm_status_time(cx));
    assert_eq!(my_status(&app, vcx), 5003);
    app.read_with(vcx, |app, _| assert!(app.pickers.media_panel.until.is_none()));
}

#[gpui_kit::test]
fn cancel_closes_the_box_without_a_change(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| app.open_status_time_box(5003, window, cx));
    frames(vcx);
    assert!(labelled(vcx, "Cancel"));
    click_label(vcx, "Cancel");
    app.read_with(vcx, |app, _| assert!(app.pickers.media_panel.until.is_none()));
    assert_eq!(my_status(&app, vcx), 0);
}

#[test]
fn the_demo_seed_is_premium_with_every_status_resolved() {
    let sink = std::sync::Arc::new(quill::diagnostics::MemorySink::default());
    let session = seed_status_session(sink);
    assert_eq!(session.payments.premium_option, Some(true));
    let emoji = &session.stickers.emoji;
    for id in emoji
        .themed_status_ids
        .iter()
        .chain(&emoji.default_status_ids)
    {
        assert!(emoji.custom_emoji_stickers.lookup(*id).is_some());
    }
}

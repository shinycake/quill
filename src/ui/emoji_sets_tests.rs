//! UI tests on the `ready-emoji-packs` demo: the box lists the packs with
//! previews and nothing to press to load, search or retry; tabs, search as
//! you type, remove with confirmation and opening a pack.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::component::input::InputEvent;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::emoji::EmojiSetTab;
use std::time::Duration;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-emoji-packs");
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    vcx.executor().advance_clock(Duration::from_millis(600));
    frames(&mut vcx);
    (app, shell, vcx)
}

fn has(vcx: &mut VisualTestContext, id: String) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn tab(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> EmojiSetTab {
    app.read_with(vcx, |app, _| {
        app.demo_session.as_ref().expect("demo").stickers.emoji.tab
    })
}

#[gpui_kit::test]
fn the_box_lists_installed_packs_without_manual_controls(cx: &mut TestAppContext) {
    let (_app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::EmojiSets)));
    for id in 1..=3 {
        assert!(has(vcx, format!("emoji-pack-open-{id}")));
        assert!(has(vcx, format!("emoji-pack-remove-{id}")));
        assert!(!has(vcx, format!("emoji-pack-add-{id}")));
    }
    assert!(!has(vcx, "emoji-pack-search".into()));
    assert!(!has(vcx, "download-emoji-pack".into()));
    assert!(!has(vcx, "emoji-packs-more".into()));
    assert!(!has(vcx, "dynamic-emoji-pack-order".into()));
}

#[gpui_kit::test]
fn the_trending_tab_offers_add_for_packs_not_yet_added(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.select_emoji_sets_tab(EmojiSetTab::Trending, window, cx)
        })
    });
    frames(vcx);
    assert_eq!(tab(&app, vcx), EmojiSetTab::Trending);
    assert!(has(vcx, "emoji-pack-add-4".into()));
    assert!(has(vcx, "emoji-pack-add-5".into()));
    assert!(!has(vcx, "emoji-pack-add-2".into()));
    assert!(!has(vcx, "emoji-pack-remove-4".into()));
}

#[gpui_kit::test]
fn typing_searches_after_a_pause_and_clearing_goes_back(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let input = app.read_with(vcx, |app, _| app.pickers.emoji_set_search_input.clone());
    vcx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_value("trav", window, cx);
            cx.emit(InputEvent::Change);
        })
    });
    frames(vcx);
    assert_eq!(tab(&app, vcx), EmojiSetTab::Installed, "waits for a pause");
    vcx.executor().advance_clock(Duration::from_millis(400));
    frames(vcx);
    assert_eq!(tab(&app, vcx), EmojiSetTab::Search);
    assert!(has(vcx, "emoji-pack-open-4".into()));
    assert!(!has(vcx, "emoji-pack-open-1".into()));
    vcx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            cx.emit(InputEvent::Change);
        })
    });
    frames(vcx);
    assert_eq!(tab(&app, vcx), EmojiSetTab::Installed);
    assert!(has(vcx, "emoji-pack-open-1".into()));
}

#[gpui_kit::test]
fn remove_asks_before_removing_and_a_row_opens_the_pack(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| window.click("emoji-pack-open-2".to_string(), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu_ui.sticker_set_open));
    vcx.update(|window, cx| window.click("emoji-pack-remove-1".to_string(), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.admin.group_confirm_dialog.is_some()));
}

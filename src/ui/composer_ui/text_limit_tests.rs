use super::{TextLimitKey, TextLimitMemo, TextLimitNote};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use std::cell::Cell;

const LIMIT: i32 = 4096;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats-composer");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn fill(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str) {
    app.update_in(vcx, |app, window, cx| {
        app.set_composer_markup(text, window, cx);
        app.composer.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(text.len()..text.len(), cx);
        });
    });
    frames(vcx);
}

fn note(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, editing: bool) -> TextLimitNote {
    app.update(vcx, |app, cx| app.text_limit_note(editing, cx))
}

fn shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, cx| app.text_limit_bar(cx).is_some())
}

#[gpui_kit::test]
fn the_line_follows_the_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert!(!shown(&app, vcx));

    fill(&app, vcx, &"x".repeat(LIMIT as usize));
    assert!(!shown(&app, vcx));
    vcx.update(|window, cx| window.input("y", cx));
    frames(vcx);
    assert!(shown(&app, vcx));
    assert_eq!(note(&app, vcx, false), TextLimitNote::Parts(2));
    assert_eq!(note(&app, vcx, true), TextLimitNote::UnitsOver(1));
    vcx.update(|window, cx| window.press("secondary-z", cx));
    frames(vcx);
    assert!(!shown(&app, vcx));

    let text = format!("**{}**", "x".repeat(LIMIT as usize));
    fill(&app, vcx, &text);
    assert_eq!(note(&app, vcx, false), TextLimitNote::Fits);
}

#[gpui_kit::test]
fn a_big_paste_counts_as_it_will_be_sent(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let text = "12:00:01 [INFO] [worker-3] a request came in\n".repeat(1_000);
    let parts = quill::text_split::split_markup_text(&text, LIMIT).len();
    assert!(parts > 5);
    fill(&app, vcx, &text);
    assert_eq!(note(&app, vcx, false), TextLimitNote::Parts(parts));
}

#[gpui_kit::test]
fn a_note_is_counted_once_per_revision(cx: &mut TestAppContext) {
    let key = TextLimitKey {
        field: cx.new(|_| ()).entity_id(),
        revision: 1,
        rich_editor: false,
        limit: LIMIT,
        editing: false,
    };
    let memo = TextLimitMemo::default();
    let runs = Cell::new(0);
    let count = || {
        runs.set(runs.get() + 1);
        TextLimitNote::Parts(runs.get())
    };
    assert_eq!(memo.get(key, count), TextLimitNote::Parts(1));
    assert_eq!(memo.get(key, count), TextLimitNote::Parts(1));
    let moved = [
        TextLimitKey { revision: 2, ..key },
        TextLimitKey {
            rich_editor: true,
            ..key
        },
        TextLimitKey { limit: 1024, ..key },
        TextLimitKey {
            editing: true,
            ..key
        },
    ];
    for (ix, key) in moved.into_iter().enumerate() {
        assert_eq!(memo.get(key, count), TextLimitNote::Parts(ix + 2));
    }
    assert_eq!(runs.get(), 5);
}

#[test]
fn a_note_reads_the_text_as_a_send_does() {
    assert_eq!(
        TextLimitNote::of("  hello  ", 5, false),
        TextLimitNote::Fits
    );
    assert_eq!(
        TextLimitNote::of("**hello**", 5, false),
        TextLimitNote::Fits
    );
    assert_eq!(
        TextLimitNote::of("hello you", 5, true),
        TextLimitNote::UnitsOver(4)
    );
    assert_eq!(
        TextLimitNote::of("hello you", 5, false),
        TextLimitNote::Parts(2)
    );
}

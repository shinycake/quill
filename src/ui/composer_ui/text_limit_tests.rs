use super::TextLimitNote;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

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
    app.update(vcx, |app, cx| app.text_limit_note(editing, cx));
    vcx.run_until_parked();
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
    vcx.update(|window, cx| window.press("secondary-z", cx));
    frames(vcx);
    assert!(!shown(&app, vcx));

    let text = format!("**{}**", "x".repeat(LIMIT as usize));
    fill(&app, vcx, &text);
    assert_eq!(note(&app, vcx, false), TextLimitNote::Fits);
}

#[gpui_kit::test]
fn a_long_text_shows_no_count_until_its_own_count_returns(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let text = "12:00:01 [INFO] [worker-3] a request came in\n".repeat(1_000);
    fill(&app, vcx, &text);
    assert!(matches!(note(&app, vcx, false), TextLimitNote::Parts(_)));
    let count = quill::text_split::split_markup_text(&format!("{text}y"), LIMIT).len();
    app.update_in(vcx, |app, window, cx| {
        let end = app.composer.read(cx).text().len();
        app.replace_composer_text(end..end, "y", window, cx);
        assert_eq!(app.text_limit_note(false, cx), TextLimitNote::Counting);
    });
    assert_eq!(note(&app, vcx, false), TextLimitNote::Parts(count));
}

#[gpui_kit::test]
fn another_text_in_the_field_never_shows_the_last_count(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let first = "12:00:01 [INFO] [worker-3] a request came in\n".repeat(1_000);
    fill(&app, vcx, &first);
    let first_count = note(&app, vcx, false);
    let second = "another chat holds another draft\n".repeat(3_000);
    let second_count = quill::text_split::split_markup_text(&second, LIMIT).len();
    app.update_in(vcx, |app, window, cx| {
        app.set_composer_markup(&second, window, cx);
        assert_eq!(app.text_limit_note(false, cx), TextLimitNote::Counting);
    });
    assert_ne!(first_count, TextLimitNote::Parts(second_count));
    assert_eq!(note(&app, vcx, false), TextLimitNote::Parts(second_count));
}

#[gpui_kit::test]
fn a_text_near_the_limit_is_counted_in_the_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let text = "x".repeat(LIMIT as usize + 1);
    fill(&app, vcx, &text);
    app.update_in(vcx, |app, window, cx| {
        app.set_composer_markup(&format!("{text}y"), window, cx);
        assert_eq!(app.text_limit_note(false, cx), TextLimitNote::Parts(2));
    });
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

#[test]
fn a_note_reads_the_text_as_a_send_does() {
    assert_eq!(
        TextLimitNote::of("hello!", 5, true),
        TextLimitNote::UnitsOver(1)
    );
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

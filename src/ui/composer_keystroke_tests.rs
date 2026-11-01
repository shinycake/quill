//! What one typed character leaves behind in the composer: the text, the
//! remembered previous text, the stored draft, and a redraw of the composer
//! slice. Over the `ready-chats` demo; nothing reaches Telegram.
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

const OPEN: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    });
    vcx.run_until_parked();
    (app, vcx)
}

fn focus_composer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
}

fn composer_text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string())
}

#[gpui_kit::test]
fn a_keystroke_leaves_the_text_the_previous_text_and_the_draft_in_step(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_composer(&app, vcx);
    for (typed, expected) in [("h", "h"), ("i", "hi"), (" ", "hi "), ("there", "hi there")] {
        vcx.simulate_input(typed);
        assert_eq!(composer_text(&app, vcx), expected);
        let (prev, draft) = app.read_with(vcx, |app, _| {
            (
                app.composer_ui.prev_text.clone(),
                app.session()
                    .and_then(|session| session.draft_for(OPEN, Default::default()))
                    .map(|draft| draft.text.clone()),
            )
        });
        assert_eq!(prev, expected, "the previous text follows the field");
        assert_eq!(
            draft.as_deref(),
            Some(expected),
            "the draft follows the field"
        );
    }
}

#[gpui_kit::test]
fn a_keystroke_redraws_the_composer_slice(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_composer(&app, vcx);
    let _ = crate::ui::frame_clock::take_slice_renders();
    vcx.simulate_input("a");
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let renders = crate::ui::frame_clock::take_slice_renders();
    assert!(renders.contains_key("composer"), "{renders:?}");
}

#[gpui_kit::test]
fn an_emoji_keystroke_keeps_the_text_and_the_previous_text_in_step(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    focus_composer(&app, vcx);
    vcx.simulate_input("ok \u{1f600}");
    assert_eq!(composer_text(&app, vcx), "ok \u{1f600}");
    let prev = app.read_with(vcx, |app, _| app.composer_ui.prev_text.clone());
    assert_eq!(prev, "ok \u{1f600}");
}

/// A finished background spell check redraws the composer slice, where
/// the underlines are, not the whole app.
#[gpui_kit::test]
fn a_spell_check_result_redraws_only_the_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        app.chat_prefs.spellcheck_enabled = true;
        app.spell.checker = std::sync::Arc::new(quill::spellcheck::SpellChecker::wordlist());
    });
    focus_composer(&app, vcx);
    vcx.simulate_input("qzxvb");
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let underlined = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| !app.spell.misspellings.is_empty())
    };
    assert!(!underlined(&app, vcx), "the check waits for the pause");
    let _ = crate::ui::frame_clock::take_slice_renders();
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    vcx.run_until_parked();
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(underlined(&app, vcx));
    let renders = crate::ui::frame_clock::take_slice_renders();
    assert!(renders.contains_key("composer"), "{renders:?}");
    assert!(!renders.contains_key("(app notified)"), "{renders:?}");
}

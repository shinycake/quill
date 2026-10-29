//! Words with emoji in the composer: which light up, the strip a hovered
//! one offers, Esc, the setting. Over the `ready-chats` demo with the
//! keyword answers put in the session by hand; nothing reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::RequestId;
use quill::telegram::envelope_emoji::EmojiKeyword;

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
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    answer(
        &app,
        &mut vcx,
        &[
            ("pizza", &["🍕"]),
            ("cake", &["🍰", "🎂"]),
            ("party", &["🎉", "🥳", "🎊"]),
        ],
    );
    (app, vcx)
}

/// TDLib answering `searchEmojis` for each word.
fn answer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, words: &[(&str, &[&str])]) {
    app.update(vcx, |app, _| {
        let lookups = &mut app
            .demo_session
            .as_mut()
            .expect("demo session")
            .stickers
            .emoji
            .keyword_lookups;
        for (word, emoji) in words {
            let id = RequestId(800_000 + lookups.generation);
            lookups.sent(id, word.to_string());
            lookups.answered(
                id,
                emoji
                    .iter()
                    .map(|e| EmojiKeyword {
                        emoji: e.to_string(),
                        keyword: word.to_string(),
                    })
                    .collect(),
            );
        }
    });
}

fn frame(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
}

fn lit(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<String> {
    app.read_with(vcx, |app, _| {
        app.composer_ui
            .emoji_words
            .highlights
            .iter()
            .map(|h| h.normalized.clone())
            .collect()
    })
}

fn text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string())
}

#[gpui_kit::test]
fn finished_words_light_up_and_the_word_being_typed_waits(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("we got pizza");
    frame(vcx);
    assert!(lit(&app, vcx).is_empty(), "the caret is still on `pizza`");
    vcx.simulate_input(" and cake for the party later tonight with all our friends ");
    frame(vcx);
    // 13 words: two highlights, the most specific words.
    assert_eq!(lit(&app, vcx), ["pizza", "cake"]);
}

#[gpui_kit::test]
fn a_hovered_word_offers_its_emoji_and_becomes_one(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("we got pizza and cake for the party later tonight with friends ");
    frame(vcx);
    app.update(vcx, |app, _| app.hover_highlight(1));
    let glyphs: Vec<String> = app.read_with(vcx, |app, _| {
        let (_, items) = app
            .composer_ui
            .emoji_words
            .hovered
            .clone()
            .expect("hovered");
        items.into_iter().map(|i| i.glyph).collect()
    });
    assert_eq!(glyphs[..2], ["🍰", "🎂"]);
    vcx.update(|window, cx| app.update(cx, |app, cx| app.pick_word_emoji(1, window, cx)));
    assert_eq!(
        text(&app, vcx),
        "we got pizza and 🎂 for the party later tonight with friends "
    );
    // The caret stays at the end, where the user was typing.
    let (caret, len) = app.read_with(vcx, |app, cx| {
        let composer = app.composer.read(cx);
        (composer.selected_range(), composer.value().len())
    });
    assert_eq!(caret, len..len);
}

#[gpui_kit::test]
fn a_selection_darkens_the_words_it_covers(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("we got pizza and cake for the party later tonight with friends ");
    frame(vcx);
    assert_eq!(lit(&app, vcx), ["pizza", "cake"]);
    // Select "pizza and cake": both may be about to change.
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.composer
                .update(cx, |input, cx| input.set_selected_range(7..21, cx));
        })
    });
    frame(vcx);
    let now = lit(&app, vcx);
    assert!(!now.contains(&"pizza".to_string()));
    assert!(!now.contains(&"cake".to_string()));
}

#[gpui_kit::test]
fn escape_closes_the_colon_strip_before_a_hovered_word(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("we got pizza and cake for the party later tonight with friends ");
    frame(vcx);
    app.update(vcx, |app, _| app.hover_highlight(0));
    vcx.simulate_input(":heart");
    frame(vcx);
    let open = app.read_with(vcx, |app, _| app.composer_ui.suggest.active.is_some());
    assert!(open, "the `:` strip is showing");
    vcx.simulate_keystrokes("escape");
    frame(vcx);
    let open = app.read_with(vcx, |app, _| app.composer_ui.suggest.active.is_some());
    assert!(!open);
    assert!(lit(&app, vcx).contains(&"pizza".to_string()));
}

#[gpui_kit::test]
fn escape_stops_highlighting_that_word(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("we got pizza and cake for the party later tonight with friends ");
    frame(vcx);
    app.update(vcx, |app, _| app.hover_highlight(0));
    vcx.simulate_keystrokes("escape");
    frame(vcx);
    let now = lit(&app, vcx);
    assert!(!now.contains(&"pizza".to_string()));
    assert!(now.contains(&"cake".to_string()));
}

#[gpui_kit::test]
fn the_setting_turns_the_highlights_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.simulate_input("we got pizza and cake for the party later tonight with friends ");
    frame(vcx);
    assert!(!lit(&app, vcx).is_empty());
    app.update(vcx, |app, _| app.chat_prefs.highlight_emoji_words = false);
    frame(vcx);
    assert!(lit(&app, vcx).is_empty());
    // "Suggest emoji replacements" off turns them off too.
    app.update(vcx, |app, _| {
        app.chat_prefs.highlight_emoji_words = true;
        app.chat_prefs.suggest_emoji = false;
    });
    frame(vcx);
    assert!(lit(&app, vcx).is_empty());
}

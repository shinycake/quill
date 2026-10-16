//! The platform spell engine is built on a background task and swapped in
//! when ready: starting a build never waits for it, and the wordlist keeps
//! answering until then.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::spellcheck_ui::SpellInfo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::spell_dict::EngineKind;
use quill::spellcheck::SpellChecker;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(900.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| crate::ui::shell::QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app built");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

/// A backend whose init takes a while, like a cold COM or spell-server start.
fn slow_engine(chosen: &[String]) -> (Arc<SpellChecker>, SpellInfo) {
    std::thread::sleep(Duration::from_millis(300));
    (
        Arc::new(SpellChecker::wordlist()),
        SpellInfo {
            kind: EngineKind::System,
            available: vec!["xx".into()],
            active: vec!["xx".into()],
            chosen: chosen.to_vec(),
        },
    )
}

#[gpui_kit::test]
fn slow_engine_build_does_not_block_and_swaps_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let old = app.update_in(&mut vcx, |app, _, _| app.spell.checker.clone());
    let started = Instant::now();
    app.update_in(&mut vcx, |app, _, cx| {
        app.spell.engine_builder = slow_engine;
        app.set_spell_languages(vec!["xx".into()], cx);
    });
    // Starting the build returned at once, with the old engine in place.
    assert!(started.elapsed() < Duration::from_millis(250));
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.spell.engine_task.is_some());
        assert!(Arc::ptr_eq(&app.spell.checker, &old));
        assert_eq!(app.spell.info.chosen, vec!["xx".to_string()]);
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.spell.engine_task.is_none());
        assert!(!Arc::ptr_eq(&app.spell.checker, &old));
        assert_eq!(app.spell.info.active, vec!["xx".to_string()]);
    });
}

#[gpui_kit::test]
fn words_learned_while_loading_survive_the_swap(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.spell.checker.set_app_words(["zorbleflux".to_string()]);
        app.spell.engine_builder = slow_engine;
        app.start_spell_engine(Vec::new(), cx);
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.spell.checker.is_correct("zorbleflux"));
    });
}

#[test]
fn provisional_checker_is_the_wordlist_with_no_dictionary_calls() {
    let (checker, info, _) = QuillApp::provisional_spellchecker();
    assert!(checker.is_correct("hello"));
    assert!(info.active.is_empty() && info.available.is_empty());
}

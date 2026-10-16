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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

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

fn engine_for(chosen: &[String]) -> (Arc<SpellChecker>, SpellInfo) {
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

/// A backend that builds at once.
fn fast_engine(chosen: &[String]) -> (Arc<SpellChecker>, SpellInfo) {
    engine_for(chosen)
}

/// Opens the gate a gated build waits on, like a cold COM or spell-server
/// start finishing. No clock involved: the test decides when it ends.
static GATE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
static BUILD_FINISHED: AtomicBool = AtomicBool::new(false);

fn open_gate() {
    *GATE.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
    GATE.1.notify_all();
}

/// A backend that stays busy until `open_gate`. The 30 s cap only keeps a
/// broken test from hanging forever; it is not an assertion.
fn gated_engine(chosen: &[String]) -> (Arc<SpellChecker>, SpellInfo) {
    let open = GATE.0.lock().unwrap_or_else(|e| e.into_inner());
    let _released = GATE
        .1
        .wait_timeout_while(open, Duration::from_secs(30), |open| !*open)
        .unwrap_or_else(|e| e.into_inner());
    BUILD_FINISHED.store(true, Ordering::SeqCst);
    engine_for(chosen)
}

#[gpui_kit::test]
fn slow_engine_build_does_not_block_and_swaps_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let old = app.update_in(&mut vcx, |app, _, _| app.spell.checker.clone());
    app.update_in(&mut vcx, |app, _, cx| {
        app.spell.engine_builder = gated_engine;
        app.set_spell_languages(vec!["xx".into()], cx);
    });
    // Starting the build returned while the build is still blocked on the
    // gate, with the old engine in place.
    assert!(!BUILD_FINISHED.load(Ordering::SeqCst));
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.spell.engine_task.is_some());
        assert!(Arc::ptr_eq(&app.spell.checker, &old));
        assert_eq!(app.spell.info.chosen, vec!["xx".to_string()]);
    });
    // Releasing the build lets the swap happen, and only then.
    open_gate();
    vcx.run_until_parked();
    assert!(BUILD_FINISHED.load(Ordering::SeqCst));
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
        app.spell.engine_builder = fast_engine;
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

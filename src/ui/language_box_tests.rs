//! Settings → Language in the UI (needs the `demo-capture` feature for
//! gpui-kit's test support): the Settings row opens the box, picking a
//! pack translates the interface, and English comes back. The strings stay
//! on this test's thread (`quill::lang::isolate_thread`), and demos write
//! no files.
use crate::ui::app::QuillApp;
use crate::ui::navigation::NavigationAction;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::lang::{keys, tr};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    quill::lang::isolate_thread();
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn pack(id: &str) -> quill::telegram::envelope::LanguagePackInfoData {
    crate::ui::language_demo::demo_packs()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap()
}

#[gpui_kit::test]
fn settings_row_opens_the_box_and_a_pick_translates(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(tr(keys::MENU_SETTINGS), "Settings");
    app.update_in(vcx, |app, window, cx| {
        app.navigate(NavigationAction::Language, window, cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| assert!(app.language.open.is_some()));

    app.update_in(vcx, |app, _, cx| app.choose_language(pack("pt-br"), cx));
    vcx.run_until_parked();
    assert_eq!(tr(keys::MENU_SETTINGS), "Configurações");
    assert_eq!(quill::lang::current_pack_id(), "pt-br");
    // A key the pack lacks falls back to English.
    assert_eq!(tr(keys::SETTINGS_INFORMATION), "Edit profile");
    // The box stays open over the switch, like tdesktop's.
    app.read_with(vcx, |app, _| assert!(app.language.open.is_some()));

    app.update_in(vcx, |app, _, cx| app.choose_language(pack("en"), cx));
    vcx.run_until_parked();
    assert_eq!(tr(keys::MENU_SETTINGS), "Settings");
    assert_eq!(quill::lang::current_pack_id(), "en");
}

/// Round 8, 2g: an account switch leaves no page, and the language box
/// is app-wide. The next arrival on the page makes a fresh box, which is
/// what asks the account shown for its packs, and drops the old search.
#[gpui_kit::test]
fn arriving_on_the_page_again_makes_a_fresh_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.navigate(NavigationAction::Language, window, cx);
    });
    vcx.run_until_parked();
    let first = app.update_in(vcx, |app, window, cx| {
        let open = app.language.open.as_ref().expect("the box");
        open.search
            .update(cx, |input, cx| input.set_value("port", window, cx));
        open.search.entity_id()
    });
    // A fresh Settings state for the next account: no page.
    app.update_in(vcx, |app, window, cx| {
        app.settings.nav = crate::ui::settings_window::SettingsNav::List;
        app.open_language_box(window, cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, cx| {
        let open = app.language.open.as_ref().expect("a box");
        assert_ne!(open.search.entity_id(), first, "a fresh box");
        assert_eq!(open.search.read(cx).value(), "");
    });
}

fn recent_pack(id: &str) -> quill::lang::prefs::LanguageChoice {
    quill::lang::prefs::LanguageChoice {
        pack_id: id.into(),
        base_pack_id: "en".into(),
        plural_code: "en".into(),
        is_rtl: false,
        name: id.into(),
        native_name: id.into(),
    }
}

/// 10-155: a recent pack deleted in the box leaves the list at once, so a
/// later pick that rewrites the list never writes it back; Restore puts it
/// back where it was.
#[gpui_kit::test]
fn a_removed_recent_pack_stays_out_of_every_save(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.language.recent = vec![recent_pack("xa"), recent_pack("xb")];
        app.navigate(NavigationAction::Language, window, cx);
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        app.remove_recent_language("xa", false, cx)
    });
    app.read_with(vcx, |app, _| {
        let ids: Vec<&str> = app
            .language
            .recent
            .iter()
            .map(|c| c.pack_id.as_str())
            .collect();
        assert_eq!(ids, ["xb"], "the list a save writes has lost it");
    });
    // Another pick rewrites the recent list: the removed pack stays out.
    app.update_in(vcx, |app, _, cx| app.choose_language(pack("pt-br"), cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.language.recent.iter().all(|c| c.pack_id != "xa"));
        let shown = app.shown_recent_languages();
        assert!(
            shown.iter().any(|c| c.pack_id == "xa"),
            "still listed, dimmed, for Restore"
        );
    });
    app.update_in(vcx, |app, _, cx| app.remove_recent_language("xa", true, cx));
    app.read_with(vcx, |app, _| {
        assert!(app.language.recent.iter().any(|c| c.pack_id == "xa"));
    });
}

/// A recent pack deleted and then picked again is a kept row again: its
/// Restore must not put a second copy beside it.
#[gpui_kit::test]
fn a_repicked_removed_pack_is_not_restored_twice(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.language.recent = vec![recent_pack("xa"), recent_pack("xb")];
        app.navigate(NavigationAction::Language, window, cx);
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        app.remove_recent_language("xa", false, cx);
        let mut info = pack("pt-br");
        info.id = "xa".into();
        info.is_official = false;
        app.install_language(
            quill::state::LanguageReady {
                info,
                pack: Vec::new(),
                base: Vec::new(),
            },
            cx,
        );
        // Restore asked for the same pack must find nothing to restore.
        app.remove_recent_language("xa", true, cx);
    });
    app.read_with(vcx, |app, _| {
        let xa = app
            .language
            .recent
            .iter()
            .filter(|c| c.pack_id == "xa")
            .count();
        assert_eq!(xa, 1, "one row for the pack");
        let shown = app
            .shown_recent_languages()
            .iter()
            .filter(|c| c.pack_id == "xa")
            .count();
        assert_eq!(shown, 1);
    });
}

/// Quitting waits for the writes still out: both of two writes queued for a
/// file are taken, and awaiting them runs both, in the order asked.
#[gpui_kit::test]
fn quitting_flushes_the_writes_still_out(cx: &mut TestAppContext) {
    use crate::ui::language_state::LanguageFile;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let pending = app.update(vcx, |app, cx| {
        for name in ["A", "B"] {
            let written = written.clone();
            app.language
                .writes
                .spawn(LanguageFile::Choice, cx, move || {
                    quill::locks::lock_or_recover(&written).push(name);
                });
        }
        app.language.writes.take_pending()
    });
    assert_eq!(pending.len(), 1, "one chain per file");
    let done = app.update(vcx, |_, cx| {
        cx.spawn(async move |_, _| {
            for task in pending {
                task.await;
            }
        })
    });
    vcx.run_until_parked();
    drop(done);
    assert_eq!(*quill::locks::lock_or_recover(&written), vec!["A", "B"]);
}

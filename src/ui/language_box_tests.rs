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

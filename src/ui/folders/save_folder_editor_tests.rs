#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The folder editor's Save waits for the folder to load: saving the blank
//! form would send an empty spec over the real folder (sweep 09-104).

use crate::ui::app::QuillApp;
use crate::ui::dialogs::FolderEditorDialog;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, TestAppContext, px, size};

#[gpui_kit::test]
fn save_does_nothing_while_the_folder_is_loading(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1000.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = gpui_kit::VisualTestContext::from_window(handle.into(), cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let dialog = FolderEditorDialog::new(window, cx, Some(5));
        assert!(dialog.fetch_pending, "an existing folder is fetched first");
        app.folders.editor = Some(dialog);
        app.save_folder_editor(cx);
        let dialog = app.folders.editor.as_ref().expect("still open");
        assert!(
            dialog.error.is_none(),
            "the blank form was not even validated, let alone saved"
        );
    });
}

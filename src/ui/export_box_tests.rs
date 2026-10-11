//! The export box opens, edits its draft and renders every view without
//! panicking (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::export_box::DateField;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::export::{ExportFormat, MediaKind};
use quill::ids::ChatId;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

#[gpui_kit::test]
fn the_box_edits_its_draft_and_shows_both_views(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.start_chat_export(ChatId(1), cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let draft = app.dialogs.chat_export_dialog.as_ref().expect("box open");
        assert_eq!(draft.settings.format, ExportFormat::Html);
        assert!(draft.settings.media.contains(MediaKind::Photo));
    });
    // Switch the calendar on for each end of the range and pick a day.
    for field in [DateField::From, DateField::Till] {
        app.update_in(vcx, |app, _, cx| {
            let draft = app.dialogs.chat_export_dialog.as_mut().unwrap();
            draft.picking = Some(field);
            cx.notify();
        });
        vcx.run_until_parked();
    }
    app.update_in(vcx, |app, _, cx| {
        let draft = app.dialogs.chat_export_dialog.as_mut().unwrap();
        draft.picking = None;
        draft.settings.set_from_day(19_700);
        draft.settings.set_till_day(19_710);
        draft.settings.media = draft.settings.media.with(MediaKind::File, true);
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let settings = &app.dialogs.chat_export_dialog.as_ref().unwrap().settings;
        assert!(settings.from > 0 && settings.from < settings.till);
        assert!(settings.media.contains(MediaKind::File));
    });
}

#[gpui_kit::test]
fn closing_the_box_drops_the_draft(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.start_chat_export(ChatId(1), cx));
    app.update_in(vcx, |app, _, cx| {
        app.dialogs.chat_export_dialog = None;
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.chat_export_dialog.is_none());
    });
}

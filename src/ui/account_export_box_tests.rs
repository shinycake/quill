//! The "Export Telegram data" box opens, edits its draft, and renders both
//! the options and a running export's progress without panicking (needs
//! the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::export::account::{AccountSettings, Section, Sections};
use quill::export::{ExportFormat, MediaKind};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
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
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

#[gpui_kit::test]
fn the_box_opens_with_tdesktops_defaults_and_edits_its_draft(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.open_account_export(cx);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::AccountExport));
    });
    app.read_with(vcx, |app, _| {
        let draft = app
            .dialogs
            .account_export_dialog
            .as_ref()
            .expect("box open");
        assert_eq!(draft.settings.format, ExportFormat::Html);
        assert!(draft.settings.is_on(Section::PersonalInfo));
        assert!(draft.settings.only_my(Section::PrivateGroups));
    });
    // Every chat type on (so every "Only my messages" row renders), media
    // off, then nothing at all: each view must render.
    app.update_in(vcx, |app, _, cx| {
        let draft = app.dialogs.account_export_dialog.as_mut().unwrap();
        for section in Section::CHATS {
            draft.settings.set(section, true);
        }
        draft.settings.set_only_my(Section::PrivateGroups, false);
        draft.settings.media = draft.settings.media.with(MediaKind::File, true);
        draft.settings.format = ExportFormat::HtmlAndJson;
        cx.notify();
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        let draft = app.dialogs.account_export_dialog.as_mut().unwrap();
        draft.settings.sections = Sections::none();
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let settings = &app.dialogs.account_export_dialog.as_ref().unwrap().settings;
        assert!(!settings.can_start());
        assert!(!settings.shows_media());
        assert!(!settings.only_my(Section::PrivateGroups));
    });
    // Starting without a connection keeps the box and says why.
    app.update_in(vcx, |app, _, cx| {
        let draft = app.dialogs.account_export_dialog.as_mut().unwrap();
        draft.settings.set(Section::Sessions, true);
        app.run_account_export(cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.account_export_dialog.is_some());
    });
}

#[gpui_kit::test]
fn a_running_export_shows_its_progress_and_hides(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = std::env::temp_dir().join(format!("quill-account-box-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut settings = AccountSettings::new(dir.clone());
    settings.sections = Sections::of(&[Section::Sessions]);
    // A worker nobody answers: it stays at its first step.
    let export = quill::export::AccountExport::start(settings, dir.clone(), None).unwrap();
    app.update_in(vcx, |app, _, cx| {
        app.demo_session.as_mut().unwrap().settings.account_export = Some(export);
        app.open_account_export(cx);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::AccountExport));
    });
    app.read_with(vcx, |app, _| {
        assert!(
            app.dialogs.account_export_dialog.is_none(),
            "progress, not options"
        );
        let export = app
            .session()
            .unwrap()
            .settings
            .account_export
            .as_ref()
            .unwrap();
        assert!(!export.progress().is_settled());
    });
    app.update_in(vcx, |app, _, cx| {
        app.dialogs.account_export_progress_hidden = true;
        cx.notify();
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_ne!(shell.open_dialog, Some(DialogKind::AccountExport));
    });
    app.update_in(vcx, |app, _, _| {
        app.demo_session.as_mut().unwrap().settings.account_export = None;
    });
    let _ = std::fs::remove_dir_all(&dir);
}

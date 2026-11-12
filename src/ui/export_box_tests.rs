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
        let draft = app.dialogs.chat_export.draft().expect("box open");
        assert_eq!(draft.settings.format, ExportFormat::Html);
        assert!(draft.settings.media.contains(MediaKind::Photo));
    });
    // Switch the calendar on for each end of the range and pick a day.
    for field in [DateField::From, DateField::Till] {
        app.update_in(vcx, |app, _, cx| {
            let draft = app.dialogs.chat_export.draft_mut().unwrap();
            draft.page = super::ExportPage::Calendar {
                field,
                month: quill::search_filters::YearMonth::of(1_700_000_000),
            };
            cx.notify();
        });
        vcx.run_until_parked();
    }
    app.update_in(vcx, |app, _, cx| {
        let draft = app.dialogs.chat_export.draft_mut().unwrap();
        draft.page = super::ExportPage::Options;
        draft.settings.set_from_day(19_700);
        draft.settings.set_till_day(19_710);
        draft.settings.media = draft.settings.media.with(MediaKind::File, true);
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let settings = &app.dialogs.chat_export.draft().unwrap().settings;
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
        app.dialogs.chat_export.close();
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.chat_export.draft().is_none());
    });
}

/// The box's own stage says whether the progress shows, so a run started
/// with the box closed stays hidden until the box is asked for again.
#[gpui_kit::test]
fn a_hidden_export_stays_hidden_until_the_box_is_asked_for_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = std::env::temp_dir().join(format!("quill-chat-box-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = quill::export::ExportSettings::new(16, "Export".into(), dir.clone());
    // A worker nobody answers: it stays at its first step.
    let export = quill::export::ChatExport::start(settings, dir.clone(), None).unwrap();
    app.update_in(vcx, |app, _, cx| {
        app.demo_session.as_mut().unwrap().settings.chat_export = Some(export);
        cx.notify();
    });
    let shown = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.dialogs.chat_export.is_progress())
    };
    assert!(!shown(&app, vcx));
    app.update_in(vcx, |app, _, cx| app.start_chat_export(ChatId(1), cx));
    assert!(shown(&app, vcx));
    app.update_in(vcx, |app, _, _| app.close_chat_export_box());
    assert!(!shown(&app, vcx));
    app.update_in(vcx, |app, _, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = None;
    });
    let _ = std::fs::remove_dir_all(&dir);
}

/// Closing the box closes it whatever it showed, and the run goes on;
/// asking for the box while the run exists drops a draft over it.
#[gpui_kit::test]
fn closing_the_box_leaves_a_running_export_going(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = std::env::temp_dir().join(format!("quill-chat-draft-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = quill::export::ExportSettings::new(16, "Export".into(), dir.clone());
    let export = quill::export::ChatExport::start(settings, dir.clone(), None).unwrap();
    // A draft is open when an export starts elsewhere.
    app.update_in(vcx, |app, _, cx| app.start_chat_export(ChatId(1), cx));
    app.update_in(vcx, |app, _, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = Some(export);
    });
    app.read_with(vcx, |app, _| assert!(app.dialogs.chat_export.is_options()));
    app.update_in(vcx, |app, _, _| app.close_chat_export_box());
    app.read_with(vcx, |app, _| {
        assert!(!app.dialogs.chat_export.is_options());
        assert!(!app.dialogs.chat_export.is_progress());
        assert!(app.session().unwrap().settings.chat_export.is_some());
    });
    app.update_in(vcx, |app, _, cx| app.start_chat_export(ChatId(1), cx));
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.chat_export.is_progress());
        assert!(!app.dialogs.chat_export.is_options());
    });
    app.update_in(vcx, |app, _, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = None;
    });
    let _ = std::fs::remove_dir_all(&dir);
}

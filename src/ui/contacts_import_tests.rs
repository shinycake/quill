//! The Contacts import box renders from `VcardImport` alone: Import waits
//! for a ready file and a failed file leaves the box open (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::vcard_import::{VcardError, VcardImport, preview_from_bytes};

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
                Some(ScreenshotDemo::named("ready-contacts-import")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("window built");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn set_state(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, state: VcardImport) {
    app.update_in(vcx, |app, _, cx| {
        if let Some(dialog) = app.dialogs.import_contacts_dialog.as_mut() {
            dialog.state = state;
        }
        cx.notify();
    });
    vcx.run_until_parked();
}

fn submit(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.submit_import_contacts_dialog(window, cx);
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn the_box_opens_empty_and_a_failed_file_cannot_be_imported(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::ImportContacts));
    });
    submit(&app, vcx);
    for state in [
        VcardImport::Empty,
        VcardImport::Failed {
            file_name: "notes.txt".into(),
            error: VcardError::NotVcard,
        },
    ] {
        set_state(&app, vcx, state);
        submit(&app, vcx);
        app.read_with(vcx, |app, _| {
            assert!(app.dialogs.import_contacts_dialog.is_some());
        });
    }
}

#[gpui_kit::test]
fn a_ready_file_imports_and_closes_the_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let ready = preview_from_bytes(
        "people.vcf",
        b"BEGIN:VCARD\nFN:Ada Lovelace\nTEL:+15550100\nEND:VCARD\n",
    );
    assert!(ready.can_import());
    set_state(&app, vcx, ready);
    submit(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.import_contacts_dialog.is_none());
    });
    shell.read_with(vcx, |shell, _| assert_eq!(shell.open_dialog, None));
}

/// Sweep 09-001: a read started in a closed box can never be taken for the
/// next box's, which would show the wrong file as ready.
#[gpui_kit::test]
fn a_new_box_never_shares_a_read_token_with_a_closed_one(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let file = quill::scratch::process_dir("quill-contacts-import").join("people.vcf");
    let _ = std::fs::create_dir_all(file.parent().expect("has a parent"));
    std::fs::write(&file, "BEGIN:VCARD\nFN:Ada\nTEL:+15550100\nEND:VCARD\n").expect("writes");
    let mut picks = Vec::new();
    for _ in 0..2 {
        app.update_in(vcx, |app, window, cx| {
            app.close_import_contacts_dialog(cx);
            app.open_import_contacts_dialog(window, cx);
            app.read_vcard(file.clone(), cx);
            picks.extend(app.dialogs.import_contacts_dialog.as_ref().map(|d| d.pick));
        });
    }
    vcx.run_until_parked();
    assert_eq!(picks.len(), 2);
    assert_ne!(picks.first(), picks.get(1));
}

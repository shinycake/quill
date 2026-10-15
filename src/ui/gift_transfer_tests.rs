//! The transfer picker over the demo's chat list: it opens above the
//! gifts dialog, lists the contacts, explains the locked export, walks
//! the steps and closes back to the gift (needs the `demo-capture` feature
//! for gpui-kit's test support). Nothing reaches Telegram: the app has no
//! connection, so neither Transfer nor Submit can send anything.
use super::DEMO_GIFT;
use crate::ui::app::QuillApp;
use crate::ui::gift_transfer_view::stage_view;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::gift_manage::TransferStage;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    quill::gift_market::set_gift_market_persistence(false);
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
                Some(ScreenshotDemo::named("ready-gift-transfer")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn stage(app: &QuillApp) -> Option<TransferStage> {
    app.session()
        .and_then(|s| s.payments.gifts_manage.transfer())
        .map(|p| p.stage.clone())
}

#[gpui_kit::test]
fn the_picker_opens_over_the_gift_and_walks_its_steps(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::GiftTransfer));
    });
    app.update_in(vcx, |app, _, cx| {
        assert_eq!(stage(app), Some(TransferStage::Pick));
        let view = stage_view(app, cx).expect("the picker");
        assert_eq!(view.title.as_deref(), Some("Transfer Plush Pepe #1,204"));
        // The export unlocks in five days: the row explains, OK goes back.
        app.tap_transfer_export(cx);
        assert_eq!(stage(app), Some(TransferStage::Locked { hours: 120 }));
        let view = stage_view(app, cx).expect("the locked box");
        assert_eq!(view.title.as_deref(), Some("Unlocking in progress"));
    });
    app.update_in(vcx, |app, window, cx| {
        app.transfer_step_back(window, cx);
        assert_eq!(stage(app), Some(TransferStage::Pick));
        // A contact: the confirmation, without a title as in tdesktop.
        app.pick_transfer_contact(502, "Maya Chen".into(), cx);
        assert_eq!(
            stage(app),
            Some(TransferStage::ConfirmPeer {
                user_id: 502,
                name: "Maya Chen".into()
            })
        );
        assert!(stage_view(app, cx).is_some_and(|v| v.title.is_none()));
        // The demo has no connection: Transfer sends nothing and says so.
        app.confirm_picked_transfer(cx);
        assert!(stage(app).is_some());
        app.transfer_step_back(window, cx);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::GiftTransfer));
    });
    // Closing the picker shows the gift again.
    app.update_in(vcx, |app, window, cx| app.close_gift_transfer(window, cx));
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::ReceivedGifts));
    });
    app.read_with(vcx, |app, _| {
        let hub = app.session().map(|s| &s.payments.hub);
        assert_eq!(
            hub.and_then(|h| h.gift_selected.as_deref()),
            Some(DEMO_GIFT)
        );
        assert!(stage(app).is_none());
    });
}

#[gpui_kit::test]
fn the_password_box_never_keeps_the_password(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        // Straight to the password box, as after "Open Fragment".
        if let Some(picker) = app.gift_manage_mut().and_then(|m| m.transfer_mut()) {
            picker.stage = TransferStage::Password {
                sending: false,
                error: None,
            };
        }
        let view = stage_view(app, cx).expect("the password box");
        assert_eq!(view.title.as_deref(), Some("Two-step verification"));
        app.payments
            .gift_export_password
            .update(cx, |input, cx| input.set_value("CANARY-pw", window, cx));
        // Submit empties the field at once (and the demo sends nothing).
        app.submit_gift_export_password(window, cx);
        assert!(
            app.payments
                .gift_export_password
                .read(cx)
                .value()
                .is_empty()
        );
        app.payments
            .gift_export_password
            .update(cx, |input, cx| input.set_value("CANARY-pw", window, cx));
        // Leaving the box empties it too.
        app.transfer_step_back(window, cx);
        assert!(
            app.payments
                .gift_export_password
                .read(cx)
                .value()
                .is_empty()
        );
        assert_eq!(stage(app), Some(TransferStage::Pick));
    });
}

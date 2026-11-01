//! Settings → My Grams in a real window (needs the `demo-capture` feature
//! for gpui-kit's test support): the screen, the tabs, the receipt and the
//! Settings row, from the recorded fixture.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::gift_manage::ExportError;
use quill::premium_hub::TxFilter;
use quill::premium_hub::ton::GramWithdraw;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, _shell, vcx) = new_app_with_shell(cx);
    (app, vcx)
}

fn new_app_with_shell(
    cx: &mut TestAppContext,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(2400.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-grams")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn grams_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session().is_some_and(|s| s.payments.hub.grams.open)
    })
}

#[gpui_kit::test]
fn the_screen_shows_balance_withdrawal_and_every_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.open_grams(cx));
    vcx.run_until_parked();
    assert!(grams_open(&app, vcx));
    assert!(vcx.debug_bounds("grams-balance").is_some());
    assert!(vcx.debug_bounds("grams-withdrawal").is_some());
    for id in [
        "grams-tx-tx-topup",
        "grams-tx-tx-withdrawal",
        "grams-tx-tx-suggested",
        "grams-tx-tx-sale",
        "grams-tx-tx-unsupported",
    ] {
        assert!(vcx.debug_bounds(id).is_some(), "{id}");
    }

    // A tab switch keeps the dialog and changes the filter.
    app.update_in(vcx, |app, _, cx| {
        app.set_grams_filter(TxFilter::Outgoing, cx)
    });
    vcx.run_until_parked();
    let filter = app.read_with(vcx, |app, _| {
        app.session().map(|s| s.payments.hub.grams.filter)
    });
    assert_eq!(filter, Some(TxFilter::Outgoing));

    // A row opens its receipt, Back returns to the list.
    app.update_in(vcx, |app, _, cx| {
        app.select_grams_tx(Some("tx-sale".into()), cx)
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("grams-receipt").is_some());
    assert!(vcx.debug_bounds("grams-balance").is_none());
    app.update_in(vcx, |app, _, cx| app.select_grams_tx(None, cx));
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("grams-receipt").is_none());

    app.update_in(vcx, |app, _, cx| app.close_grams(cx));
    vcx.run_until_parked();
    assert!(!grams_open(&app, vcx));
}

#[gpui_kit::test]
fn settings_lists_my_grams_only_with_a_balance(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let row = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update_in(vcx, |app, _, cx| {
            app.grams_settings_row(|_, _| {}, cx).is_some()
        })
    };
    assert!(row(&app, vcx));
    // The Settings list renders with the row.
    app.update_in(vcx, |app, _, cx| {
        app.settings.open = true;
        cx.notify();
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.hub.grams.balance_nano = Some(0);
        }
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(!row(&app, vcx));
}

#[gpui_kit::test]
fn an_unknown_peer_shows_a_placeholder_then_a_name(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let title = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            let session = app.session()?;
            let tx = session
                .payments
                .hub
                .grams
                .entries
                .iter()
                .find(|tx| tx.id == "tx-sale")?;
            super::peer_name(session, tx.peer()?)
        })
    };
    // The demo seeds the buyer's name.
    assert_eq!(title(&app, vcx), Some("Leo Park".to_string()));
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.users.remove(&12);
        }
        app.open_grams(cx);
        app.select_grams_tx(Some("tx-sale".into()), cx);
    });
    vcx.run_until_parked();
    // Unknown: a placeholder, never "user 12"; the receipt still draws.
    assert_eq!(title(&app, vcx), None);
    assert!(vcx.debug_bounds("grams-receipt").is_some());
    // TDLib could not return it: a deleted account.
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.hub.grams.missing_users.insert(12);
        }
        cx.notify();
    });
    vcx.run_until_parked();
    assert_eq!(title(&app, vcx), Some("Deleted Account".to_string()));
}

fn withdraw(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<GramWithdraw> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.payments.hub.grams.withdraw.clone())
    })
}

fn password_value(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| {
        app.payments
            .grams_withdraw_password
            .read(cx)
            .value()
            .to_string()
    })
}

#[gpui_kit::test]
fn collect_opens_the_password_box_over_the_screen(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app_with_shell(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.open_grams(cx));
    vcx.run_until_parked();
    // The button sits in the withdrawal section.
    assert!(vcx.debug_bounds("grams-withdraw").is_some());
    app.update_in(vcx, |app, _, cx| app.start_grams_withdraw(cx));
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::GramsWithdraw));
    });
    assert_eq!(withdraw(&app, vcx), Some(GramWithdraw::idle(None)));

    // The field took the keyboard; Enter submits (the demo sends
    // nothing) and the field is emptied at once.
    vcx.simulate_keystrokes("a b c");
    assert_eq!(password_value(&app, vcx), "abc");
    vcx.simulate_keystrokes("enter");
    assert!(password_value(&app, vcx).is_empty());

    // An error stays on screen under the field.
    app.update_in(vcx, |app, _, cx| {
        if let Some(grams) = app.demo_session.as_mut().map(|s| &mut s.payments.hub.grams) {
            grams.withdraw = Some(GramWithdraw::idle(Some(ExportError::WrongPassword)));
        }
        cx.notify();
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::GramsWithdraw));
    });

    // Cancel closes the box, empties the field and shows the screen again.
    app.update_in(vcx, |app, window, cx| {
        app.payments
            .grams_withdraw_password
            .update(cx, |input, cx| input.set_value("CANARY", window, cx));
        app.cancel_grams_withdraw(window, cx);
    });
    vcx.run_until_parked();
    assert_eq!(withdraw(&app, vcx), None);
    assert!(password_value(&app, vcx).is_empty());
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::Grams));
    });

    // The security check replaces the box when there is no password.
    app.update_in(vcx, |app, _, cx| {
        if let Some(grams) = app.demo_session.as_mut().map(|s| &mut s.payments.hub.grams) {
            grams.withdraw = Some(GramWithdraw::SecurityCheck { later: false });
        }
        cx.notify();
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::GramsWithdraw));
    });
}

#[gpui_kit::test]
fn the_withdrawal_url_opens_only_through_the_link_policy(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.open_grams(cx));
    vcx.run_until_parked();
    let hand_over = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, url: &str| {
        let url = url.to_string();
        app.update_in(vcx, |app, _, cx| {
            if let Some(grams) = app.demo_session.as_mut().map(|s| &mut s.payments.hub.grams) {
                grams.withdraw_url = Some(url);
            }
            cx.notify();
        });
        vcx.run_until_parked();
    };
    // Not https: never opened, the status line says so.
    hand_over(&app, vcx, "http://fragment.com/withdraw");
    assert_eq!(quill::platform::os_opened_last(), None);
    // A look-alike address (Cyrillic "а") asks first: the link box opens,
    // the browser does not.
    let look_alike = "https://fr\u{0430}gment.com/withdraw";
    hand_over(&app, vcx, look_alike);
    assert_eq!(quill::platform::os_opened_last(), None);
    let asked = app.read_with(vcx, |app, _| {
        app.message_ui
            .open_link_confirm
            .as_ref()
            .map(|confirm| confirm.url.clone())
    });
    assert_eq!(asked.as_deref(), Some(look_alike));
    app.update_in(vcx, |app, _, cx| {
        app.message_ui.open_link_confirm = None;
        cx.notify();
    });
    // Fragment's https page opens in the browser, once.
    hand_over(&app, vcx, "https://fragment.com/withdraw?token=1");
    assert_eq!(
        quill::platform::os_opened_last().as_deref(),
        Some("https://fragment.com/withdraw?token=1")
    );
    let left = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.payments.hub.grams.withdraw_url.clone())
    });
    assert_eq!(left, None);
}

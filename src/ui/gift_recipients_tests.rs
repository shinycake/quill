//! The birthday card's gift action, "Gift Premium or Stars" and Settings >
//! Premium Gifting over the demo account (needs the `demo-capture` feature
//! for gpui-kit's test support). Nothing reaches Telegram.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::lang::{keys, tr};
use quill::settings_catalog::Page;

fn new_app(
    cx: &mut TestAppContext,
    kind: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let demo = ScreenshotDemo::named(kind);
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

/// The kit dialog that shows, once its open animation has finished so
/// clicks land where the controls are drawn.
fn shown_dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    vcx.update(|window, cx| window.render_frame(cx));
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn labelled(vcx: &mut VisualTestContext, label: &str) -> bool {
    vcx.update(|window, _| window.try_find_by_label(label).is_some())
}

fn click_label(vcx: &mut VisualTestContext, label: &str) {
    let bounds = vcx.update(|window, _| window.find_by_label(label).bounds());
    vcx.simulate_click(bounds.center(), Modifiers::default());
}

/// A click on the chat list's suggestion card.
fn click_card(vcx: &mut VisualTestContext) {
    let bounds = vcx.debug_bounds("suggestion-body").expect("the card");
    vcx.simulate_click(bounds.center(), Modifiers::default());
}

fn card_shown(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| window.try_find("suggestion-dismiss").is_some())
}

/// Who the open send-gift dialog is for.
fn gift_for(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<String> {
    app.read_with(vcx, |app, _| {
        let send = &app.session()?.payments.gifts_manage.send;
        send.open
            .then(|| send.recipient.as_ref().map(|r| r.name.clone()))
            .flatten()
    })
}

#[gpui_kit::test]
fn several_birthdays_open_the_list_and_a_row_opens_the_gift(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-birthday-gift");
    let vcx = &mut vcx;
    assert_eq!(shown_dialog(&shell, vcx), None);
    click_card(vcx);
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::GiftRecipients));
    let today = format!(", {}", tr(keys::GIFT_LIST_BIRTHDAY_STATUS_TODAY));
    let today = today.replace("{emoji}", "\u{1F382}");
    assert!(labelled(
        vcx,
        &format!("Quill Demo, {}", tr(keys::GIFT_SELF_STATUS))
    ));
    assert!(labelled(vcx, &format!("Ada Lovelace{today}")));
    assert!(labelled(vcx, &format!("Grace Hopper{today}")));
    assert!(labelled(vcx, "Linus Pauling"), "other contacts");
    click_label(vcx, &format!("Grace Hopper{today}"));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::SendGift));
    assert_eq!(gift_for(&app, vcx).as_deref(), Some("Grace Hopper"));
    app.read_with(vcx, |app, _| assert!(!app.payments.gift_recipients_open));
}

#[gpui_kit::test]
fn one_birthday_opens_the_gift_picker_for_them(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-birthday-gift-one");
    let vcx = &mut vcx;
    shown_dialog(&shell, vcx);
    click_card(vcx);
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::SendGift));
    assert_eq!(gift_for(&app, vcx).as_deref(), Some("Ada Lovelace"));
}

#[gpui_kit::test]
fn hiding_the_card_says_where_gifts_wait(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-birthday-gift");
    let vcx = &mut vcx;
    shown_dialog(&shell, vcx);
    vcx.update(|window, cx| window.click("suggestion-dismiss", cx));
    shown_dialog(&shell, vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.connection.status_note(),
            tr(keys::DIALOGS_SUGGESTIONS_BIRTHDAY_CONTACT_DISMISS)
        );
        assert!(
            app.session()
                .is_some_and(|s| s.chat_list.suggestions.birthdays_hidden)
        );
    });
    assert!(!card_shown(vcx));
}

#[gpui_kit::test]
fn no_birthday_card_where_premium_is_not_sold(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-birthday-gift");
    let vcx = &mut vcx;
    shown_dialog(&shell, vcx);
    assert!(card_shown(vcx));
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_available = Some(false);
        }
        cx.notify();
    });
    shown_dialog(&shell, vcx);
    assert!(!card_shown(vcx));
}

#[gpui_kit::test]
fn the_search_narrows_the_list(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-birthday-gift");
    let vcx = &mut vcx;
    shown_dialog(&shell, vcx);
    app.update_in(vcx, |app, window, cx| app.open_gift_recipients(window, cx));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::GiftRecipients));
    app.update_in(vcx, |app, window, cx| {
        app.payments
            .gift_recipient_search
            .update(cx, |input, cx| input.set_value("lin", window, cx));
    });
    shown_dialog(&shell, vcx);
    assert!(labelled(vcx, "Linus Pauling"));
    assert!(!labelled(vcx, "Ada Lovelace"), "no match");
}

#[gpui_kit::test]
fn settings_offers_premium_gifting_where_premium_is_sold(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-birthday-gift");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Telegram));
        cx.notify();
    });
    assert_eq!(shown_dialog(&shell, vcx), None);
    vcx.update(|window, cx| window.click("settings-row-SendGift", cx));
    assert_eq!(shown_dialog(&shell, vcx), Some(DialogKind::GiftRecipients));
    app.update_in(vcx, |app, window, cx| {
        app.payments.gift_recipients_open = false;
        app.close_kit_dialog_if_done(DialogKind::GiftRecipients, window, cx);
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_available = Some(false);
        }
        cx.notify();
    });
    assert_eq!(shown_dialog(&shell, vcx), None);
    vcx.update(|window, _| assert!(window.try_find("settings-row-SendGift").is_none()));
}

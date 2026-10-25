//! The privacy editor's Premium upsells over the `ready-privacy` demo: the
//! Subscribe rows of Last seen and Voice messages, the locked voice choice
//! and the one-time last seen warning.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::privacy::PrivacyEditorTarget;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::privacy::prefs::{last_seen_warning_seen, set_privacy_hints_persistence};
use quill::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use quill::telegram::requests::PrivacyWho;
use quill::telegram::requests_privacy::PrivacySettingKey;

fn new_app(
    cx: &mut TestAppContext,
    premium: bool,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    set_privacy_hints_persistence(false);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-privacy");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_option = Some(premium);
        }
        cx.notify();
    });
    frames(&mut vcx);
    (app, shell, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn has(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn show_last_seen_to_everybody(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.privacy.insert(
                PrivacySettingKey::ShowStatus,
                PrivacyKeyState::Ready(PrivacyRuleDetail {
                    who: Some(PrivacyWho::Everybody),
                    ..Default::default()
                }),
            );
        }
        cx.notify();
    });
    frames(vcx);
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| window.click(id, cx));
    frames(vcx);
}

fn open_editor(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, key: PrivacySettingKey) {
    app.update(vcx, |app, cx| {
        app.privacy.editor = Some(PrivacyEditorTarget::Rule(key));
        cx.notify();
    });
    frames(vcx);
}

fn rule_who(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    key: PrivacySettingKey,
) -> Option<PrivacyWho> {
    app.read_with(vcx, |app, _| {
        match app.session()?.settings.privacy.get(&key)? {
            PrivacyKeyState::Ready(detail) => detail.who,
            _ => None,
        }
    })
}

fn premium_page(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(bool, String)> {
    app.read_with(vcx, |app, _| {
        app.session().map(|session| {
            (
                session.payments.hub.premium_open,
                session.payments.hub.premium_ref.clone(),
            )
        })
    })
}

fn dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn choose(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    key: PrivacySettingKey,
    who: PrivacyWho,
) {
    app.update_in(vcx, |app, window, cx| {
        app.set_privacy_target_who(PrivacyEditorTarget::Rule(key), who, window, cx);
    });
    frames(vcx);
}

#[gpui_kit::test]
fn a_free_account_can_subscribe_from_last_seen(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, false);
    let vcx = &mut vcx;
    open_editor(&app, vcx, PrivacySettingKey::ShowStatus);
    assert!(has(vcx, "privacy-lastseen-subscribe"));
    click(vcx, "privacy-lastseen-subscribe");
    assert_eq!(
        premium_page(&app, vcx),
        Some((true, "lastseen".to_string()))
    );
}

#[gpui_kit::test]
fn premium_and_everybody_hide_the_last_seen_subscribe_row(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    open_editor(&app, vcx, PrivacySettingKey::ShowStatus);
    assert!(!has(vcx, "privacy-lastseen-subscribe"));
    show_last_seen_to_everybody(&app, vcx);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_option = Some(false);
        }
        cx.notify();
    });
    frames(vcx);
    assert!(!has(vcx, "privacy-lastseen-subscribe"), "nothing is hidden");
}

#[gpui_kit::test]
fn a_free_account_confirms_hiding_last_seen_once(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, false);
    let vcx = &mut vcx;
    let key = PrivacySettingKey::ShowStatus;
    open_editor(&app, vcx, key);
    show_last_seen_to_everybody(&app, vcx);
    assert!(!has(vcx, "privacy-lastseen-subscribe"));

    choose(&app, vcx, key, PrivacyWho::Nobody);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::LastSeenWarning));
    click(vcx, "lastseen-warning-cancel");
    assert_eq!(rule_who(&app, vcx, key), Some(PrivacyWho::Everybody));
    assert!(!last_seen_warning_seen());

    choose(&app, vcx, key, PrivacyWho::Nobody);
    click(vcx, "lastseen-warning-continue");
    assert_eq!(rule_who(&app, vcx, key), Some(PrivacyWho::Nobody));
    assert!(last_seen_warning_seen());
    assert_ne!(dialog(&shell, vcx), Some(DialogKind::LastSeenWarning));

    choose(&app, vcx, key, PrivacyWho::Contacts);
    assert_eq!(
        rule_who(&app, vcx, key),
        Some(PrivacyWho::Contacts),
        "asked only once"
    );
    assert_ne!(dialog(&shell, vcx), Some(DialogKind::LastSeenWarning));
}

#[gpui_kit::test]
fn voice_messages_offer_premium_to_a_free_account(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, false);
    let vcx = &mut vcx;
    let key = PrivacySettingKey::AllowVoiceMessages;
    open_editor(&app, vcx, key);
    choose(&app, vcx, key, PrivacyWho::Nobody);
    assert_eq!(
        rule_who(&app, vcx, key),
        Some(PrivacyWho::Everybody),
        "locked"
    );
    assert!(has(vcx, "privacy-voices-subscribe"));
    click(vcx, "privacy-voices-subscribe");
    assert_eq!(
        premium_page(&app, vcx),
        Some((true, "voice_restrictions_require_premium".to_string()))
    );
}

#[gpui_kit::test]
fn premium_unlocks_voice_message_choices(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, true);
    let vcx = &mut vcx;
    let key = PrivacySettingKey::AllowVoiceMessages;
    open_editor(&app, vcx, key);
    assert!(!has(vcx, "privacy-voices-subscribe"));
    choose(&app, vcx, key, PrivacyWho::Nobody);
    assert_eq!(rule_who(&app, vcx, key), Some(PrivacyWho::Nobody));
}

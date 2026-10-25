//! The chat look footer over a private chat with a new wallpaper picked:
//! "Apply for me", and "Apply for me and {user}" that a free account
//! unlocks through the Premium preview.
use super::demo::{demo_private_chat, seed_backgrounds};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::premium_hub::PremiumFeature;
use quill::premium_hub::promo::{PreviewOpen, PromoPage};

fn new_app(cx: &mut TestAppContext, premium: bool) -> (Entity<QuillApp>, VisualTestContext, i64) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-chats");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let chat = app.update(&mut vcx, |app, cx| {
        let session = app.demo_session.as_mut()?;
        session.payments.premium_option = Some(premium);
        seed_backgrounds(session);
        let chat = demo_private_chat(session)?;
        app.open_chat_look_dialog(chat, cx);
        let dialog = app.dialogs.chat_look_dialog.as_mut()?;
        dialog.background = Some(901);
        Some(chat)
    });
    frames(&mut vcx);
    (app, vcx, chat.expect("a private chat in the demo"))
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

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| window.click(id, cx));
    frames(vcx);
}

fn chat_wallpaper(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: i64) -> Option<i64> {
    app.read_with(vcx, |app, _| {
        app.session()?
            .chats_state
            .chat_backgrounds
            .get(&chat)
            .map(|background| background.background.id)
    })
}

fn look_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.dialogs.chat_look_dialog.is_some())
}

#[gpui_kit::test]
fn a_new_wallpaper_offers_me_or_both(cx: &mut TestAppContext) {
    let (_app, mut vcx, _) = new_app(cx, false);
    let vcx = &mut vcx;
    assert!(has(vcx, "chat-look-apply-me"));
    assert!(has(vcx, "chat-look-apply-both"));
    assert!(!has(vcx, "chat-look-apply"));
}

#[gpui_kit::test]
fn a_free_account_applies_for_itself(cx: &mut TestAppContext) {
    let (app, mut vcx, chat) = new_app(cx, false);
    let vcx = &mut vcx;
    click(vcx, "chat-look-apply-me");
    assert!(!look_open(&app, vcx));
    assert_eq!(chat_wallpaper(&app, vcx, chat), Some(901));
}

#[gpui_kit::test]
fn a_free_account_unlocks_both_through_the_preview(cx: &mut TestAppContext) {
    let (app, mut vcx, chat) = new_app(cx, false);
    let vcx = &mut vcx;
    click(vcx, "chat-look-apply-both");
    let page = app.read_with(vcx, |app, _| {
        app.session().map(|session| {
            (
                session.payments.hub.premium_open,
                session.payments.hub.promo_page,
            )
        })
    });
    assert_eq!(
        page,
        Some((
            true,
            PromoPage::Preview {
                feature: PremiumFeature::BackgroundForBoth.into(),
                opened: PreviewOpen::Box,
            }
        ))
    );
    assert!(
        !look_open(&app, vcx),
        "the chat look closes for the preview"
    );
    assert_eq!(chat_wallpaper(&app, vcx, chat), None);
}

#[gpui_kit::test]
fn premium_applies_for_both(cx: &mut TestAppContext) {
    let (app, mut vcx, chat) = new_app(cx, true);
    let vcx = &mut vcx;
    click(vcx, "chat-look-apply-both");
    assert!(!look_open(&app, vcx));
    assert_eq!(chat_wallpaper(&app, vcx, chat), Some(901));
    let premium_open = app.read_with(vcx, |app, _| {
        app.session().is_some_and(|s| s.payments.hub.premium_open)
    });
    assert!(!premium_open);
}

#[gpui_kit::test]
fn a_theme_alone_keeps_one_apply(cx: &mut TestAppContext) {
    let (app, mut vcx, _) = new_app(cx, false);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(dialog) = app.dialogs.chat_look_dialog.as_mut() {
            dialog.background = None;
        }
        cx.notify();
    });
    frames(vcx);
    assert!(has(vcx, "chat-look-apply"));
    assert!(!has(vcx, "chat-look-apply-both"));
}

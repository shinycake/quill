use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::MessageMenuState;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::{ChatId, MessageId};
use quill::premium_hub::promo::PromoPage;
use quill::reaction_limit::ReactionLimits;
use quill::state::{MessageReactionsState, ReactionChoice};

const CHAT: ChatId = ChatId(11);
const MESSAGE: MessageId = MessageId(101);

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn premium_features() -> quill::premium_hub::PremiumInfo {
    quill::premium_hub::parse_premium_features(&serde_json::json!({
        "features": [
            {"@type": "premiumFeatureIncreasedLimits"},
            {"@type": "premiumFeatureUniqueReactions"},
        ],
        "limits": [],
    }))
}

fn free_account(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.premium_option = Some(false);
        session.payments.hub.premium = Some(premium_features());
        cx.notify();
    });
    frames(cx);
}

fn open_menu(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_message_menu(
                MessageMenuState {
                    chat_id: CHAT,
                    message_id: MESSAGE,
                    position: point(px(420.), px(300.)),
                },
                window,
                cx,
            );
            if let Some(session) = app.demo_session.as_mut() {
                crate::ui::demos::reaction_fixtures::seed_reaction_options(session, CHAT, MESSAGE);
                if let MessageReactionsState::Ready(options) =
                    &mut session.stickers.message_reaction_options
                {
                    options.top.truncate(6);
                    options.top.push(ReactionChoice::CustomEmoji(77));
                    options.premium_locked = vec![ReactionChoice::CustomEmoji(77)];
                }
            }
            cx.notify();
        });
    });
    frames(cx);
}

/// The strip cell id of `choice`: cells are numbered in the strip's order.
fn strip_cell(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    choice: &ReactionChoice,
) -> gpui_kit::SharedString {
    let ix = app.read_with(cx, |app, _| {
        app.session()
            .and_then(|s| s.stickers.message_reaction_options.ready_for(CHAT, MESSAGE))
            .and_then(|options| options.top.iter().position(|c| c == choice))
    });
    format!("reaction-strip-{}", ix.expect("offered in the strip")).into()
}

fn premium_page(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<PromoPage> {
    app.read_with(cx, |app, _| Some(app.session()?.payments.hub.promo_page))
}

fn reactions(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<(String, i32, bool)> {
    app.read_with(cx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&CHAT.0))
            .and_then(|h| h.messages.get(&MESSAGE.0))
            .map(|m| {
                m.reaction_chips()
                    .iter()
                    .map(|chip| {
                        (
                            chip.reaction_type.emoji_text().unwrap_or("?").to_string(),
                            chip.total_count,
                            chip.is_chosen,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn a_locked_reaction_in_the_strip_opens_the_premium_preview(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-reactions");
    let vcx = &mut vcx;
    free_account(&app, vcx);
    open_menu(&app, vcx);
    let before = reactions(&app, vcx);
    let cell = strip_cell(&app, vcx, &ReactionChoice::CustomEmoji(77));
    vcx.update(|window, cx| window.click(gpui_kit::ElementId::Name(cell.clone()), cx));
    frames(vcx);
    assert_eq!(premium_page(&app, vcx), Some(PromoPage::Preview(1)));
    assert_eq!(reactions(&app, vcx), before, "nothing was sent");
}

#[gpui_kit::test]
fn a_free_account_swaps_its_reaction_from_the_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-reactions");
    let vcx = &mut vcx;
    free_account(&app, vcx);
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stickers.reaction_limits = ReactionLimits {
                default: Some(1),
                premium: Some(3),
            };
        }
    });
    open_menu(&app, vcx);
    assert_eq!(
        reactions(&app, vcx),
        vec![("❤".into(), 3, true), ("👍".into(), 2, false)]
    );
    let cell = strip_cell(&app, vcx, &ReactionChoice::Emoji("👍".into()));
    vcx.update(|window, cx| window.click(gpui_kit::ElementId::Name(cell.clone()), cx));
    frames(vcx);
    assert_eq!(
        reactions(&app, vcx),
        vec![("❤".into(), 2, false), ("👍".into(), 3, true)]
    );
    assert_ne!(premium_page(&app, vcx), Some(PromoPage::Preview(1)));
}

#[gpui_kit::test]
fn restore_on_a_locked_pack_opens_the_premium_preview(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-reaction-packs");
    let vcx = &mut vcx;
    free_account(&app, vcx);
    let restore = vcx.update(|window, _| window.try_find("reaction-pack-restore-1").is_some());
    assert!(restore, "the first pack header offers Restore");
    vcx.update(|window, cx| window.click("reaction-pack-restore-1", cx));
    frames(vcx);
    assert_eq!(premium_page(&app, vcx), Some(PromoPage::Preview(1)));
    assert!(
        app.read_with(vcx, |app, _| !app.pickers.media_panel.open),
        "the selector closes"
    );
}

#[gpui_kit::test]
fn premium_packs_have_no_lock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-reaction-packs");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_option = Some(true);
        }
        cx.notify();
    });
    frames(vcx);
    let restore = vcx.update(|window, _| window.try_find("reaction-pack-restore-1").is_some());
    assert!(!restore);
}

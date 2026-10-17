//! Crafting over the demo's chat list: the info box, the slots and their
//! chances, "Select Gifts" and its gates, the forge and the result (needs
//! the `demo-capture` feature for gpui-kit's test support). Nothing
//! reaches Telegram: the app has no connection.
use crate::ui::app::QuillApp;
use crate::ui::gift_craft_ui::FORGE_SECONDS;
use crate::ui::gift_market_state::MarketBox;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::gift_market::CraftResult;
use std::time::{Duration, Instant};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-gift-craft")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn slot_ids(app: &QuillApp) -> Vec<String> {
    app.craft_session()
        .map(|craft| {
            craft
                .slots
                .iter()
                .map(|slot| slot.received_gift_id.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[gpui_kit::test]
fn the_craft_box_adds_and_removes_gifts(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Craft));
        assert_eq!(slot_ids(app), ["own-1204", "own-77", "own-19"]);
        let view = app.craft_box(cx);
        assert_eq!(view.title.as_ref(), "Craft Gift");
        // 12% + 15% + 8%.
        assert_eq!(
            app.craft_session().map(|c| c.success_per_mille()),
            Some(350)
        );
        // "Select Gifts" offers the gifts not in a slot.
        app.gift_market.push(MarketBox::CraftSelect);
        let _ = app.craft_select_box(cx);
        app.pick_craft_gift("own-640", cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Craft));
        assert_eq!(slot_ids(app).len(), 4);
        // A gift that can't be crafted yet explains when it can.
        app.gift_market.push(MarketBox::CraftSelect);
        app.pick_craft_gift("own-5120", cx);
        match app.gift_market.top() {
            Some(MarketBox::Notice { title, text }) => {
                assert_eq!(title, "Crafting unavailable");
                assert!(text.starts_with("This gift will become available for crafting on "));
            }
            other => panic!("expected the notice, got {other:?}"),
        }
        app.market_back(cx);
        app.market_back(cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Craft));
        // Leaving the craft box drops the craft.
        app.market_back(cx);
        assert!(app.craft_session().is_none());
    });
}

#[gpui_kit::test]
fn the_info_box_starts_a_craft(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        let primary = app
            .craft_session()
            .and_then(|craft| craft.slots.first().cloned())
            .expect("the first gift");
        app.gift_market.stack.clear();
        app.open_craft_info(primary, cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::CraftInfo));
        let view = app.craft_info_box(cx);
        assert_eq!(view.title.as_ref(), "Gift Crafting");
    });
    // "Start Crafting" replaces the info box with a craft of one gift.
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click("craft-info-start", cx);
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, _| {
        assert_eq!(app.gift_market.stack, vec![MarketBox::Craft]);
        assert_eq!(slot_ids(app), ["own-1204"]);
    });
}

#[gpui_kit::test]
fn the_forge_ends_in_the_result(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        app.craft_now(cx);
        assert!(app.gift_market.forge_started.is_some());
        assert_eq!(app.craft_box(cx).title.as_ref(), "Crafting");
        // Not before the forge ran its time.
        app.craft_frame_upkeep(cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Craft));
        app.gift_market.forge_started =
            Instant::now().checked_sub(Duration::from_secs_f32(FORGE_SECONDS + 0.1));
        app.craft_frame_upkeep(cx);
        assert!(app.gift_market.forge_started.is_none());
        assert_eq!(app.gift_market.top(), Some(&MarketBox::CraftResult));
        assert_eq!(
            app.craft_session().and_then(|c| c.result.clone()),
            Some(CraftResult::Fail)
        );
        assert_eq!(app.craft_result_box(cx).title.as_ref(), "Crafting Failed");
        // "Craft New Gift": empty slots, then "Select Gifts".
        app.craft_again(cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::CraftSelect));
        assert!(slot_ids(app).is_empty());
        // The burnt gifts are gone from the list.
        let left: Vec<String> = app
            .craft_session()
            .map(|c| {
                c.free_candidates()
                    .map(|g| g.received_gift_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(left, ["own-640", "own-5120"]);
    });
}

/// Ends the running forge with `result` as if `craftGift` had answered.
fn answer_forge(app: &mut QuillApp, result: CraftResult) {
    if let Some(craft) = app.craft_mut() {
        craft.crafting = false;
        craft.result = Some(result);
    }
    app.gift_market.forge_started =
        Instant::now().checked_sub(Duration::from_secs_f32(FORGE_SECONDS + 0.1));
}

#[gpui_kit::test]
fn a_refused_craft_keeps_the_craft_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        // craftGiftResultTooEarly: the "Crafting unavailable" notice over
        // the craft box, with the gifts still in their slots.
        app.craft_now(cx);
        answer_forge(app, CraftResult::TooEarly(3_600));
        app.craft_frame_upkeep(cx);
        assert!(app.gift_market.forge_started.is_none());
        match app.gift_market.top() {
            Some(MarketBox::Notice { title, text }) => {
                assert_eq!(title, "Crafting unavailable");
                assert!(
                    text.starts_with("This gift will become available for crafting on"),
                    "{text}"
                );
            }
            other => panic!("expected the notice, got {other:?}"),
        }
        assert_eq!(app.craft_session().and_then(|c| c.result.clone()), None);
        assert_eq!(slot_ids(app), ["own-1204", "own-77", "own-19"]);
        app.market_back(cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Craft));

        // craftGiftResultInvalidGift: a status note, the craft box stays.
        app.craft_now(cx);
        answer_forge(app, CraftResult::InvalidGift);
        app.craft_frame_upkeep(cx);
        assert!(app.gift_market.forge_started.is_none());
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Craft));
        assert_eq!(
            app.connection.status_note(),
            "This gift can't be used for crafting."
        );
        assert_eq!(app.craft_session().and_then(|c| c.result.clone()), None);
        assert_eq!(slot_ids(app), ["own-1204", "own-77", "own-19"]);
    });
}

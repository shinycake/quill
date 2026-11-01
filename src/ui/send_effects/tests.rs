//! UI tests on the `ready-message-effects` demo: an unread message plays its
//! effect once when it shows, a click on a message's effect badge plays the
//! effect again, and the send menu's picker (strip, card, full list, search,
//! sending). Nothing is sent: the demo session has no sender, so a send is
//! checked by the options it would carry.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::send_effects::demo::{ARRIVING, CHAT, REACTION_EFFECTS, STICKER_EFFECTS};
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, point, px, size};
use quill::message_effects_picker::PREVIEW_TOGGLE_MS;
use std::time::Duration;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-message-effects");
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        // Effects play on the clock the test moves.
        view.read(cx)
            .playback
            .stickers
            .follow(cx.background_executor().clone());
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn shown(vcx: &mut VisualTestContext, id: impl Into<gpui_kit::ElementId>) -> bool {
    let id = id.into();
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn menu_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, _| app.composer_ui.send_menu.open.is_some())
}

#[gpui_kit::test]
fn a_click_on_the_badge_plays_the_effect_again(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    // Message 99 was sent with an effect and has been seen: nothing plays.
    let key = (CHAT, 99);
    assert!(shown(vcx, ("message-effect", 99u64)));
    assert!(!crate::ui::emoji_effect::message_effect_playing(key));
    vcx.update(|window, cx| window.click(("message-effect", 99u64), cx));
    frames(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
}

/// A message sent with an effect arrives, unread, while Quill is in front.
fn arrive(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.update(|window, _| window.activate_window());
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            crate::ui::send_effects::demo::arrive(session);
        }
        cx.notify();
    });
    frames(vcx);
}

fn let_the_history_settle(vcx: &mut VisualTestContext) {
    vcx.update(|window, _| window.activate_window());
    let _ = crate::ui::frame_clock::frames_over(vcx, 5);
}

fn unwatched(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, key: (i64, i64)) -> bool {
    app.update(vcx, |app, _| {
        app.session()
            .is_some_and(|session| session.stickers.effects.watch.is_unwatched(key))
    })
}

/// Let whatever plays run out (the test sticker never decodes, so an effect
/// waits for it until its wait is over), then draw everything again.
fn let_effects_end(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.executor().advance_clock(Duration::from_secs(61));
    frames(vcx);
    app.update(vcx, |_, cx| cx.notify());
    frames(vcx);
}

#[gpui_kit::test]
fn an_unread_message_plays_its_effect_once_when_it_shows(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let key = (CHAT, ARRIVING);
    arrive(&app, vcx);
    assert!(shown(vcx, ("message-effect", ARRIVING as u64)));
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
    assert!(!unwatched(&app, vcx, key));
    // Shown again (the history drawn anew, as after scrolling back): it
    // does not play by itself a second time.
    let_effects_end(&app, vcx);
    assert!(shown(vcx, ("message-effect", ARRIVING as u64)));
    assert!(!crate::ui::emoji_effect::message_effect_playing(key));
    // A tap on its badge plays it again.
    vcx.update(|window, cx| window.click(("message-effect", ARRIVING as u64), cx));
    frames(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
}

/// An effect plays over the history: its ticks render `QuillApp`, which
/// draws it, and every slice replays.
#[gpui_kit::test]
fn the_ticks_of_an_effect_leave_the_slices_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let key = (CHAT, ARRIVING);
    let_the_history_settle(vcx);
    arrive(&app, vcx);
    // The arrival's own redraws settle in the first second; the effect
    // plays on, on the test clock.
    let _ = crate::ui::frame_clock::frames_over(vcx, 1);
    let drawn = crate::ui::frame_clock::frames_over(vcx, 1);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
    assert!(drawn.get("app").is_some_and(|app| *app > 5), "{drawn:?}");
    for slice in ["sidebar", "conversation", "composer"] {
        assert_eq!(drawn.get(slice), None, "{drawn:?}");
    }
}

/// A message sent with an effect that arrives behind another app keeps its
/// effect for when the window is activated (tdesktop `markingContentsRead`)
/// and draws nothing meanwhile. Its badge asked for a frame on every paint
/// to start the effect, and with nothing marking it seen in an inactive
/// window that went on at the display's rate.
#[gpui_kit::test]
fn an_effect_that_arrives_behind_another_app_waits_without_drawing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let key = (CHAT, ARRIVING);
    vcx.deactivate_window();
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            crate::ui::send_effects::demo::arrive(session);
        }
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(shown(vcx, ("message-effect", ARRIVING as u64)));
    // Let the arrival settle: a scrollbar a scroll shows hides 2 s later.
    let _ = crate::ui::frame_clock::frames_over(vcx, 5);
    let drawn = crate::ui::frame_clock::frames_over(vcx, 60);
    assert!(drawn.is_empty(), "drawn behind another app: {drawn:?}");
    assert!(unwatched(&app, vcx, key), "not seen yet");

    vcx.update(|window, _| window.activate_window());
    frames(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
    assert!(!unwatched(&app, vcx, key));
}

#[gpui_kit::test]
fn power_saving_skips_the_effect_on_arrival_but_a_tap_plays_it(cx: &mut TestAppContext) {
    quill::power_saving::isolate_for_tests();
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    // After the demo applied its settings.
    quill::power_saving::set(quill::power_saving::Flag::ChatEffects.bit());
    let key = (CHAT, ARRIVING);
    arrive(&app, vcx);
    assert!(!crate::ui::emoji_effect::message_effect_playing(key));
    // Marked as played all the same (tdesktop `playEffectOnRead`).
    assert!(!unwatched(&app, vcx, key));
    vcx.update(|window, cx| window.click(("message-effect", ARRIVING as u64), cx));
    frames(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
    quill::power_saving::set(0);
}

#[gpui_kit::test]
fn the_badge_effect_waits_on_the_executor_clock_not_the_wall_clock(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let key = (CHAT, 99);
    vcx.update(|window, cx| window.click(("message-effect", 99u64), cx));
    frames(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
    // The badge stays on screen: simulated time passing, well past the
    // 400 ms grace a missing badge gets, does not end the effect.
    vcx.executor().advance_clock(Duration::from_millis(1500));
    frames(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
}

#[gpui_kit::test]
fn the_send_menu_offers_effects_and_sends_with_the_chosen_one(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let (fire, _, _) = REACTION_EFFECTS[0];

    vcx.update(|window, cx| window.right_click("composer-send-wrap", cx));
    frames(vcx);
    assert!(menu_open(&app, vcx));
    assert!(shown(vcx, "send-effects-strip"));
    assert!(shown(vcx, ("send-effect", fire as u64)));
    // Nine effects do not fit the menu's width: the last cell expands.
    assert!(shown(vcx, "send-effects-expand"));

    // Choosing one shows its card, the menu staying open under it.
    vcx.update(|window, cx| window.click(("send-effect", fire as u64), cx));
    frames(vcx);
    assert!(menu_open(&app, vcx));
    assert!(shown(vcx, ("send-effect-card", fire as u64)));
    assert!(shown(vcx, "send-with-effect"));

    // The send carries the effect once, then the menu closes.
    app.update(vcx, |app, _| {
        app.composer_ui.send_effect_once = fire;
        assert_eq!(app.composer_send_options().effect_id, fire);
        app.composer_ui.send_effect_once = 0;
    });
    vcx.update(|window, cx| window.click("send-with-effect", cx));
    frames(vcx);
    assert!(!menu_open(&app, vcx));
    app.update(vcx, |app, _| {
        assert_eq!(app.composer_ui.send_effect_once, 0);
        assert_eq!(app.composer_send_options().effect_id, 0);
    });
}

#[gpui_kit::test]
fn a_click_on_the_card_hides_it_and_one_outside_closes_the_menu(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let (party, _, _) = REACTION_EFFECTS[3];
    vcx.update(|window, cx| window.right_click("composer-send-wrap", cx));
    frames(vcx);
    vcx.update(|window, cx| window.click(("send-effect", party as u64), cx));
    frames(vcx);
    let card = ("send-effect-card", party as u64);
    vcx.update(|window, cx| window.click_at(card, point(px(100.), px(40.)), cx));
    frames(vcx);
    app.update(vcx, |app, _| {
        let open = app.composer_ui.send_menu.open.as_ref().expect("still open");
        assert!(open.preview.as_ref().is_some_and(|p| p.hiding.is_some()));
    });
    // A hiding card takes no presses.
    assert!(!shown(vcx, card));
    vcx.simulate_click(point(px(300.), px(300.)), Modifiers::none());
    frames(vcx);
    assert!(!menu_open(&app, vcx));
}

#[gpui_kit::test]
fn a_hidden_card_is_dropped_once_its_fade_has_run(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let (party, _, _) = REACTION_EFFECTS[3];
    vcx.update(|window, cx| window.right_click("composer-send-wrap", cx));
    frames(vcx);
    vcx.update(|window, cx| window.click(("send-effect", party as u64), cx));
    frames(vcx);
    let card = ("send-effect-card", party as u64);
    vcx.update(|window, cx| window.click_at(card, point(px(100.), px(40.)), cx));
    frames(vcx);
    // The fade runs on the executor's clock: it has not moved, so the card
    // is still drawn, however long the machine took.
    app.update(vcx, |app, _| {
        assert!(
            app.composer_ui
                .send_menu
                .open
                .as_ref()
                .is_some_and(|open| { open.areas.card.get().is_some() })
        );
    });
    cx.executor()
        .advance_clock(Duration::from_millis(PREVIEW_TOGGLE_MS));
    frames(vcx);
    app.update(vcx, |app, _| {
        assert!(
            app.composer_ui
                .send_menu
                .open
                .as_ref()
                .is_some_and(|open| { open.areas.card.get().is_none() })
        );
    });
    assert!(menu_open(&app, vcx));
}

#[gpui_kit::test]
fn the_full_list_shows_every_effect_and_searches_them(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| window.right_click("composer-send-wrap", cx));
    frames(vcx);
    vcx.update(|window, cx| window.click("send-effects-expand", cx));
    frames(vcx);
    assert!(shown(vcx, "send-effects-panel"));
    for (id, _, _) in REACTION_EFFECTS.iter().chain(STICKER_EFFECTS.iter()) {
        assert!(shown(vcx, ("send-effect-all", *id as u64)), "effect {id}");
    }

    vcx.update(|window, cx| window.input("fire", cx));
    frames(vcx);
    let (fire, _, _) = REACTION_EFFECTS[0];
    let (thumbs, _, _) = REACTION_EFFECTS[1];
    assert!(shown(vcx, ("send-effect-all", fire as u64)));
    assert!(!shown(vcx, ("send-effect-all", thumbs as u64)));

    // A sticker effect opens its card from the list too.
    app.update_in(vcx, |app, window, cx| {
        app.composer_ui
            .send_menu
            .open
            .as_mut()
            .expect("open")
            .query
            .clear();
        let _ = window;
        cx.notify();
    });
    frames(vcx);
    let (sticker, _, _) = STICKER_EFFECTS[0];
    vcx.update(|window, cx| window.click(("send-effect-all", sticker as u64), cx));
    frames(vcx);
    assert!(shown(vcx, ("send-effect-card", sticker as u64)));
    // A Premium effect without Premium offers it instead of sending.
    assert!(!shown(vcx, "send-with-effect"));
}

#[gpui_kit::test]
fn nothing_to_send_means_no_effects(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_send_menu(point(px(600.), px(900.)), false, window, cx);
    });
    frames(vcx);
    assert!(menu_open(&app, vcx));
    assert!(!shown(vcx, "send-effects-strip"));
}

#[gpui_kit::test]
fn a_chat_in_its_own_window_plays_its_effects_there(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_chat_in_new_window(quill::ids::ChatId(CHAT), window, cx)
    });
    vcx.run_until_parked();
    let main = vcx.update(|window, _| window.window_handle().window_id());
    let chat_window = vcx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main)
        .expect("the chat window opened");
    let draw = |vcx: &mut VisualTestContext| {
        vcx.run_until_parked();
        for _ in 0..3 {
            vcx.update_window(chat_window, |_, window, cx| {
                window.activate_window();
                window.render_frame(cx);
            })
            .unwrap();
        }
        vcx.run_until_parked();
    };
    draw(vcx);
    let key = (CHAT, ARRIVING);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            crate::ui::send_effects::demo::arrive(session);
        }
        cx.notify();
    });
    draw(vcx);
    let badge = ("message-effect", ARRIVING as u64);
    let in_window = |vcx: &mut VisualTestContext| {
        vcx.update_window(chat_window, |_, window, _| window.try_find(badge).is_some())
            .unwrap()
    };
    assert!(in_window(vcx), "the badge shows in the chat window");
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
    assert!(!unwatched(&app, vcx, key));
    frames(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));

    vcx.executor().advance_clock(Duration::from_secs(61));
    draw(vcx);
    assert!(!crate::ui::emoji_effect::message_effect_playing(key));
    vcx.update_window(chat_window, |_, window, cx| window.click(badge, cx))
        .unwrap();
    draw(vcx);
    assert!(crate::ui::emoji_effect::message_effect_playing(key));
}

#[gpui_kit::test]
fn the_cards_schedule_sends_with_the_effect_after_the_time_is_picked(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let (fire, _, _) = REACTION_EFFECTS[0];
    app.update_in(vcx, |app, window, cx| {
        app.schedule_with_effect(fire, window, cx);
        let picker = app.composer_ui.schedule_picker.as_ref().expect("picker");
        assert!(picker.send_after);
        assert_eq!(picker.send_effect, fire);
        assert!(app.composer_ui.send_menu.open.is_none());
    });
}

//! The takeover state in a real window: driving shows the layer, real input
//! and Esc pause, "Let it continue" hands back, idle and a disconnect fade
//! it out, Stop refuses what waits, the layer never takes a click meant for
//! what is under it, and an idle window asks for no frames.
use crate::ui::agent_state::{TEST_LOCK, reset_for_test};
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::take_slice_renders;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, WindowVisibility, point, px,
    size,
};
use quill::agent::ui_driver::bridge::{self, UiCommand, UiReply};
use quill::agent::ui_driver::takeover::{self, Aim, FADE_IN, FADE_OUT, HOLD, PauseReason, Phase};
use quill::agent::{access, activity};
use quill::ids::{AccountKey, ChatId};
use std::time::Duration;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-takeover-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot
        .borrow()
        .clone()
        .unwrap_or_else(|| panic!("window opened"));
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor().advance_clock(Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn ms(duration: Duration) -> u64 {
    duration.as_millis() as u64
}

/// Access and the driver on, one agent connected, the clock pinned.
fn turn_on() -> activity::Presence {
    reset_for_test(&AccountKey::primary());
    takeover::reset_for_test();
    access::set_enabled(true);
    access::set_ui_driver(true);
    takeover::pin_clock(Some(1000));
    activity::connect("Claude Code")
}

/// An agent drove at `at` (pinned clock).
fn drive_at(at: u64) {
    takeover::pin_clock(Some(at));
    takeover::note_command("Claude Code");
}

fn pill(vcx: &mut VisualTestContext) -> bool {
    vcx.debug_bounds("agent-takeover-pill").is_some()
}

/// A pill button (by its element id) was laid out.
fn shown(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat))
}

fn clock_running(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, _| app.frame.frame_clock_running.get())
}

#[gpui_kit::test]
fn real_input_pauses_continue_hands_back_and_idle_fades(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let presence = turn_on();
    settle(vcx);
    assert!(!pill(vcx), "nothing before the first command");

    drive_at(1000);
    takeover::pin_clock(Some(1000 + ms(FADE_IN)));
    settle(vcx);
    assert!(pill(vcx));
    assert!(shown(vcx, "agent-takeover-stop"));
    assert!(matches!(takeover::current(), Phase::Driving { .. }));

    // The user clicks somewhere: the agent is paused and the tools refuse.
    vcx.simulate_click(point(px(600.), px(400.)), Modifiers::none());
    settle(vcx);
    assert!(matches!(
        takeover::current(),
        Phase::Paused {
            reason: PauseReason::Input,
            ..
        }
    ));
    assert!(
        quill::agent::ui_driver::policy::switch_refusal()
            .is_some_and(|text| text.contains("took control"))
    );
    assert!(shown(vcx, "agent-takeover-continue"));

    // "Let it continue" (itself a click, which must not pause again).
    vcx.update(|window, cx| window.click("agent-takeover-continue", cx));
    settle(vcx);
    assert!(matches!(takeover::current(), Phase::Driving { .. }));
    assert_eq!(quill::agent::ui_driver::policy::switch_refusal(), None);

    // No more commands: the hold runs out, the glow fades, then nothing.
    takeover::pin_clock(Some(1000 + ms(FADE_IN) + ms(HOLD) + 10));
    settle(vcx);
    assert!(matches!(takeover::current(), Phase::FadingOut { .. }));
    takeover::pin_clock(Some(1000 + ms(FADE_IN) + ms(HOLD) + ms(FADE_OUT) + 20));
    settle(vcx);
    assert_eq!(takeover::current(), Phase::Idle);
    assert!(!pill(vcx));

    // Driving again, then the agent disconnects: it fades from that moment.
    drive_at(20_000);
    settle(vcx);
    assert!(pill(vcx));
    drop(presence);
    settle(vcx);
    assert!(matches!(takeover::current(), Phase::FadingOut { .. }));
    takeover::pin_clock(Some(20_000 + ms(FADE_OUT) + 10));
    settle(vcx);
    assert!(!pill(vcx));
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn escape_while_driving_pauses_and_goes_nowhere_else(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _presence = turn_on();
    settle(vcx);
    let before = open_chat(&app, vcx);
    assert!(before.is_some(), "the demo opens a chat");
    drive_at(1000);
    settle(vcx);

    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert!(matches!(
        takeover::current(),
        Phase::Paused {
            reason: PauseReason::Escape,
            ..
        }
    ));
    assert_eq!(open_chat(&app, vcx), before, "Esc did not close the chat");
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn stop_refuses_what_waits_and_what_comes(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _presence = turn_on();
    drive_at(1000);
    settle(vcx);
    let queued =
        bridge::post("Claude Code", UiCommand::ReadTree).unwrap_or_else(|_| panic!("filed"));

    vcx.update(|window, cx| window.click("agent-takeover-stop", cx));
    settle(vcx);
    match queued.poll() {
        Some(UiReply::Refused(text)) => assert!(text.contains("Stop"), "{text}"),
        other => panic!("the queued command is refused, got {other:?}"),
    }
    assert!(takeover::stopped());
    assert!(matches!(takeover::current(), Phase::FadingOut { .. }));
    assert!(access::enabled() && access::ui_driver(), "access stays on");

    let later =
        bridge::post("Claude Code", UiCommand::ReadTree).unwrap_or_else(|_| panic!("filed"));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.agent_driver_tick(window, cx);
        })
    });
    settle(vcx);
    assert!(
        matches!(later.poll(), Some(UiReply::Refused(text)) if text.contains("Stop")),
        "a command after Stop is refused too"
    );
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn a_reconnect_lifts_neither_a_pause_nor_stop(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let presence = turn_on();
    drive_at(1000);
    settle(vcx);
    vcx.simulate_click(point(px(600.), px(400.)), Modifiers::none());
    settle(vcx);
    assert!(takeover::paused());

    // The agent kills its `quill mcp` and connects again: still paused,
    // and the pill is back so the user can let it continue.
    drop(presence);
    takeover::pin_clock(Some(2000));
    settle(vcx);
    takeover::pin_clock(Some(2000 + ms(FADE_OUT) + 10));
    settle(vcx);
    assert!(!pill(vcx));
    assert!(takeover::paused());
    let presence = activity::connect("Claude Code");
    takeover::pin_clock(Some(5000));
    settle(vcx);
    takeover::pin_clock(Some(5000 + ms(FADE_IN)));
    settle(vcx);
    assert!(matches!(takeover::current(), Phase::Paused { .. }));
    assert!(shown(vcx, "agent-takeover-continue"));

    // Stop, then a reconnect: still stopped.
    vcx.update(|window, cx| window.click("agent-takeover-stop", cx));
    settle(vcx);
    drop(presence);
    settle(vcx);
    let _presence = activity::connect("Claude Code");
    settle(vcx);
    assert!(takeover::stopped());
    assert!(
        quill::agent::ui_driver::policy::switch_refusal().is_some_and(|text| text.contains("Stop"))
    );
    // Only the driving switch off and on lifts it.
    access::set_ui_driver(false);
    access::set_ui_driver(true);
    assert!(!takeover::stopped());
    assert_eq!(quill::agent::ui_driver::policy::switch_refusal(), None);
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn the_layer_never_takes_a_click_meant_for_what_is_under_it(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _presence = turn_on();
    drive_at(1000);
    takeover::note_aim(Aim::Click, (60.0, 300.0));
    takeover::pin_clock(Some(1000 + ms(FADE_IN)));
    settle(vcx);
    assert!(pill(vcx));
    // Chat B's row, at its left edge: under the glow's bloom and the ghost
    // cursor's ring.
    let row = vcx.update(|window, _| window.find(("chat-row", 12u64)).bounds());
    let spot = point(row.origin.x + px(6.), row.origin.y + row.size.height / 2.);
    takeover::note_aim(Aim::Click, (spot.x.as_f32(), spot.y.as_f32()));
    settle(vcx);
    vcx.simulate_click(spot, Modifiers::none());
    settle(vcx);
    assert_eq!(
        open_chat(&app, vcx),
        Some(ChatId(12)),
        "the row got the click"
    );
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn frames_are_asked_only_while_something_moves(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _presence = turn_on();
    settle(vcx);
    settle(vcx);
    assert!(!clock_running(&app, vcx), "idle: no frames");

    // Fading in moves: the frame clock runs.
    drive_at(1000);
    takeover::pin_clock(Some(1010));
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(clock_running(&app, vcx));

    // Steady with nothing to sweep (Power Saving's interface animations
    // on): the clock stops.
    let saving = |on: bool| {
        use quill::power_saving::{self, Flag, with_flag};
        power_saving::set(with_flag(power_saving::current(), Flag::Animations, on));
    };
    saving(true);
    takeover::pin_clock(Some(1000 + ms(FADE_IN) + 10));
    settle(vcx);
    settle(vcx);
    assert!(!clock_running(&app, vcx), "still glow: no frames");
    assert!(pill(vcx));
    saving(false);
    takeover::reset_for_test();
    settle(vcx);
    settle(vcx);
    assert!(!clock_running(&app, vcx));
}

/// Whether a steady, driving takeover sweeps (keeps the frame clock going)
/// under each "Takeover effect" and saving signal.
#[gpui_kit::test]
fn the_sweep_follows_the_effect_and_the_saving_signals_live(cx: &mut TestAppContext) {
    use quill::agent::access::TakeoverEffect;
    use quill::power_saving::{self, Flag, with_flag};
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _presence = turn_on();
    drive_at(1000);
    let steady = 1000 + ms(FADE_IN) + 10;
    let sweeping = |vcx: &mut VisualTestContext, effect, saving: bool, reduce: bool| {
        access::set_takeover_effect(effect);
        power_saving::set(with_flag(power_saving::current(), Flag::Animations, saving));
        power_saving::set_os_reduce_motion(reduce);
        takeover::pin_clock(Some(steady));
        vcx.update(|window, cx| window.render_frame(cx));
        settle(vcx);
        settle(vcx);
        clock_running(&app, vcx)
    };
    use TakeoverEffect::*;
    assert!(sweeping(vcx, Automatic, false, false), "the default sweeps");
    assert!(
        !sweeping(vcx, Automatic, true, false),
        "Power Saving: still"
    );
    assert!(sweeping(vcx, Automatic, false, false), "and back, live");
    assert!(
        !sweeping(vcx, Automatic, false, true),
        "reduced motion: still"
    );
    assert!(sweeping(vcx, GlowAndSweep, true, false), "always sweeps");
    assert!(
        !sweeping(vcx, GlowAndSweep, false, true),
        "but reduced motion wins"
    );
    assert!(!sweeping(vcx, StillGlow, false, false), "never sweeps");
    // Paused, the light holds still.
    access::set_takeover_effect(Automatic);
    power_saving::set_os_reduce_motion(false);
    takeover::pause(PauseReason::Input);
    settle(vcx);
    settle(vcx);
    assert!(!clock_running(&app, vcx), "paused: no frames");
    takeover::reset_for_test();
}

#[gpui_kit::test]
fn turning_access_off_mid_drive_removes_the_takeover_at_once(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _presence = turn_on();
    drive_at(1000);
    settle(vcx);
    assert!(pill(vcx), "driving shows the takeover");

    access::set_enabled(false);
    app.update(vcx, |_, cx| cx.notify());
    vcx.update(|window, cx| window.render_frame(cx));
    // The takeover clock is pinned, so a fade could not have moved on:
    // only the gate removes the layer this frame.
    assert!(!pill(vcx), "no fade: the glow, ghost and pill are gone now");
    takeover::reset_for_test();
}

/// The takeover is process-wide, and hiding redraws for it only where it
/// shows: a redraw reaches every window that follows the app, and a hidden
/// chat window's history pass wakes the clips asleep in it.
#[gpui_kit::test]
fn hiding_a_window_redraws_it_for_the_takeover_only_where_agents_show(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    settle(vcx);
    let draws_as_it_hides = |vcx: &mut VisualTestContext| {
        vcx.simulate_visibility_change(WindowVisibility::Visible);
        vcx.run_until_parked();
        let _ = take_slice_renders();
        vcx.simulate_visibility_change(WindowVisibility::Hidden);
        vcx.run_until_parked();
        take_slice_renders().get("app").copied().unwrap_or(0)
    };

    takeover::reset_for_test();
    access::set_enabled(true);
    access::set_ui_driver(true);
    let _elsewhere = activity::connect("Claude Code");
    drive_at(1000);
    assert_eq!(draws_as_it_hides(vcx), 0, "no agents show here");

    let _presence = turn_on();
    drive_at(1000);
    assert!(draws_as_it_hides(vcx) > 0, "the takeover shows here");
    takeover::reset_for_test();
}

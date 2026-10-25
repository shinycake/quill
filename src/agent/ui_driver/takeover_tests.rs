use super::{
    Aim, FADE_IN, FADE_OUT, Facts, GLIDE, HOLD, JITTER_PX, Live, PULSE, PauseReason, Phase,
    RESUME_GRACE, RealInput, aim, command, ghost_arrival, ghost_point, moving, next_change_ms,
    observe, pauses_on, phase, press, press_pulse, resume, stop,
};
use std::time::Duration;

const ON: Live = Live {
    connected: true,
    allowed: true,
};

fn ms(duration: Duration) -> u64 {
    duration.as_millis() as u64
}

fn driving(facts: &Facts, live: Live, now: u64) -> bool {
    matches!(phase(facts, live, now), Phase::Driving { .. })
}

#[test]
fn nothing_shows_before_the_first_command() {
    assert_eq!(phase(&Facts::default(), ON, 10), Phase::Idle);
    assert_eq!(Phase::Idle.presence(10), 0.0);
}

#[test]
fn driving_lasts_while_commands_keep_coming_then_fades() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 1000);
    let shown = phase(&facts, ON, 1000);
    assert_eq!(shown.agent(), Some("Claude Code"));
    assert_eq!(shown.presence(1000), 0.0, "fades in");
    assert_eq!(shown.presence(1000 + ms(FADE_IN)), 1.0);
    // Another command within the hold keeps the same session going.
    command(&mut facts, "Claude Code", 1000 + ms(HOLD) - 1);
    assert!(driving(&facts, ON, 1000 + ms(HOLD) + 100));
    let started = facts.session.as_ref().map(|s| s.started_ms);
    assert_eq!(started, Some(1000), "one session");
    // Quiet for the hold: fading out, then idle.
    let end = 1000 + 2 * ms(HOLD) - 1;
    assert!(matches!(
        phase(&facts, ON, end + 10),
        Phase::FadingOut { .. }
    ));
    assert!(phase(&facts, ON, end + ms(FADE_OUT) / 2).presence(end + ms(FADE_OUT) / 2) < 1.0);
    assert_eq!(phase(&facts, ON, end + ms(FADE_OUT)), Phase::Idle);
    // A command after the fade starts a new session that fades in again.
    command(&mut facts, "Claude Code", end + ms(FADE_OUT) + 5);
    assert_eq!(
        facts.session.as_ref().map(|s| s.started_ms),
        Some(end + ms(FADE_OUT) + 5)
    );
}

#[test]
fn real_input_pauses_and_let_it_continue_hands_back() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 100);
    let shown = phase(&facts, ON, 200);
    for input in [RealInput::Press, RealInput::Scroll, RealInput::Key] {
        assert!(pauses_on(&facts, &shown, input, true, 200));
        assert!(pauses_on(&facts, &shown, input, false, 200), "{input:?}");
    }
    facts.paused = Some(PauseReason::Input);
    // Paused holds past the hold: the agent waits for the user.
    let later = 100 + ms(HOLD) * 10;
    assert!(matches!(
        phase(&facts, ON, later),
        Phase::Paused {
            reason: PauseReason::Input,
            ..
        }
    ));
    assert!(!pauses_on(
        &facts,
        &phase(&facts, ON, later),
        RealInput::Key,
        true,
        later
    ));
    resume(&mut facts, later);
    assert!(driving(&facts, ON, later + 10), "driving again");
    assert!(matches!(
        phase(&facts, ON, later + ms(HOLD) + 1),
        Phase::FadingOut { .. }
    ));
}

#[test]
fn pointer_moves_pause_only_when_deliberate() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 100);
    let shown = phase(&facts, ON, 150);
    let far = RealInput::Move { x: 50.0, y: 50.0 };
    assert!(!pauses_on(&facts, &shown, far, true, 150), "no baseline yet");
    facts.last_pointer = Some((10.0, 10.0));
    assert!(pauses_on(&facts, &shown, far, true, 150));
    assert!(
        !pauses_on(&facts, &shown, far, false, 150),
        "crossing a window behind another app"
    );
    let jitter = RealInput::Move {
        x: 10.0 + JITTER_PX / 2.0,
        y: 10.0,
    };
    assert!(!pauses_on(&facts, &shown, jitter, true, 150));
    // Right after "Let it continue" the pointer leaves the button.
    resume(&mut facts, 150);
    let shown = phase(&facts, ON, 200);
    assert!(!pauses_on(&facts, &shown, far, true, 200));
    let settled = 150 + ms(RESUME_GRACE);
    assert!(pauses_on(
        &facts,
        &phase(&facts, ON, settled),
        far,
        true,
        settled
    ));
    // Nothing pauses while the glow fades or nothing drives.
    let gone = 150 + ms(HOLD) + 10;
    assert!(!pauses_on(
        &facts,
        &phase(&facts, ON, gone),
        RealInput::Key,
        true,
        gone
    ));
}

#[test]
fn stop_ends_the_session_now_and_holds_until_the_agents_leave() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 100);
    stop(&mut facts, 500);
    assert!(facts.stopped);
    assert!(matches!(
        phase(&facts, ON, 510),
        Phase::FadingOut { ended_ms: 500, .. }
    ));
    assert_eq!(phase(&facts, ON, 500 + ms(FADE_OUT)), Phase::Idle);
    observe(&mut facts, ON, 2000);
    assert!(facts.stopped, "still connected: Stop holds");
    observe(
        &mut facts,
        Live {
            connected: false,
            allowed: true,
        },
        3000,
    );
    assert!(!facts.stopped, "a new connection may drive");
}

#[test]
fn an_interrupted_fade_in_carries_on_from_where_it_got_to() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 0);
    let half = ms(FADE_IN) / 2;
    let reached = phase(&facts, ON, half).presence(half);
    // Paused halfway: the fade-in keeps going, it does not jump to full.
    facts.paused = Some(PauseReason::Escape);
    let paused = phase(&facts, ON, half);
    assert_eq!(paused.presence(half), reached);
    assert!(moving(&paused, half), "still fading in");
    assert!(!moving(&paused, ms(FADE_IN)));
    // Stopped halfway: the fade-out starts at the value reached.
    facts.paused = None;
    stop(&mut facts, half);
    let fading = phase(&facts, ON, half);
    assert_eq!(fading.presence(half), reached);
    let later = half + ms(FADE_OUT) / 2;
    assert!(fading.presence(later) < reached);
}

#[test]
fn a_disconnect_fades_out_from_the_moment_it_is_seen() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 100);
    facts.paused = Some(PauseReason::Escape);
    let gone = Live {
        connected: false,
        allowed: true,
    };
    observe(&mut facts, gone, 300);
    assert_eq!(facts.paused, None, "nobody left to wait");
    assert!(matches!(
        phase(&facts, gone, 310),
        Phase::FadingOut { ended_ms: 300, .. }
    ));
    assert_eq!(phase(&facts, gone, 300 + ms(FADE_OUT)), Phase::Idle);
    // The switch going off does the same.
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 100);
    let off = Live {
        connected: true,
        allowed: false,
    };
    observe(&mut facts, off, 200);
    assert!(matches!(phase(&facts, off, 210), Phase::FadingOut { .. }));
}

#[test]
fn the_ghost_glides_eased_and_pulses_on_a_click() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 0);
    aim(&mut facts, Aim::Click, (100.0, 100.0), 10);
    let first = facts
        .session
        .as_ref()
        .and_then(|s| s.action)
        .unwrap_or_else(|| panic!("aimed"));
    assert_eq!(first.from, None, "the first appears where it acts");
    assert_eq!(ghost_point(&first, 10), (100.0, 100.0));
    assert!(ghost_arrival(&first, 10) < 0.1);
    assert_eq!(ghost_arrival(&first, 10 + ms(GLIDE)), 1.0);
    aim(&mut facts, Aim::Click, (300.0, 100.0), 1000);
    let second = facts
        .session
        .as_ref()
        .and_then(|s| s.action)
        .unwrap_or_else(|| panic!("aimed"));
    assert_eq!(second.from, Some((100.0, 100.0)));
    let (mid, _) = ghost_point(&second, 1000 + ms(GLIDE) / 2);
    assert!(mid > 200.0 && mid < 300.0, "ease-out is past half: {mid}");
    assert_eq!(ghost_point(&second, 1000 + ms(GLIDE)), (300.0, 100.0));
    assert_eq!(press_pulse(&second, 1100), None);
    press(&mut facts, 1300);
    let pressed = facts
        .session
        .as_ref()
        .and_then(|s| s.action)
        .unwrap_or_else(|| panic!("aimed"));
    assert_eq!(press_pulse(&pressed, 1300), Some(0.0));
    assert_eq!(press_pulse(&pressed, 1300 + ms(PULSE)), None);
}

#[test]
fn only_fades_glides_and_pulses_move_and_the_next_change_is_known() {
    let mut facts = Facts::default();
    command(&mut facts, "Claude Code", 0);
    let early = phase(&facts, ON, 10);
    assert!(moving(&early, 10), "fading in");
    let steady = phase(&facts, ON, ms(FADE_IN) + 10);
    assert!(!moving(&steady, ms(FADE_IN) + 10), "nothing moves: no frames");
    assert_eq!(next_change_ms(&steady, &facts), Some(ms(HOLD)));
    aim(&mut facts, Aim::Hover, (5.0, 5.0), 1000);
    assert!(moving(&phase(&facts, ON, 1010), 1010));
    assert!(!moving(
        &phase(&facts, ON, 1000 + ms(GLIDE)),
        1000 + ms(GLIDE)
    ));
    let fading = phase(&facts, ON, 1000 + ms(HOLD) + 1);
    assert!(moving(&fading, 1000 + ms(HOLD) + 1));
    assert_eq!(
        next_change_ms(&fading, &facts),
        Some(1000 + ms(HOLD) + ms(FADE_OUT))
    );
    assert_eq!(next_change_ms(&Phase::Idle, &facts), None);
}

#[test]
fn the_global_record_refuses_while_paused_or_stopped() {
    let _guard = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
    super::reset_for_test();
    assert_eq!(super::refusal(), None);
    super::pause(PauseReason::Input);
    assert!(super::paused());
    assert!(super::refusal().is_some_and(|text| text.contains("quill_status")));
    super::let_continue();
    assert!(!super::paused());
    super::stop_driving();
    assert!(super::stopped());
    assert!(super::refusal().is_some_and(|text| text.contains("Stop")));
    super::reset_for_test();
    assert!(!super::stopped());
}

//! The poll loops' pacing, on the test scheduler's clock.
use super::{CHROME_PACE, NextPass, PassEnd, PollPace, RestTimer, run_poll_loop};
use gpui_kit::{ActivityGuard, TestAppContext};
use quill::wake::Signal;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const PACE: PollPace = PollPace {
    gap: ms(10),
    ceiling: ms(2000),
};

const IDLE: PassEnd = PassEnd {
    work_left: false,
    due: None,
};

const BACKLOG: PassEnd = PassEnd {
    work_left: true,
    due: None,
};

/// Run a poll loop whose passes count themselves and answer `end`.
fn counted_loop(
    cx: &mut TestAppContext,
    signal: &'static Signal,
    pace: PollPace,
    end: impl FnMut(u32) -> PassEnd + 'static,
) -> Rc<Cell<u32>> {
    let passes = Rc::new(Cell::new(0));
    let counted = passes.clone();
    let mut end = end;
    let pass = move || {
        counted.set(counted.get() + 1);
        Some(end(counted.get()))
    };
    let executor = cx.background_executor.clone();
    cx.foreground_executor
        .spawn(run_poll_loop(
            executor,
            signal,
            pace,
            ActivityGuard::noop,
            pass,
        ))
        .detach();
    cx.run_until_parked();
    passes
}

/// Passes that each find work waiting, the way a flood looks to the loop.
/// A frame is asked for during every pass; returns how many frames had
/// run by the start of each pass.
fn flood(cx: &mut TestAppContext, signal: &'static Signal, end: PassEnd) -> Vec<u32> {
    const PASSES: usize = 6;
    let frames = Rc::new(Cell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let foreground = cx.foreground_executor.clone();
    let pass = {
        let seen = seen.clone();
        move || {
            seen.borrow_mut().push(frames.get());
            if seen.borrow().len() == PASSES {
                return None;
            }
            let frames = frames.clone();
            foreground
                .spawn(async move { frames.set(frames.get() + 1) })
                .detach();
            signal.poke();
            Some(end)
        }
    };
    let executor = cx.background_executor.clone();
    cx.foreground_executor
        .spawn(run_poll_loop(
            executor,
            signal,
            PACE,
            ActivityGuard::noop,
            pass,
        ))
        .detach();
    cx.run_until_parked();
    for _ in 0..PASSES {
        cx.background_executor.advance_clock(PACE.gap);
        cx.run_until_parked();
    }
    seen.take()
}

#[gpui_kit::test]
fn a_flood_of_pokes_lets_a_frame_run_between_passes(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    assert_eq!(flood(cx, &SIGNAL, IDLE), [0, 1, 2, 3, 4, 5]);
}

#[gpui_kit::test]
fn a_backlog_lets_a_frame_run_between_passes(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    assert_eq!(flood(cx, &SIGNAL, BACKLOG), [0, 1, 2, 3, 4, 5]);
}

#[gpui_kit::test]
fn a_backlog_comes_back_after_the_gap_without_a_poke(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let passes = counted_loop(cx, &SIGNAL, PACE, |_| BACKLOG);
    assert_eq!(passes.get(), 1);
    cx.background_executor.advance_clock(ms(9));
    cx.run_until_parked();
    assert_eq!(passes.get(), 1);
    cx.background_executor.advance_clock(ms(1));
    cx.run_until_parked();
    assert_eq!(passes.get(), 2);
}

#[gpui_kit::test]
fn a_backlog_holds_app_nap_off_once_until_the_loop_rests(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let (hold, state) = counted_hold();
    let passes = Cell::new(0);
    let pass = move || {
        passes.set(passes.get() + 1);
        Some(if passes.get() < 3 { BACKLOG } else { IDLE })
    };
    let executor = cx.background_executor.clone();
    cx.foreground_executor
        .spawn(run_poll_loop(executor, &SIGNAL, PACE, hold, pass))
        .detach();
    cx.run_until_parked();
    assert_eq!(state(), (1, 1));
    cx.background_executor.advance_clock(PACE.gap);
    cx.run_until_parked();
    assert_eq!(state(), (1, 1));
    cx.background_executor.advance_clock(PACE.gap);
    cx.run_until_parked();
    assert_eq!(state(), (1, 0));
}

/// Counts holds begun and holds still held.
fn counted_hold() -> (impl Fn() -> ActivityGuard, impl Fn() -> (usize, usize)) {
    let begun = Arc::new(AtomicUsize::new(0));
    let held = Arc::new(AtomicUsize::new(0));
    let hold = {
        let (begun, held) = (begun.clone(), held.clone());
        move || {
            begun.fetch_add(1, Ordering::SeqCst);
            held.fetch_add(1, Ordering::SeqCst);
            let held = held.clone();
            ActivityGuard::new(move || {
                held.fetch_sub(1, Ordering::SeqCst);
            })
        }
    };
    let state = move || (begun.load(Ordering::SeqCst), held.load(Ordering::SeqCst));
    (hold, state)
}

/// Updates that arrive just slower than each pass drains them: a poke ends
/// every rest early, so the update it brought waits out the gap. App Nap
/// would run that gap's timer up to 200 ms late.
#[gpui_kit::test]
fn a_poke_after_a_short_rest_waits_its_gap_under_the_hold(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let (hold, state) = counted_hold();
    let passes = Rc::new(Cell::new(0));
    let pass = {
        let passes = passes.clone();
        move || {
            passes.set(passes.get() + 1);
            Some(IDLE)
        }
    };
    let executor = cx.background_executor.clone();
    cx.foreground_executor
        .spawn(run_poll_loop(executor, &SIGNAL, PACE, hold, pass))
        .detach();
    cx.run_until_parked();
    assert_eq!((passes.get(), state()), (1, (0, 0)), "at rest, no hold");
    cx.background_executor.advance_clock(ms(3));
    SIGNAL.poke();
    cx.run_until_parked();
    assert_eq!((passes.get(), state()), (1, (1, 1)), "the gap runs held");
    cx.background_executor.advance_clock(ms(7));
    cx.run_until_parked();
    assert_eq!((passes.get(), state()), (2, (1, 0)), "resting again lets go");
}

/// A poke after a full gap of rest runs at once, and takes no hold.
#[gpui_kit::test]
fn a_poke_after_a_long_rest_takes_no_hold(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let (hold, state) = counted_hold();
    let passes = Rc::new(Cell::new(0));
    let pass = {
        let passes = passes.clone();
        move || {
            passes.set(passes.get() + 1);
            Some(IDLE)
        }
    };
    let executor = cx.background_executor.clone();
    cx.foreground_executor
        .spawn(run_poll_loop(executor, &SIGNAL, PACE, hold, pass))
        .detach();
    cx.run_until_parked();
    cx.background_executor.advance_clock(ms(500));
    SIGNAL.poke();
    cx.run_until_parked();
    assert_eq!((passes.get(), state()), (2, (0, 0)));
}

#[gpui_kit::test]
fn the_first_update_after_a_quiet_stretch_runs_at_once(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let passes = counted_loop(cx, &SIGNAL, PACE, |_| IDLE);
    cx.background_executor.advance_clock(ms(500));
    cx.run_until_parked();
    assert_eq!(passes.get(), 1);
    SIGNAL.poke();
    cx.run_until_parked();
    assert_eq!(passes.get(), 2, "no clock moved");
}

#[gpui_kit::test]
fn one_update_costs_one_pass_with_no_ramp_behind_it(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let passes = counted_loop(cx, &SIGNAL, PACE, |_| IDLE);
    SIGNAL.poke();
    cx.background_executor.advance_clock(PACE.gap);
    cx.run_until_parked();
    assert_eq!(passes.get(), 2);
    cx.background_executor.advance_clock(ms(1900));
    cx.run_until_parked();
    assert_eq!(passes.get(), 2);
}

#[gpui_kit::test]
fn pokes_inside_the_gap_run_at_most_two_passes(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let background = PollPace {
        gap: ms(40),
        ceiling: ms(2000),
    };
    let passes = counted_loop(cx, &SIGNAL, background, |_| IDLE);
    cx.background_executor.advance_clock(ms(500));
    cx.run_until_parked();
    for _ in 0..13 {
        SIGNAL.poke();
        cx.run_until_parked();
        cx.background_executor.advance_clock(ms(3));
    }
    cx.run_until_parked();
    assert_eq!(passes.get(), 2, "the one at the start and the first poke");
    cx.background_executor.advance_clock(ms(1));
    cx.run_until_parked();
    assert_eq!(passes.get(), 3, "the rest in one pass at the gap's end");
}

#[gpui_kit::test]
fn a_poke_during_a_pass_is_never_lost(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let passes = counted_loop(cx, &SIGNAL, PACE, |pass| {
        if pass == 1 {
            SIGNAL.poke();
        }
        IDLE
    });
    cx.background_executor.advance_clock(PACE.gap);
    cx.run_until_parked();
    assert_eq!(passes.get(), 2);
}

#[gpui_kit::test]
fn at_rest_the_loop_wakes_only_at_its_ceiling(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let passes = counted_loop(cx, &SIGNAL, PACE, |_| IDLE);
    for _ in 0..30 {
        cx.background_executor.advance_clock(ms(1000));
        cx.run_until_parked();
    }
    assert_eq!(passes.get(), 16, "the first, then one every 2 s for 30 s");
}

#[gpui_kit::test]
fn timed_work_ends_a_rest_on_its_due_time(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let passes = counted_loop(cx, &SIGNAL, PACE, |pass| PassEnd {
        work_left: false,
        due: (pass == 2).then_some(ms(400)),
    });
    cx.background_executor.advance_clock(ms(100));
    cx.run_until_parked();
    SIGNAL.poke();
    cx.run_until_parked();
    assert_eq!(passes.get(), 2);
    cx.background_executor.advance_clock(ms(399));
    cx.run_until_parked();
    assert_eq!(passes.get(), 2, "not before it is due");
    cx.background_executor.advance_clock(ms(1));
    cx.run_until_parked();
    assert_eq!(passes.get(), 3);
}

#[gpui_kit::test]
fn a_timer_a_poke_left_pending_serves_the_next_rest(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let ended = Rc::new(Cell::new(0));
    let (executor, counted) = (cx.background_executor.clone(), ended.clone());
    cx.foreground_executor
        .spawn(async move {
            let mut timer = RestTimer::default();
            let seen = SIGNAL.epoch();
            timer.rest(&executor, &SIGNAL, seen, ms(2000)).await;
            counted.set(1);
            let seen = SIGNAL.epoch();
            timer.rest(&executor, &SIGNAL, seen, ms(2000)).await;
            counted.set(2);
        })
        .detach();
    cx.run_until_parked();
    cx.background_executor.advance_clock(ms(1000));
    SIGNAL.poke();
    cx.run_until_parked();
    assert_eq!(ended.get(), 1);
    cx.background_executor.advance_clock(ms(1000));
    cx.run_until_parked();
    assert_eq!(ended.get(), 2, "ended by the first timer, not a new one");
}

#[test]
fn a_pass_with_work_waiting_gives_the_thread_back_for_the_gap() {
    assert_eq!(PACE.next(BACKLOG, false), NextPass::Yield(PACE.gap));
    assert_eq!(PACE.next(IDLE, true), NextPass::Yield(PACE.gap));
}

#[test]
fn a_quiet_pass_rests_until_the_sooner_of_its_due_time_and_the_ceiling() {
    let due = |due| PassEnd {
        work_left: false,
        due: Some(due),
    };
    assert_eq!(PACE.next(IDLE, false), NextPass::Rest(PACE.ceiling));
    assert_eq!(PACE.next(due(ms(300)), false), NextPass::Rest(ms(300)));
    assert_eq!(
        PACE.next(due(ms(9000)), false),
        NextPass::Rest(PACE.ceiling)
    );
    assert_eq!(PACE.next(due(ms(0)), false), NextPass::Rest(PACE.gap));
}

/// The tray and icon badge recount on each pass. Every applied update batch
/// pokes `wake::CHROME`, so a busy account pokes many times a second: the
/// first poke still syncs at once, the rest at most four times a second.
#[gpui_kit::test]
fn a_burst_of_chrome_pokes_syncs_the_badge_four_times_a_second(cx: &mut TestAppContext) {
    static SIGNAL: Signal = Signal::new();
    let passes = counted_loop(cx, &SIGNAL, CHROME_PACE, |_| IDLE);
    cx.background_executor.advance_clock(ms(5000));
    cx.run_until_parked();
    assert_eq!(passes.get(), 1);
    SIGNAL.poke();
    cx.run_until_parked();
    assert_eq!(passes.get(), 2, "the first update syncs at once");
    for _ in 0..20 {
        cx.background_executor.advance_clock(ms(50));
        SIGNAL.poke();
        cx.run_until_parked();
    }
    assert_eq!(passes.get(), 6, "one second of pokes: four more syncs");
    cx.background_executor.advance_clock(ms(250));
    cx.run_until_parked();
    assert_eq!(passes.get(), 7, "the last poke still lands");
    cx.background_executor.advance_clock(ms(2000));
    cx.run_until_parked();
    assert_eq!(passes.get(), 7, "then it rests");
}

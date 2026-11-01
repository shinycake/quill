//! Closing clients in the window (`closing`): a client that never answers
//! `close` never holds up the background poll and is waited for until it
//! is Closed, work waiting on a close runs only then, and quitting closes
//! every client together within one deadline.
use super::closing::{CloseWait, closed};
use super::tests::new_app;
use crate::ui::app::QuillApp;
use gpui_kit::TestAppContext;
use quill::connect::{CLOSE_GRACE, Closable, CloseProgress, QUIT_CLOSE_DEADLINE};
use quill::ids::AccountKey;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const CLOSED: u8 = 1;
const LOST: u8 = 2;

/// A client whose `close` is answered only when the test says so.
#[derive(Clone, Default)]
pub(super) struct Fake {
    close_sent: Arc<AtomicUsize>,
    progress: Arc<AtomicU8>,
    finished: Arc<AtomicBool>,
    longest_drain_micros: Arc<AtomicU64>,
    drains: Arc<AtomicU64>,
}

impl Closable for Fake {
    fn begin_close(&mut self) {
        self.close_sent.fetch_add(1, Ordering::SeqCst);
    }

    fn drain(&mut self, budget: Duration) -> CloseProgress {
        self.drains.fetch_add(1, Ordering::SeqCst);
        let micros = u64::try_from(budget.as_micros()).unwrap_or(u64::MAX);
        self.longest_drain_micros
            .fetch_max(micros, Ordering::SeqCst);
        match self.progress.load(Ordering::SeqCst) {
            CLOSED => CloseProgress::Closed,
            LOST => CloseProgress::Lost,
            _ => CloseProgress::Pending,
        }
    }

    fn finish(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
    }
}

impl Fake {
    /// The longest a drain was allowed to block: how long a poll could hold
    /// the window up, without timing the (loaded) test machine.
    pub(super) fn longest_drain(&self) -> Duration {
        Duration::from_micros(self.longest_drain_micros.load(Ordering::SeqCst))
    }

    /// Whether a drain happened at all, so a budget assert is not vacuous.
    pub(super) fn was_drained(&self) -> bool {
        self.drains.load(Ordering::SeqCst) > 0
    }

    pub(super) fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    /// TDLib answers `authorizationStateClosed`.
    pub(super) fn close(&self) {
        self.progress.store(CLOSED, Ordering::SeqCst);
    }

    /// The receive loop dies before Closed arrives.
    pub(super) fn lose(&self) {
        self.progress.store(LOST, Ordering::SeqCst);
    }

    pub(super) fn boxed(&self) -> Box<dyn Closable> {
        Box::new(self.clone())
    }
}

pub(super) fn account(n: u32) -> AccountKey {
    AccountKey::new(&format!("account-{n}")).unwrap()
}

/// Let timers and spawned work run for `ms` of test time.
pub(super) fn run_for(vcx: &mut gpui_kit::VisualTestContext, ms: u64) {
    for _ in 0..(ms / 20).max(1) {
        vcx.executor().advance_clock(Duration::from_millis(20));
        vcx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_client_that_never_answers_close_does_not_hold_the_poll(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(account(1), fake.boxed(), false);
        app.poll_background(cx);
        assert!(fake.was_drained(), "the poll drained the fake");
        assert!(
            fake.longest_drain() < Duration::from_millis(500),
            "poll could block for {:?}",
            fake.longest_drain()
        );
        assert!(app.background.closing.contains(&account(1)));
        assert!(app.background.has_work(), "the poll keeps draining it");
    });
    assert_eq!(fake.close_sent.load(Ordering::SeqCst), 1);
    assert!(!fake.finished());
    // Closed arrives: the next poll finishes and drops it.
    fake.close();
    app.update_in(&mut vcx, |app, _, cx| {
        app.poll_background(cx);
        assert!(app.background.closing.is_empty());
    });
    assert!(fake.finished());
}

#[gpui_kit::test]
fn a_slow_close_is_waited_for_not_dropped_at_a_deadline(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, _| {
        app.background.closing.push(account(1), fake.boxed(), false);
        let later = Instant::now() + CLOSE_GRACE * 20;
        assert!(app.background.closing.poll(later).is_empty());
        assert!(app.background.closing.may_be_open(&account(1)));
        assert_eq!(app.open_wait(&account(1)), CloseWait::Wait);
    });
    assert!(!fake.finished());
}

#[gpui_kit::test]
fn a_dead_receive_loop_keeps_the_account_closed_for_the_run(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(account(1), fake.boxed(), true);
        fake.lose();
        app.poll_background(cx);
        assert!(app.background.closing.is_empty());
        assert!(app.background.closing.may_be_open(&account(1)));
        assert_eq!(app.open_wait(&account(1)), CloseWait::Never);
    });
    assert!(fake.finished());
}

#[gpui_kit::test]
fn work_waiting_on_a_close_runs_once_the_client_is_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    let other = Fake::default();
    let ran = Rc::new(Cell::new(None));
    let ran_in = ran.clone();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(account(1), fake.boxed(), false);
        // Another account still closing does not hold this one up.
        app.background
            .closing
            .push(account(2), other.boxed(), false);
        app.after_closing(
            cx,
            |app| app.open_wait(&account(1)),
            move |app, go, _| {
                assert!(!app.background.closing.contains(&account(1)));
                ran_in.set(Some(go));
            },
        );
    });
    run_for(&mut vcx, 300);
    assert_eq!(ran.get(), None, "still closing");
    fake.close();
    run_for(&mut vcx, 40);
    assert_eq!(ran.get(), Some(true));
    assert!(fake.finished());
    assert!(!other.finished());
    app.read_with(&vcx, |app, _| {
        assert!(app.background.closing.contains(&account(2)));
    });
}

#[gpui_kit::test]
fn switching_to_an_account_waits_for_its_old_client_to_close(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    let opened = Rc::new(Cell::new(None));
    let opened_in = opened.clone();
    app.update_in(&mut vcx, |app, _, cx| {
        // The account's client was handed to the closing set by the switch.
        app.background.closing.push(account(1), fake.boxed(), false);
        // What `start_connection` does before it opens the database.
        QuillApp::spawn_main(cx, async move |this, cx| {
            let safe = closed(&this, &account(1), cx).await;
            opened_in.set(Some(safe));
        })
        .detach();
    });
    run_for(&mut vcx, 500);
    assert_eq!(
        opened.get(),
        None,
        "never opened while the old client closes"
    );
    fake.close();
    run_for(&mut vcx, 40);
    assert_eq!(opened.get(), Some(true));
    // A close that can't be confirmed never lets it open in this run.
    let lost = Fake::default();
    let refused = Rc::new(Cell::new(None));
    let refused_in = refused.clone();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(account(2), lost.boxed(), false);
        QuillApp::spawn_main(cx, async move |this, cx| {
            refused_in.set(Some(closed(&this, &account(2), cx).await));
        })
        .detach();
    });
    lost.lose();
    run_for(&mut vcx, 40);
    assert_eq!(refused.get(), Some(false));
}

#[gpui_kit::test]
fn stopping_every_account_never_waits_cancels_restarts_and_keeps_draining(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(account(1), fake.boxed(), true);
        app.stop_background_accounts(cx);
        assert!(app.background.closing.contains(&account(1)));
        assert!(
            app.background.polling,
            "a poll of the new generation drains it"
        );
    });
    fake.close();
    // Nobody waits on it: the background poll alone finishes it.
    run_for(&mut vcx, 600);
    assert!(fake.was_drained(), "the background poll drained the fake");
    assert!(fake.longest_drain() < Duration::from_millis(500));
    assert!(fake.finished());
    app.read_with(&vcx, |app, _| {
        assert!(app.background.closing.is_empty());
        assert!(app.background.clients.is_empty(), "no restart");
    });
}

#[gpui_kit::test]
fn quitting_closes_every_client_together_within_one_deadline(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fakes: Vec<Fake> = (0..3).map(|_| Fake::default()).collect();
    let shared = Duration::from_millis(300);
    app.update_in(&mut vcx, |app, _, _| {
        for (n, fake) in (1..).zip(&fakes) {
            app.background.closing.push(account(n), fake.boxed(), false);
        }
        let started = Instant::now();
        app.close_all_for_quit(started + shared);
        let took = started.elapsed();
        assert!(took >= shared, "waited for them: {took:?}");
        // One deadline, not one each (3 x 300 ms); generous for loaded CI.
        assert!(took < shared * 3, "one deadline, not one each: {took:?}");
        assert!(app.background.closing.is_empty());
    });
    assert!(fakes.iter().all(Fake::finished));
}

#[gpui_kit::test]
fn the_quit_observer_closes_clients_within_the_quit_deadline(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fakes: Vec<Fake> = (0..3).map(|_| Fake::default()).collect();
    app.update_in(&mut vcx, |app, _, cx| {
        app.watch_quit_close(cx);
        app.watch_quit_close(cx);
        for (n, fake) in (1..).zip(&fakes) {
            app.background.closing.push(account(n), fake.boxed(), false);
        }
    });
    let started = Instant::now();
    cx.quit();
    let took = started.elapsed();
    assert!(fakes.iter().all(Fake::finished));
    assert!(took < QUIT_CLOSE_DEADLINE * 3, "took {took:?}");
}

#[gpui_kit::test]
fn an_account_a_closed_window_left_unconfirmed_stays_shut_in_the_next(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let lost = Fake::default();
    lost.lose();
    app.update_in(&mut vcx, |app, _, _| {
        app.background.closing.push(account(1), lost.boxed(), false);
    });
    vcx.update(|window, _| window.remove_window());
    drop(app);
    // GPUI drops released entities when it next flushes its effects.
    vcx.cx.update(|_| {});
    assert!(lost.finished(), "the old app is gone");
    let (next, mut vcx) = new_app(cx);
    let wait = next.update_in(&mut vcx, |app, _, _| app.open_wait(&account(1)));
    assert_eq!(wait, CloseWait::Never);
}

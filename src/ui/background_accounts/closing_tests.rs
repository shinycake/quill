//! Closing clients in the window (`closing`): a client that never answers
//! `close` never holds up the background poll, work waiting on a close runs
//! once the client is Closed (or out of time), and quitting closes every
//! client together within one deadline.
use super::tests::new_app;
use gpui_kit::TestAppContext;
use quill::connect::{Closable, QUIT_CLOSE_DEADLINE, deadline_after};
use quill::ids::AccountKey;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A client whose `close` is answered only when the test says so.
#[derive(Clone, Default)]
struct Fake {
    close_sent: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

impl Closable for Fake {
    fn begin_close(&mut self) {
        self.close_sent.fetch_add(1, Ordering::SeqCst);
    }

    fn drain_closed(&mut self, _budget: Duration) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn finish(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
    }
}

impl Fake {
    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

fn account(n: u32) -> AccountKey {
    AccountKey::new(&format!("account-{n}")).unwrap()
}

#[gpui_kit::test]
fn a_client_that_never_answers_close_does_not_hold_the_poll(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background.closing.push(
            account(1),
            Box::new(fake.clone()),
            false,
            deadline_after(Duration::from_secs(5)),
        );
        let started = Instant::now();
        app.poll_background(cx);
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "poll took {:?}",
            started.elapsed()
        );
        assert!(app.background.closing.contains(&account(1)));
        assert!(app.background.has_work(), "the poll keeps draining it");
    });
    assert_eq!(fake.close_sent.load(Ordering::SeqCst), 1);
    assert!(!fake.finished());
    // Closed arrives: the next poll finishes and drops it.
    fake.closed.store(true, Ordering::SeqCst);
    app.update_in(&mut vcx, |app, _, cx| {
        app.poll_background(cx);
        assert!(app.background.closing.is_empty());
    });
    assert!(fake.finished());
}

#[gpui_kit::test]
fn a_client_past_its_deadline_is_dropped_without_waiting(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, cx| {
        app.background
            .closing
            .push(account(1), Box::new(fake.clone()), false, Instant::now());
        let started = Instant::now();
        app.poll_background(cx);
        assert!(started.elapsed() < Duration::from_millis(50));
        assert!(app.background.closing.is_empty());
    });
    assert!(fake.finished());
}

#[gpui_kit::test]
fn work_waiting_on_a_close_runs_once_the_client_is_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    let other = Fake::default();
    let ran = Rc::new(Cell::new(false));
    let ran_in = ran.clone();
    app.update_in(&mut vcx, |app, _, cx| {
        let far = deadline_after(Duration::from_secs(60));
        app.background
            .closing
            .push(account(1), Box::new(fake.clone()), false, far);
        // Another account still closing does not hold this one up.
        app.background
            .closing
            .push(account(2), Box::new(other.clone()), false, far);
        let waiting = account(1);
        app.after_closing(
            cx,
            move |set| !set.contains(&waiting),
            move |app, _| {
                assert!(!app.background.closing.contains(&account(1)));
                ran_in.set(true);
            },
        );
    });
    for _ in 0..5 {
        vcx.executor().advance_clock(Duration::from_millis(50));
        vcx.run_until_parked();
    }
    assert!(!ran.get(), "still closing");
    fake.closed.store(true, Ordering::SeqCst);
    vcx.executor().advance_clock(Duration::from_millis(50));
    vcx.run_until_parked();
    assert!(ran.get());
    assert!(fake.finished());
    assert!(!other.finished());
    app.read_with(&vcx, |app, _| {
        assert!(app.background.closing.contains(&account(2)));
    });
}

#[gpui_kit::test]
fn stopping_every_account_never_waits_and_cancels_restarts(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fake = Fake::default();
    app.update_in(&mut vcx, |app, _, _| {
        app.background.closing.push(
            account(1),
            Box::new(fake.clone()),
            true,
            deadline_after(Duration::from_secs(60)),
        );
        let started = Instant::now();
        app.stop_background_accounts();
        assert!(started.elapsed() < Duration::from_millis(50));
        assert!(app.background.closing.contains(&account(1)));
    });
    fake.closed.store(true, Ordering::SeqCst);
    app.update_in(&mut vcx, |app, _, _| {
        let done = app.background.closing.poll(Instant::now());
        assert_eq!(done.len(), 1);
        assert!(done.iter().all(|closed| !closed.restart));
    });
}

#[gpui_kit::test]
fn quitting_closes_every_client_together_within_one_deadline(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let fakes: Vec<Fake> = (0..3).map(|_| Fake::default()).collect();
    let shared = Duration::from_millis(300);
    app.update_in(&mut vcx, |app, _, _| {
        for (n, fake) in (1..).zip(&fakes) {
            app.background.closing.push(
                account(n),
                Box::new(fake.clone()),
                false,
                deadline_after(Duration::from_secs(5)),
            );
        }
        let started = Instant::now();
        app.close_all_for_quit(started + shared);
        let took = started.elapsed();
        assert!(took >= shared, "waited for them: {took:?}");
        assert!(took < shared * 2, "one deadline, not one each: {took:?}");
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
            app.background.closing.push(
                account(n),
                Box::new(fake.clone()),
                false,
                deadline_after(Duration::from_secs(5)),
            );
        }
    });
    let started = Instant::now();
    cx.quit();
    let took = started.elapsed();
    assert!(fakes.iter().all(Fake::finished));
    assert!(took < QUIT_CLOSE_DEADLINE * 2, "took {took:?}");
}

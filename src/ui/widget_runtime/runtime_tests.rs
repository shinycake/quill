//! The runtime thread tells the UI how it ended.
use super::{Outbox, RuntimeEvent, run_told};
use std::sync::Mutex;

#[test]
fn a_panic_on_the_runtime_thread_is_told_as_a_failure() {
    let outbox = Mutex::new(Outbox::default());
    run_told(&outbox, || panic!("a display list the painter cannot take"));
    let events = std::mem::take(&mut outbox.lock().unwrap().events);
    assert!(
        matches!(events.as_slice(), [RuntimeEvent::Failed(_)]),
        "the entry leaves Loading or Running"
    );
}

#[test]
fn a_runtime_that_stops_on_its_own_tells_nothing_more() {
    let outbox = Mutex::new(Outbox::default());
    run_told(&outbox, || {});
    assert!(outbox.lock().unwrap().events.is_empty());
}

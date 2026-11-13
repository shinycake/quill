//! The time limit of a "where am I" lookup does not run while the system's
//! permission prompt is open.
use super::geolocation::{PromptGate, channel};
use super::{LOCATE_TIMEOUT, poll_answer_unless};
use gpui_kit::TestAppContext;
use quill::location_picker::GeoOutcome;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

type Landed = Rc<RefCell<Option<Option<GeoOutcome>>>>;

fn wait(
    cx: &mut TestAppContext,
    prompt: &Arc<PromptGate>,
) -> (
    super::geolocation::Answer<GeoOutcome>,
    Landed,
    gpui_kit::Task<()>,
) {
    let (answer, mut receiver) = channel::<GeoOutcome>();
    let landed: Landed = Rc::default();
    let task = cx.update(|app| {
        let (prompt, landed) = (prompt.clone(), landed.clone());
        app.spawn(async move |cx| {
            let got = poll_answer_unless(&mut receiver, LOCATE_TIMEOUT, &prompt, cx).await;
            *landed.borrow_mut() = Some(got);
        })
    });
    (answer, landed, task)
}

#[gpui_kit::test]
fn a_fix_after_a_long_prompt_still_lands(cx: &mut TestAppContext) {
    let prompt = Arc::new(PromptGate::default());
    prompt.set_open(true);
    let (answer, landed, _task) = wait(cx, &prompt);
    cx.executor().advance_clock(Duration::from_secs(25));
    cx.run_until_parked();
    // Allow: the full limit starts again from here.
    prompt.set_open(false);
    cx.executor().advance_clock(Duration::from_secs(7));
    cx.run_until_parked();
    assert!(landed.borrow().is_none(), "still waiting at 32 s");
    assert!(answer.send(GeoOutcome::Denied).is_ok());
    cx.run_until_parked();
    assert!(matches!(&*landed.borrow(), Some(Some(GeoOutcome::Denied))));
}

#[gpui_kit::test]
fn without_a_prompt_the_limit_ends_the_wait(cx: &mut TestAppContext) {
    let prompt = Arc::new(PromptGate::default());
    let (_answer, landed, _task) = wait(cx, &prompt);
    cx.executor().advance_clock(LOCATE_TIMEOUT);
    cx.run_until_parked();
    assert!(matches!(&*landed.borrow(), Some(None)));
}

#[gpui_kit::test]
fn a_prompt_still_open_when_the_limit_ends_keeps_the_wait(cx: &mut TestAppContext) {
    let prompt = Arc::new(PromptGate::default());
    prompt.set_open(true);
    let (_answer, landed, _task) = wait(cx, &prompt);
    cx.executor().advance_clock(LOCATE_TIMEOUT * 3);
    cx.run_until_parked();
    assert!(landed.borrow().is_none(), "the user is still deciding");
}

//! The permissions box sliders: the stops, the labels and sentence, and
//! the thumb following the value passed in while a release reports the
//! new stop.
use super::stops_slider::{
    SLOW_MODE_KEYS, StopsSlider, slow_mode_about, slow_mode_label, stop_index, value_for,
};
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    px, size,
};
use quill::permissions_box::SLOW_MODE_STOPS;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Harness {
    delay: Rc<Cell<i32>>,
    sent: Rc<RefCell<Vec<i32>>>,
    slider: Rc<RefCell<Option<Entity<SliderState>>>>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let sent = self.sent.clone();
        StopsSlider::new(
            SLOW_MODE_KEYS,
            7,
            &SLOW_MODE_STOPS,
            self.delay.get(),
            slow_mode_label,
            move |delay, _| sent.borrow_mut().push(delay),
        )
        .probe(self.slider.clone())
    }
}

type Rig = (
    VisualTestContext,
    Rc<Cell<i32>>,
    Rc<RefCell<Vec<i32>>>,
    Rc<RefCell<Option<Entity<SliderState>>>>,
);

fn harness(cx: &mut TestAppContext, delay: i32) -> Rig {
    cx.update(gpui_kit::init);
    let delay = Rc::new(Cell::new(delay));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let slider = Rc::new(RefCell::new(None));
    let (delay_in, sent_in, slider_in) = (delay.clone(), sent.clone(), slider.clone());
    let handle = cx.open_window(size(px(500.), px(200.)), move |window, cx| {
        let view = cx.new(|_| Harness {
            delay: delay_in,
            sent: sent_in,
            slider: slider_in,
        });
        Root::new(view, window, cx)
    });
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (vcx, delay, sent, slider)
}

#[test]
fn stops_snap_down() {
    let stops = &SLOW_MODE_STOPS;
    assert_eq!(stop_index(stops, 0), 0);
    assert_eq!(stop_index(stops, 30), 3);
    assert_eq!(stop_index(stops, 3600), 7);
    assert_eq!(stop_index(stops, 45), 3);
    assert_eq!(value_for(stops, 4.0), 60);
    assert_eq!(value_for(stops, 99.0), 3600);
    assert_eq!(value_for(stops, -1.0), 0);
}

#[test]
fn labels_and_sentence_follow_the_delay() {
    let labels: Vec<String> = SLOW_MODE_STOPS
        .iter()
        .map(|s| slow_mode_label(*s))
        .collect();
    assert_eq!(labels, ["Off", "5s", "10s", "30s", "1m", "5m", "15m", "1h"]);
    assert_eq!(
        slow_mode_about(0),
        "Members will be able to send only one message per this interval."
    );
    assert_eq!(
        slow_mode_about(30),
        "Members will be able to send only one message every 30 seconds."
    );
    assert_eq!(
        slow_mode_about(3600),
        "Members will be able to send only one message every 60 minutes."
    );
}

#[gpui_kit::test]
fn thumb_follows_the_server_and_release_reports_the_stop(cx: &mut TestAppContext) {
    let (mut vcx, delay, sent, slot) = harness(cx, 30);
    vcx.run_until_parked();
    let slider = slot.borrow().clone().expect("the rendered slider");
    assert_eq!(slider.read_with(&vcx, |s, _| s.value().end()), 3.0);
    // Dragging to 15m reports once, in seconds.
    slider.update(&mut vcx, |_, cx| {
        cx.emit(SliderEvent::Release(SliderValue::Single(6.0)))
    });
    assert_eq!(*sent.borrow(), vec![900]);
    // Releasing on the server's own stop reports nothing.
    slider.update(&mut vcx, |_, cx| {
        cx.emit(SliderEvent::Release(SliderValue::Single(3.0)))
    });
    assert_eq!(*sent.borrow(), vec![900]);
    // The server answers with the new delay: the thumb moves there.
    delay.set(900);
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    assert_eq!(slider.read_with(&vcx, |s, _| s.value().end()), 6.0);
}

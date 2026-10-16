//! The slow-mode slider: the stops, the labels and sentence, and the
//! thumb following the server value while a release reports the new stop.
use super::slow_mode::{
    SLOW_MODE_STOPS, SlowModeSlider, about_text, delay_for_value, stop_index, stop_label,
};
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    px, size,
};
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
        SlowModeSlider::new(7, self.delay.get(), move |delay, _| {
            sent.borrow_mut().push(delay)
        })
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
fn stops_match_tdesktop_and_snap_down() {
    assert_eq!(SLOW_MODE_STOPS, [0, 5, 10, 30, 60, 300, 900, 3600]);
    assert_eq!(stop_index(0), 0);
    assert_eq!(stop_index(30), 3);
    assert_eq!(stop_index(3600), 7);
    assert_eq!(stop_index(45), 3);
    assert_eq!(delay_for_value(4.0), 60);
    assert_eq!(delay_for_value(99.0), 3600);
    assert_eq!(delay_for_value(-1.0), 0);
}

#[test]
fn labels_and_sentence_follow_the_delay() {
    let labels: Vec<String> = SLOW_MODE_STOPS.iter().map(|s| stop_label(*s)).collect();
    assert_eq!(labels, ["Off", "5s", "10s", "30s", "1m", "5m", "15m", "1h"]);
    assert_eq!(
        about_text(0),
        "Members will be able to send only one message per this interval."
    );
    assert_eq!(
        about_text(30),
        "Members will be able to send only one message every 30 seconds."
    );
    assert_eq!(
        about_text(3600),
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

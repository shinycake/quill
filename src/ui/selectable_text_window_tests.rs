//! Each window's copy names the message its own selection covers, not the
//! one another window painted last (round 12, area B, finding 1).

use super::{SelectableRichText, selected_message_text};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, IntoElement, Modifiers, MouseButton, ParentElement, Render, SharedString,
    Styled, StyledText, TestAppContext, TextLayout, VisualTestContext, Window, div, point, px,
    size,
};
use std::cell::RefCell;
use std::rc::Rc;

struct Message {
    key: (i64, u64),
    text: SharedString,
    layout: Rc<RefCell<Option<TextLayout>>>,
}

impl Render for Message {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let styled = StyledText::new(self.text.clone());
        *self.layout.borrow_mut() = Some(styled.layout().clone());
        div()
            .w(px(300.))
            .text_size(px(14.))
            .child(SelectableRichText::new("message", self.text.clone(), styled).message(self.key))
    }
}

fn open(cx: &mut TestAppContext, key: (i64, u64), text: &str) -> VisualTestContext {
    let slot = Rc::new(RefCell::new(None));
    let handle = cx.open_window(size(px(320.), px(120.)), {
        let (slot, text) = (slot.clone(), SharedString::from(text.to_string()));
        move |window, cx| {
            let view = cx.new(|_| Message {
                key,
                text,
                layout: slot,
            });
            Root::new(view, window, cx)
        }
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    let layout = slot.borrow().clone().expect("rendered");
    let half = layout.line_height() / 2.;
    let start = layout.position_for_index(0).expect("first character");
    let end = layout.position_for_index(text.len()).expect("text end");
    let (from, to) = (point(start.x, start.y + half), point(end.x, end.y + half));
    let none = Modifiers::none();
    vcx.simulate_mouse_move(from, None, none);
    vcx.simulate_mouse_down(from, MouseButton::Left, none);
    vcx.simulate_mouse_move(to, MouseButton::Left, none);
    vcx.simulate_mouse_up(to, MouseButton::Left, none);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx
}

#[gpui_kit::test]
fn a_copy_names_the_message_selected_in_its_own_window(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut protected = open(cx, (7, 70), "protected words");
    let mut ordinary = open(cx, (9, 90), "ordinary words");
    // The ordinary window paints again last (a new message, a typing line).
    ordinary.update(|window, cx| window.render_frame(cx));
    assert_eq!(
        protected.update(selected_message_text),
        Some(((7, 70), "protected words".to_string())),
        "the protected window's copy is checked against its own chat"
    );
    protected.update(|window, cx| window.render_frame(cx));
    assert_eq!(
        ordinary.update(selected_message_text),
        Some(((9, 90), "ordinary words".to_string())),
        "and the ordinary window's copy against its own"
    );
}

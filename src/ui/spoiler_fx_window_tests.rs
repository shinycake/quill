//! Each window's frame asks for the text specks it painted itself, so a
//! chat window's spoilers keep moving while the main window is not drawn
//! (round 12, area B, the paint-state sweep).

use super::{paint_text_specks, take_text_painted};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, IntoElement, ParentElement, Render, Styled,
    TestAppContext, Window, canvas, div, hsla, px, size,
};

struct Specks {
    paint: bool,
}

impl Render for Specks {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let paint = self.paint;
        div().size_full().child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    if paint {
                        paint_text_specks(bounds, bounds.origin, hsla(0., 0., 0.5, 1.), window);
                    }
                },
            )
            .size_full(),
        )
    }
}

fn open(cx: &mut TestAppContext, paint: bool) -> AnyWindowHandle {
    let handle = cx.open_window(size(px(200.), px(100.)), move |_, _| Specks { paint });
    handle.into()
}

#[gpui_kit::test]
fn a_window_takes_only_the_specks_it_painted(cx: &mut TestAppContext) {
    let painting = open(cx, true);
    let plain = open(cx, false);
    for handle in [painting, plain] {
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .expect("the window");
    }
    let plain_took = cx
        .update_window(plain, |_, window, _| take_text_painted(window))
        .expect("the window");
    assert!(!plain_took, "the other window painted no specks");
    let painting_took = cx
        .update_window(painting, |_, window, _| take_text_painted(window))
        .expect("the window");
    assert!(
        painting_took,
        "the window that painted them asks for its tick"
    );
}

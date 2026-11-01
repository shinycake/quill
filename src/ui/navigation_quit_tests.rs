use super::after_windows_on_quit;
use gpui_kit::{Context, IntoElement, Render, TestAppContext, Window, div, px, size};
use std::cell::Cell;
use std::rc::Rc;

/// Notes when its window lets it go.
struct Shown(Rc<Cell<bool>>);

impl Render for Shown {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Drop for Shown {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[gpui_kit::test]
fn the_quit_flush_runs_once_no_window_is_left(cx: &mut TestAppContext) {
    let shown = Rc::new(Cell::new(true));
    let in_window = shown.clone();
    cx.open_window(size(px(100.), px(100.)), move |_, _| Shown(in_window));
    let window_up_at_flush = Rc::new(Cell::new(None));
    let seen = window_up_at_flush.clone();
    let at_flush = shown.clone();
    cx.update(|cx| after_windows_on_quit(cx, move || seen.set(Some(at_flush.get()))));
    cx.quit();
    assert_eq!(window_up_at_flush.get(), Some(false));
}

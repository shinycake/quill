//! A selection or a link under the pointer redraws the view that holds the
//! text: a view beside it, cached, replays, unless the selection reaches it.

use super::SelectableRichText;
use gpui_kit::base::TextSelection;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext, Context, Entity, Hsla, IntoElement, Modifiers, MouseButton, ParentElement, Pixels,
    Point, Quad, Render, SharedString, StyleRefinement, Styled, StyledText, TestAppContext,
    VisualTestContext, Window, div, hsla, point, px, size,
};
use std::cell::Cell;
use std::rc::Rc;

const COLUMN: f32 = 300.;
const LEFT: Point<Pixels> = point(px(10.), px(8.));
const LEFT_FURTHER: Point<Pixels> = point(px(120.), px(8.));
const RIGHT: Point<Pixels> = point(px(COLUMN + 120.), px(8.));

fn highlight() -> Hsla {
    hsla(0.6, 1., 0.5, 1.)
}

fn underline() -> Hsla {
    hsla(0.3, 1., 0.5, 1.)
}

struct Column {
    id: &'static str,
    order: u64,
    renders: Rc<Cell<u32>>,
}

impl Render for Column {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let text: SharedString = "a line of text long enough to drag along".into();
        let link = 0..text.len();
        div().w(px(COLUMN)).text_size(px(14.)).child(
            SelectableRichText::new(self.id, text.clone(), StyledText::new(text))
                .document_order(self.order)
                .selection_color(highlight())
                .on_click(vec![link], |_, _, _| {})
                .on_range_hover(|_, _, _| {})
                .link_underline(vec![true], underline()),
        )
    }
}

struct Page {
    left: Entity<Column>,
    right: Entity<Column>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let column = || StyleRefinement::default().w(px(COLUMN)).h(px(40.));
        div()
            .size_full()
            .flex()
            .child(self.left.clone().cached(column()))
            .child(self.right.clone().cached(column()))
    }
}

struct Open {
    vcx: VisualTestContext,
    left: Rc<Cell<u32>>,
    right: Rc<Cell<u32>>,
}

impl Open {
    fn window(cx: &mut TestAppContext) -> Self {
        cx.update(gpui_kit::init);
        let (left, right) = (Rc::<Cell<u32>>::default(), Rc::<Cell<u32>>::default());
        let (left_renders, right_renders) = (left.clone(), right.clone());
        let handle = cx.open_window(size(px(640.), px(120.)), move |window, cx| {
            let left = cx.new(|_| Column {
                id: "left",
                order: 1,
                renders: left_renders,
            });
            let right = cx.new(|_| Column {
                id: "right",
                order: 2,
                renders: right_renders,
            });
            let page = cx.new(|_| Page { left, right });
            Root::new(page, window, cx)
        });
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        vcx.run_until_parked();
        Self { vcx, left, right }
    }

    /// What `f` rendered: of the left view, and of the right one.
    fn renders(&mut self, f: impl FnOnce(&mut VisualTestContext)) -> (u32, u32) {
        let before = (self.left.get(), self.right.get());
        f(&mut self.vcx);
        self.vcx.run_until_parked();
        (self.left.get() - before.0, self.right.get() - before.1)
    }

    fn quads(&mut self, color: Hsla) -> Vec<Quad> {
        let color = gpui_kit::Background::from(color);
        let quads = self.vcx.update(|window, _| window.painted_quads());
        quads
            .into_iter()
            .filter(|q| q.background == color)
            .collect()
    }

    fn selected(&mut self) -> String {
        self.vcx.update(TextSelection::selected_text).to_string()
    }
}

fn press(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
}

fn drag(vcx: &mut VisualTestContext, to: Point<Pixels>) {
    vcx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
}

/// Crossing a link underlines it in the view that holds it; the other
/// view replays.
#[gpui_kit::test]
fn a_link_under_the_pointer_redraws_its_own_view(cx: &mut TestAppContext) {
    let mut open = Open::window(cx);
    assert!(open.quads(underline()).is_empty());
    let (left, right) = open.renders(|vcx| {
        vcx.simulate_mouse_move(LEFT, None, Modifiers::none());
    });
    assert!(left > 0, "the underline shows");
    assert_eq!(right, 0);
    assert!(!open.quads(underline()).is_empty());

    let (left, right) = open.renders(|vcx| {
        vcx.simulate_mouse_move(point(px(10.), px(100.)), None, Modifiers::none());
    });
    assert!(left > 0, "and goes");
    assert_eq!(right, 0);
    assert!(open.quads(underline()).is_empty());
}

/// The highlight follows the drag in the view it runs through, without
/// the view beside it; once it reaches that view both show it.
#[gpui_kit::test]
fn a_selection_redraws_the_views_it_runs_through(cx: &mut TestAppContext) {
    let mut open = Open::window(cx);
    press(&mut open.vcx, LEFT);
    let (_, right) = open.renders(|vcx| drag(vcx, LEFT_FURTHER));
    assert_eq!(right, 0, "the selection is all in the left view");
    let short = open.selected();
    assert!(!short.is_empty(), "the drag selects");
    let width = |quads: Vec<Quad>| quads.iter().map(|q| q.bounds.size.width.0).sum::<f32>();
    let highlighted = width(open.quads(highlight()));
    assert!(highlighted > 0.);

    let (left, right) = open.renders(|vcx| drag(vcx, point(px(200.), px(8.))));
    assert!(left > 0, "it grows");
    assert_eq!(right, 0);
    assert!(open.selected().len() > short.len());
    assert!(width(open.quads(highlight())) > highlighted);

    let (_, right) = open.renders(|vcx| drag(vcx, RIGHT));
    assert!(right > 0, "the highlight reaches the right view");
    let reached = open.quads(highlight());
    let split = COLUMN * open.vcx.update(|window, _| window.scale_factor());
    assert!(reached.iter().any(|q| q.bounds.origin.x.0 < split));
    assert!(reached.iter().any(|q| q.bounds.origin.x.0 >= split));
}

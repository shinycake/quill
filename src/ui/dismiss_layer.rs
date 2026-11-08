//! Full-window layers that close on a click outside their content.
//!
//! GPUI hit tests every hitbox under the pointer, topmost first, down to the
//! first one that occludes (`.occlude()`), and a mouse handler fires for each
//! of them. A backdrop that closes on click, drawn beside or around a panel
//! that does not occlude, therefore also closes when the panel itself is
//! clicked; and a backdrop that does not occlude lets the click, hover and
//! scroll reach whatever is drawn behind it as well.
//!
//! [`DismissLayer`] encodes the one arrangement that avoids both. The layer
//! covers the window and occludes, so nothing behind it is reached. Its
//! content is added through [`DismissLayer::content`] or
//! [`DismissLayer::anchored`], which make the content occlude too, so a
//! click on the content never reaches the backdrop. Only a click whose press
//! and release both land outside every content element dismisses it, as a
//! click outside a tdesktop layer or popup does.
//! `scripts/check-click-through.py` rejects hand-built layers of this kind.
#![deny(clippy::indexing_slicing)]

use gpui_base::FocusTrapElement;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, Stateful, StatefulInteractiveElement, Styled, Window, anchored,
    div, px,
};

/// A window-covering layer that closes when clicked outside its content.
///
/// Three tiers, bottom to top: the root (occludes everything behind; takes
/// the layout, scrim and focus tracking), a backdrop over it that takes the
/// dismissing click, and the content, which occludes the backdrop. The
/// backdrop keeps a click outside the content off the root, so a root that
/// tracks focus does not take focus from the press that closes it.
pub(super) struct DismissLayer {
    root: Stateful<Div>,
    backdrop: Stateful<Div>,
    children: Vec<AnyElement>,
}

impl DismissLayer {
    /// A layer over the whole window that calls `on_dismiss` on a click
    /// that lands outside its content.
    pub(super) fn new(
        id: impl Into<ElementId>,
        on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            root: div().id(id).occlude().absolute().inset_0(),
            backdrop: div()
                .id("dismiss-backdrop")
                .occlude()
                .absolute()
                .inset_0()
                .on_click(on_dismiss),
            children: Vec::new(),
        }
    }

    /// Style the layer itself: its scrim, how it lays out the content,
    /// focus. Content goes through [`Self::content`].
    pub(super) fn layer(mut self, style: impl FnOnce(Stateful<Div>) -> Stateful<Div>) -> Self {
        self.root = style(self.root);
        self
    }

    /// More input on the area outside the content (a right press that also
    /// dismisses).
    pub(super) fn backdrop(mut self, style: impl FnOnce(Stateful<Div>) -> Stateful<Div>) -> Self {
        self.backdrop = style(self.backdrop);
        self
    }

    /// Content laid out by the layer (centered panels, cards). It occludes,
    /// so its clicks, hover and scroll stay on it.
    pub(super) fn content<E>(mut self, content: E) -> Self
    where
        E: InteractiveElement + IntoElement,
    {
        self.children.push(content.occlude().into_any_element());
        self
    }

    /// Content at `position` in the window, kept 8 px inside its edges
    /// (menus and popups). Wrapped in an occluding box of its own size.
    pub(super) fn anchored(mut self, position: Point<Pixels>, content: impl IntoElement) -> Self {
        self.children.push(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.))
                .child(div().occlude().child(content))
                .into_any_element(),
        );
        self
    }

    /// Things drawn over the layer that take no input of their own (an
    /// animation painter). Clicks through them still dismiss.
    pub(super) fn decorations(
        mut self,
        decorations: impl IntoIterator<Item = impl IntoElement>,
    ) -> Self {
        self.children
            .extend(decorations.into_iter().map(IntoElement::into_any_element));
        self
    }

    fn build(self) -> Stateful<Div> {
        self.root.child(self.backdrop).children(self.children)
    }

    /// Finish the layer with Tab kept inside it (menus): `focus` should be
    /// tracked on the layer through [`Self::layer`].
    pub(super) fn focus_trap(self, id: impl Into<ElementId>, focus: &FocusHandle) -> AnyElement {
        self.build().focus_trap(id, focus).into_any_element()
    }
}

impl IntoElement for DismissLayer {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        self.build()
    }
}

/// The layers in the app, over the screenshot demos.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "click_through_tests.rs"]
mod click_through_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::DismissLayer;
    use gpui_kit::prelude::FluentBuilder;
    use gpui_kit::{
        Context, Entity, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement,
        Render, ScrollDelta, ScrollEvent, ScrollSource, StatefulInteractiveElement, Styled,
        TestAppContext, TouchPhase, VisualTestContext, Window, div, point, px, size,
    };

    /// A button behind, and a layer over it with a panel that holds a
    /// button of its own: centered by the layer, or anchored at 100,100.
    #[derive(Default)]
    struct Probe {
        anchored: bool,
        open: bool,
        behind_clicks: u32,
        behind_scrolls: u32,
        behind_hovered: bool,
        panel_clicks: u32,
        panel_scrolls: u32,
    }

    impl Render for Probe {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let panel = div()
                .id("panel")
                .w(px(200.))
                .h(px(100.))
                .p(px(20.))
                .on_scroll(cx.listener(|this, _, _, _| this.panel_scrolls += 1))
                .child(
                    div()
                        .id("panel-button")
                        .size(px(20.))
                        .on_click(cx.listener(|this, _, _, _| this.panel_clicks += 1)),
                );
            let anchored = self.anchored;
            div()
                .size_full()
                .child(
                    div()
                        .id("behind")
                        .size_full()
                        .on_click(cx.listener(|this, _, _, _| this.behind_clicks += 1))
                        .on_scroll(cx.listener(|this, _, _, _| this.behind_scrolls += 1))
                        .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                            this.behind_hovered |= *hovered;
                        })),
                )
                .when(self.open, |root| {
                    let layer = DismissLayer::new(
                        "layer",
                        cx.listener(|this, _, _, cx| {
                            this.open = false;
                            cx.notify();
                        }),
                    );
                    let layer = if anchored {
                        layer.anchored(point(px(100.), px(100.)), panel)
                    } else {
                        layer
                            .layer(|layer| layer.flex().items_center().justify_center())
                            .content(panel)
                    };
                    root.child(layer)
                })
        }
    }

    // A 400x300 window: either way the panel spans x 100..300, y 100..200,
    // its button x 120..140, y 120..140.
    fn open_probe(
        cx: &mut TestAppContext,
        anchored: bool,
    ) -> (Entity<Probe>, &mut VisualTestContext) {
        cx.update(gpui_kit::init);
        let (view, vcx) = cx.add_window_view(move |_, _| Probe {
            anchored,
            open: true,
            ..Probe::default()
        });
        vcx.simulate_resize(size(px(400.), px(300.)));
        vcx.run_until_parked();
        (view, vcx)
    }

    #[gpui_kit::test]
    fn clicks_on_the_content_keep_the_layer_open(cx: &mut TestAppContext) {
        for anchored in [false, true] {
            let (view, vcx) = open_probe(cx, anchored);
            vcx.simulate_click(point(px(130.), px(130.)), Modifiers::default());
            vcx.simulate_click(point(px(250.), px(180.)), Modifiers::default());
            vcx.run_until_parked();
            view.read_with(vcx, |probe, _| {
                assert!(
                    probe.open,
                    "a click on the panel or its button ({anchored})"
                );
                assert_eq!(probe.panel_clicks, 1, "the button still works ({anchored})");
                assert_eq!(probe.behind_clicks, 0);
            });
        }
    }

    #[gpui_kit::test]
    fn a_click_outside_closes_it_and_reaches_nothing_behind(cx: &mut TestAppContext) {
        for anchored in [false, true] {
            let (view, vcx) = open_probe(cx, anchored);
            vcx.simulate_click(point(px(20.), px(20.)), Modifiers::default());
            vcx.run_until_parked();
            view.read_with(vcx, |probe, _| {
                assert!(!probe.open, "the layer closes ({anchored})");
                assert_eq!(probe.behind_clicks, 0, "the button behind is not pressed");
            });
        }
    }

    #[gpui_kit::test]
    fn a_press_on_the_content_released_outside_keeps_it_open(cx: &mut TestAppContext) {
        for anchored in [false, true] {
            let (view, vcx) = open_probe(cx, anchored);
            let (inside, outside) = (point(px(250.), px(150.)), point(px(20.), px(20.)));
            vcx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::default());
            vcx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
            vcx.run_until_parked();
            view.read_with(vcx, |probe, _| {
                assert!(probe.open, "a drag out ({anchored})")
            });
        }
    }

    #[gpui_kit::test]
    fn hover_and_scroll_stay_on_the_layer(cx: &mut TestAppContext) {
        for anchored in [false, true] {
            let (view, vcx) = open_probe(cx, anchored);
            for position in [point(px(250.), px(150.)), point(px(20.), px(20.))] {
                vcx.simulate_mouse_move(position, None, Modifiers::default());
                vcx.simulate_event(ScrollEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                    source: ScrollSource::Pointer,
                });
            }
            vcx.run_until_parked();
            view.read_with(vcx, |probe, _| {
                assert_eq!(probe.panel_scrolls, 1, "the panel scrolls ({anchored})");
                assert_eq!(probe.behind_scrolls, 0, "nothing behind scrolls");
                assert!(!probe.behind_hovered, "nothing behind is hovered");
            });
        }
    }
}

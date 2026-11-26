//! Screenshot demo for rounded content masks (`Styled::clip_rounded`,
//! `Window::with_rounded_content_mask`, the vendored GPUI's
//! `scene/rounded_mask.rs`; docs/decisions/rounded-content-masks.md).
//! The Floating layout's cards and menus clip to their corners with them.
//!
//! `ready-rounded-mask` lays out one card per primitive kind, each with its
//! content running over the card's corners: a fill, a bordered square, a
//! shadow, text, emoji, an image, an icon, a rotated icon, a path, an
//! underline, glass; then a card in a card (contained, exact), a card
//! crossing its parent's rounding (the one inexact nesting), a scrolled
//! list in a card, and a list whose rows highlight on hover (move the
//! pointer over its bottom row with `QUILL_DEMO_CLICK=m:x,y`; the cut-away
//! corner does not hover it). `QUILL_DEMO_ROUNDED_MASK=off` draws the same
//! cards without `clip_rounded`, to see (and time) the frame without the
//! masks.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::assets::IconName;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::{Icon, Root};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, BoxShadow, Context, Div, Hsla, InteractiveElement, IntoElement,
    LensBend, LensDetail, LensMaterial, LensStyle, LensTune, ParentElement, PathBuilder, Render,
    Rgba, ScrollHandle, StatefulInteractiveElement, Styled, Window, canvas, div, img, point, px,
    radians,
};

register_demos![
    DemoSpec::chats(
        "ready-rounded-mask",
        "screenshot demo — rounded content masks, one card per primitive kind"
    )
    .setup(QuillApp::demo_ready_rounded_mask)
    .timing(2000, 3000)
];

impl QuillApp {
    fn demo_ready_rounded_mask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clip = std::env::var("QUILL_DEMO_ROUNDED_MASK").as_deref() != Ok("off");
        window.defer(cx, move |window, cx| show(clip, window, cx));
    }
}

/// Replaces the window's content with the demo, under a kit `Root` so the
/// capture finds one.
fn show(clip: bool, window: &mut Window, cx: &mut App) {
    let scroll = ScrollHandle::new();
    scroll.set_offset(point(px(0.0), px(-36.0)));
    let demo = cx.new(|_| MaskDemo { clip, scroll });
    window.replace_root(cx, |window, cx| Root::new(demo, window, cx));
}

struct MaskDemo {
    clip: bool,
    scroll: ScrollHandle,
}

const CARD_W: f32 = 200.0;
const CARD_H: f32 = 140.0;
const RADIUS: f32 = 28.0;
const GAP: f32 = 24.0;
const COLUMNS: usize = 5;

fn rgb(r: f32, g: f32, b: f32) -> Hsla {
    Rgba { r, g, b, a: 1.0 }.into()
}

struct Palette {
    page: Hsla,
    card: Hsla,
    text: Hsla,
    muted: Hsla,
    accent: Hsla,
    warm: Hsla,
    hover: Hsla,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                page: rgb(0.07, 0.075, 0.09),
                card: rgb(0.14, 0.15, 0.17),
                text: rgb(0.95, 0.95, 0.96),
                muted: rgb(0.6, 0.62, 0.66),
                accent: rgb(0.2, 0.55, 0.98),
                warm: rgb(0.95, 0.55, 0.2),
                hover: rgb(0.24, 0.26, 0.3),
            }
        } else {
            Self {
                page: rgb(0.92, 0.93, 0.95),
                card: rgb(1.0, 1.0, 1.0),
                text: rgb(0.08, 0.09, 0.1),
                muted: rgb(0.42, 0.44, 0.48),
                accent: rgb(0.04, 0.52, 0.96),
                warm: rgb(0.95, 0.5, 0.15),
                hover: rgb(0.88, 0.9, 0.94),
            }
        }
    }
}

/// An absolutely placed box inside a card.
fn at(left: f32, top: f32, w: f32, h: f32) -> Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(w))
        .h(px(h))
}

impl MaskDemo {
    /// A card at grid cell `index` with `label` under it.
    fn card(&self, index: usize, label: &'static str, pal: &Palette, content: Div) -> Div {
        let column = index % COLUMNS;
        let row = index / COLUMNS;
        let left = GAP + column as f32 * (CARD_W + GAP);
        let top = GAP + row as f32 * (CARD_H + GAP + 18.0);
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .child(
                content
                    .relative()
                    .w(px(CARD_W))
                    .h(px(CARD_H))
                    .rounded(px(RADIUS))
                    .bg(pal.card)
                    .when(self.clip, Div::clip_rounded),
            )
            .child(div().text_xs().text_color(pal.muted).child(label))
    }

    fn tiles(&self, pal: &Palette) -> Vec<AnyElement> {
        let photo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/screenshots/fixtures/demo-gif-still-photo.jpg");
        let corners = |color: Hsla| {
            div()
                .child(at(-10.0, -10.0, 70.0, 60.0).bg(color))
                .child(at(CARD_W - 60.0, CARD_H - 50.0, 70.0, 60.0).bg(color))
        };
        let tiles: Vec<(&'static str, Div)> =
            vec![
                ("quad", corners(pal.accent)),
                (
                    "quad with border",
                    div().child(
                        at(-6.0, -6.0, 90.0, 80.0)
                            .bg(pal.warm)
                            .border_8()
                            .border_color(pal.accent),
                    ),
                ),
                (
                    "shadow",
                    div().child(at(-30.0, -30.0, 60.0, 60.0).bg(pal.card).shadow(vec![
                        BoxShadow {
                            color: pal.accent,
                            offset: point(px(8.0), px(8.0)),
                            blur_radius: px(24.0),
                            spread_radius: px(12.0),
                            inset: false,
                        },
                    ])),
                ),
                (
                    "text",
                    div().child(
                        at(-8.0, -22.0, 260.0, 90.0)
                            .text_size(px(72.0))
                            .text_color(pal.text)
                            .child("Quill"),
                    ),
                ),
                (
                    "emoji",
                    div().child(
                        at(-18.0, -24.0, 260.0, 90.0)
                            .text_size(px(72.0))
                            .child("\u{1F600}\u{1F308}"),
                    ),
                ),
                (
                    "image",
                    div().child(img(photo).absolute().left_0().top_0().size_full()),
                ),
                (
                    "icon",
                    div().child(
                        at(-24.0, -24.0, 96.0, 96.0).child(
                            Icon::new(IconName::Lock)
                                .size(px(96.0))
                                .text_color(pal.accent),
                        ),
                    ),
                ),
                (
                    "rotated icon",
                    div().child(
                        at(-24.0, -24.0, 96.0, 96.0).child(
                            Icon::new(IconName::Lock)
                                .size(px(96.0))
                                .text_color(pal.warm)
                                .rotate(radians(0.5)),
                        ),
                    ),
                ),
                ("path", div().child(self.path(pal.accent))),
                (
                    "underline",
                    div().child(
                        at(-4.0, -6.0, 260.0, 60.0)
                            .text_size(px(40.0))
                            .text_color(pal.text)
                            .underline()
                            .text_decoration_color(pal.accent)
                            .text_decoration_1()
                            .child("Underline"),
                    ),
                ),
                ("glass", self.glass(pal)),
                ("card in a card", self.nested(pal, 8.0, RADIUS - 8.0)),
                ("crossing card", self.nested(pal, 0.0, 6.0)),
                ("scrolled list", self.list(pal, "scrolled")),
                ("hover the last row", self.list(pal, "hover")),
            ];
        tiles
            .into_iter()
            .enumerate()
            .map(|(index, (label, content))| {
                self.card(index, label, pal, content).into_any_element()
            })
            .collect()
    }

    /// A filled triangle across the card's top-left corner.
    fn path(&self, color: Hsla) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                let o = bounds.origin;
                let mut path = PathBuilder::fill();
                path.move_to(point(o.x - px(10.0), o.y - px(10.0)));
                path.line_to(point(o.x + px(150.0), o.y - px(10.0)));
                path.line_to(point(o.x - px(10.0), o.y + px(120.0)));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .size_full()
    }

    /// Glass over a coloured band, reaching the top-right corner.
    fn glass(&self, pal: &Palette) -> Div {
        let lens = LensStyle {
            unit: 0.0,
            bevel: gpui_kit::LENS_DESIGN_HEIGHT * 0.42,
            band: None,
            shadow: 0.0,
            shadow_softness: 34.0,
            shadow_offset: 22.0,
            strength: 1.0,
            color: pal.card,
            surface: pal.page,
            material: LensMaterial::Clear {
                bend: LensBend::Edge {
                    refract: 44.0,
                    magnify: 0.0,
                    dispersion: 0.12,
                    blur_center: 0.35,
                    blur_edge: 2.2,
                },
                frost: 0.3,
                blur: px(0.0),
                blur_mix: 1.0,
            },
            border: None,
            tune: LensTune::default(),
            light: point(0.0, -1.0),
            light_mix: 0.0,
            droplet: None,
            blob: None,
            detail: LensDetail::Copied,
        };
        div()
            .child(at(0.0, 0.0, CARD_W, 50.0).bg(pal.warm))
            .child(at(0.0, 50.0, CARD_W, 50.0).bg(pal.accent))
            .child(
                at(CARD_W - 110.0, -20.0, 140.0, 90.0)
                    .rounded(px(45.0))
                    .backdrop_glass(Some(lens)),
            )
    }

    /// A card inset by `inset` with radius `radius`, filled, in the card.
    fn nested(&self, pal: &Palette, inset: f32, radius: f32) -> Div {
        div().child(
            at(
                inset,
                inset,
                CARD_W - 2.0 * inset - 30.0,
                CARD_H - 2.0 * inset,
            )
            .rounded(px(radius))
            .bg(pal.hover)
            .when(self.clip, Div::clip_rounded)
            .child(at(-10.0, -10.0, 60.0, 50.0).bg(pal.warm)),
        )
    }

    /// Rows running edge to edge: scrolled by 36 px in a viewport, or
    /// resting on the card's bottom edge with a hover fill.
    fn list(&self, pal: &Palette, id: &'static str) -> Div {
        let rows = (0..6).map(|row| {
            div()
                .id(("row", row as usize))
                .h(px(34.0))
                .w_full()
                .px(px(14.0))
                .flex()
                .items_center()
                .text_sm()
                .text_color(pal.text)
                .bg(if row % 2 == 0 { pal.card } else { pal.page })
                .hover(|style| style.bg(pal.accent))
                .child(format!("Row {}", row + 1))
        });
        let list = div().id(id).absolute().left_0().w_full().children(rows);
        let list = if id == "scrolled" {
            list.top_0()
                .h_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
        } else {
            list.bottom_0()
        };
        div().child(list)
    }
}

impl Render for MaskDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::new(cx.theme().is_dark());
        div()
            .id("rounded-mask-demo")
            .size_full()
            .relative()
            .bg(pal.page)
            .children(self.tiles(&pal))
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "rounded_mask_tests.rs"]
mod tests;

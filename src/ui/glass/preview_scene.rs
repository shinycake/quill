//! What the glass tuning preview's pieces sit over: a loud, detailed scene,
//! so bending, blur, frost and colour split show at a glance. Saturated
//! colour fields, a fine grid, hard-edged stripes, a checkerboard,
//! concentric rings, large type and a photo, each placed by fraction of
//! the preview so the scene fills any width.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::lang::{keys, tr};

/// The scene's colours, loud in both themes; dark keeps them a step deeper.
struct Palette {
    from: Rgba,
    to: Rgba,
    blobs: [Rgba; 3],
    grid: Hsla,
    ink: Rgba,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            from: rgb(0x2d0a6b),
            to: rgb(0xb5179e),
            blobs: [rgb(0xffbe0b), rgb(0x00c49a), rgb(0x3a86ff)],
            grid: hsla(0., 0., 1., 0.22),
            ink: rgb(0xffffff),
        }
    } else {
        Palette {
            from: rgb(0xff5e7e),
            to: rgb(0x5f6bff),
            blobs: [rgb(0xffd60a), rgb(0x06d6a0), rgb(0x118ab2)],
            grid: hsla(0., 0., 1., 0.35),
            ink: rgb(0x14142b),
        }
    }
}

/// Hard-edged stripe colours: a rainbow split by black.
const STRIPES: [u32; 8] = [
    0xff006e, 0x111111, 0xffbe0b, 0x111111, 0x06d6a0, 0x111111, 0x3a86ff, 0x111111,
];
/// Grid spacing, and how many lines cover the widest preview.
const GRID: f32 = 24.;
const GRID_COLUMNS: usize = 48;
const GRID_ROWS: usize = 18;

fn at(left: f32, top: f32) -> Div {
    div().absolute().left(relative(left)).top(relative(top))
}

/// A soft colour field.
fn blob(left: f32, top: f32, diameter: f32, color: Rgba) -> Div {
    at(left, top)
        .size(px(diameter))
        .rounded_full()
        .bg(color)
        .opacity(0.9)
}

/// Thin light lines every `GRID` points over the whole scene.
fn grid(color: Hsla) -> impl Iterator<Item = Div> {
    let columns = (1..GRID_COLUMNS).map(move |column| {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(column as f32 * GRID))
            .w(px(1.))
            .bg(color)
    });
    let rows = (1..GRID_ROWS).map(move |row| {
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(row as f32 * GRID))
            .h(px(1.))
            .bg(color)
    });
    columns.chain(rows)
}

/// A block of vertical stripes, 9 points each.
fn stripes(left: f32) -> Div {
    at(left, 0.).bottom_0().flex().children(
        STRIPES
            .iter()
            .chain(STRIPES.iter())
            .map(|color| div().w(px(9.)).h_full().bg(rgb(*color))),
    )
}

/// A black and white checkerboard of 16-point squares.
fn checkerboard(left: f32, top: f32) -> Div {
    const COLUMNS: usize = 6;
    const ROWS: usize = 5;
    at(left, top)
        .flex()
        .flex_col()
        .children((0..ROWS).map(|row| {
            div().flex().children((0..COLUMNS).map(move |column| {
                let dark = (row + column) % 2 == 0;
                div()
                    .size(px(16.))
                    .bg(if dark { rgb(0x111111) } else { rgb(0xffffff) })
            }))
        }))
}

/// A target of rings, alternating white and a strong colour.
fn rings(left: f32, top: f32) -> Div {
    const RINGS: [(f32, u32); 5] = [
        (140., 0xffffff),
        (112., 0xff006e),
        (84., 0xffffff),
        (56., 0x3a86ff),
        (28., 0xffffff),
    ];
    at(left, top)
        .size(px(140.))
        .children(RINGS.iter().map(|(diameter, color)| {
            let inset = (140. - diameter) / 2.;
            div()
                .absolute()
                .top(px(inset))
                .left(px(inset))
                .size(px(*diameter))
                .rounded_full()
                .border(px(5.))
                .border_color(rgb(*color))
        }))
}

/// A sunset over two hills, as in a chat photo.
fn photo(left: f32, top: f32) -> Div {
    let hill = |top: f32, left: f32, color: u32| {
        div()
            .absolute()
            .top(px(top))
            .left(px(left))
            .size(px(240.))
            .rounded_full()
            .bg(rgb(color))
    };
    at(left, top)
        .w(px(200.))
        .h(px(120.))
        .rounded_xl()
        .overflow_hidden()
        .border_2()
        .border_color(rgb(0xffffff))
        .bg(linear_gradient(
            180.,
            linear_color_stop(rgb(0xf7b267), 0.),
            linear_color_stop(rgb(0xf4845f), 1.),
        ))
        .child(
            div()
                .absolute()
                .top(px(22.))
                .left(px(130.))
                .size(px(34.))
                .rounded_full()
                .bg(rgb(0xfff1c1)),
        )
        .child(hill(66., -110., 0xd9634a))
        .child(hill(84., 50., 0xa8432f))
}

/// Large type, and a paragraph of small type under it.
fn type_block(left: f32, top: f32, ink: Rgba) -> Div {
    at(left, top)
        .flex()
        .flex_col()
        .text_color(ink)
        .child(
            div()
                .text_size(px(54.))
                .line_height(px(58.))
                .font_weight(FontWeight::BLACK)
                .child("Quill"),
        )
        .children(
            [
                keys::Q_PREFS_GLASS_SCENE_LINE_1,
                keys::Q_PREFS_GLASS_SCENE_LINE_2,
                keys::Q_PREFS_GLASS_SCENE_LINE_3,
            ]
            .into_iter()
            .map(|line| {
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr(line))
            }),
        )
}

/// The whole scene, filling its parent.
pub(super) fn backdrop(dark: bool) -> Div {
    let palette = palette(dark);
    let [gold, mint, blue] = palette.blobs;
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .bg(linear_gradient(
            120.,
            linear_color_stop(palette.from, 0.),
            linear_color_stop(palette.to, 1.),
        ))
        .child(blob(-0.04, 0.45, 200., gold))
        .child(blob(0.42, -0.2, 170., mint))
        .child(blob(0.78, 0.5, 220., blue))
        .children(grid(palette.grid))
        .child(stripes(0.3))
        .child(checkerboard(0.8, 0.03))
        .child(rings(0.56, 0.46))
        .child(photo(0.04, 0.12))
        .child(type_block(0.33, 0.13, palette.ink))
}

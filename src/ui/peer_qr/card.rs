//! The white card: the code with the mark in the middle and the name
//! under it (tdesktop `PaintCard`), at any scale so it can shrink above
//! an open panel.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::peer_qr::color::Rgb;
use quill::peer_qr::layout::{
    CardLayout, SHADOW_ALPHA, SHADOW_RADIUS, SHADOW_SHIFT, USERNAME_SKIP, UsernameFit, fit_username,
};
use std::sync::Arc;

/// Line height over font size for the name.
pub(super) const LINE: f64 = 1.2;

/// `text` set bold at `size`, measured with the window's font.
pub(super) fn bold_width(text: &str, size: f64, window: &mut Window) -> f64 {
    let mut font = window.text_style().font();
    font.weight = FontWeight::BOLD;
    let run = TextRun {
        len: text.len(),
        font,
        color: white(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(
        SharedString::from(text.to_string()),
        px(size as f32),
        &[run],
        None,
    );
    f64::from(f32::from(line.width))
}

/// The name's fit on a card of `card` (unscaled).
pub(super) fn fit(label: &str, card: &CardLayout, window: &mut Window) -> Option<UsernameFit> {
    // `fit_username` takes `Fn`; measuring needs the window mutably.
    let window = std::cell::RefCell::new(window);
    let measure = |text: &str, size: f64| bold_width(text, size, &mut window.borrow_mut());
    fit_username(
        label,
        card.width - 2. * USERNAME_SKIP,
        card.band.height,
        &measure,
        &|size| size * LINE,
    )
}

/// GPUI's color for an `Rgb`.
pub(super) fn hsla(color: Rgb) -> Hsla {
    rgb(color.to_hex()).into()
}

/// The card `layout.width × layout.height`, `scale` times its size at
/// rest; the caller places it.
pub(super) fn card(
    layout: &CardLayout,
    scale: f64,
    code: Option<Arc<RenderImage>>,
    ink: Rgb,
    name: Option<&UsernameFit>,
) -> Stateful<Div> {
    let at = |value: f64| px(value as f32);
    let scaled = layout.scaled(scale);
    let shadow = BoxShadow {
        color: black().opacity(SHADOW_ALPHA),
        offset: point(px(0.), at(SHADOW_SHIFT * scale)),
        blur_radius: at(SHADOW_RADIUS * scale),
        spread_radius: px(0.),
        inset: false,
    };
    let mut card = div()
        .id("peer-qr-card")
        .relative()
        .w(at(scaled.width))
        .h(at(scaled.height))
        .rounded(at(scaled.radius))
        .bg(white())
        .shadow(vec![shadow]);
    if let Some(code) = code {
        card = card.child(
            img(ImageSource::Render(code))
                .absolute()
                .left(at(scaled.qr.x))
                .top(at(scaled.qr.y))
                .size(at(scaled.qr.width)),
        );
    }
    if let Some(name) = name {
        let size = name.size * scale;
        card = card.child(
            div()
                .id("peer-qr-name")
                .absolute()
                .left(px(0.))
                .top(at(scaled.band.y))
                .w(at(scaled.width))
                .h(at(scaled.band.height))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .text_size(at(size))
                .line_height(at(size * LINE))
                .font_weight(FontWeight::BOLD)
                .text_color(hsla(ink))
                .whitespace_nowrap()
                .children(name.lines.iter().map(|line| div().child(line.clone()))),
        );
    }
    card
}

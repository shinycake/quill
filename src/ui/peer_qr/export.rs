//! Copy and share: the card on its backdrop and pattern as a PNG
//! (tdesktop `PeerQrImage`), drawn as one SVG and rendered off the UI
//! thread.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::qr_path::{QrGrid, qr_body};
use base64::Engine as _;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_time::now_unix;
use quill::peer_qr::appearance::Appearance;
use quill::peer_qr::appearance::{GRADIENT_STEPS, gradient_stops};
use quill::peer_qr::color::Rgb;
use quill::peer_qr::layout::{
    BOX_WIDTH, CARD_WIDTH, CardLayout, SHADOW_ALPHA, SHADOW_RADIUS, SHADOW_SHIFT, UsernameFit,
    card_layout,
};
use quill::peer_qr::pattern::{Falloff, LAYER_BAND, Metrics, Rect, overlaps, pick_index, tiles};
use quill::peer_qr::state::{QrToast, ShownLink};
use std::fmt::Write as _;
use std::sync::Arc;

/// The saved image is four pixels per point.
const EXPORT_SCALE: f64 = 4.;
/// The backdrop around the card in the saved image.
const MARGIN: f64 = (BOX_WIDTH - CARD_WIDTH) / 2.;
/// The name's fonts, the UI's own first.
const FONT_FALLBACK: &str =
    "'SF Pro Display', 'Segoe UI', Inter, 'Noto Sans', 'DejaVu Sans', sans-serif";

/// Everything the picture is made of, read on the UI thread.
struct Picture {
    link: String,
    ink: Rgb,
    center: Rgb,
    edge: Rgb,
    /// The card as the box shows it: with the name band for a peer.
    card: CardLayout,
    name: Option<UsernameFit>,
    font: String,
    /// Each name line's width in the box, so the saved line matches it
    /// whichever font the renderer finds.
    name_widths: Vec<f64>,
    stills: Vec<Arc<RenderImage>>,
    tiles: Vec<(f64, f64, f64, usize)>,
    /// What the box showed, to check the image against when it is done.
    appearance: Appearance,
}

fn hex(color: Rgb) -> String {
    format!("#{:06X}", color.to_hex())
}

/// A still as a PNG data URI.
fn still_uri(still: &RenderImage) -> Option<String> {
    let size = still.size(0);
    let mut pixels = still.as_bytes(0)?.to_vec();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(
        u32::try_from(size.width.0).ok()?,
        u32::try_from(size.height.0).ok()?,
        pixels,
    )?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    ))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The picture's SVG, `width × height` points.
fn picture_svg(picture: &Picture, card: &CardLayout, width: f64, height: f64) -> Option<String> {
    let grid = QrGrid::encode(&picture.link)?;
    let radius = super::art::backdrop_radius();
    let (cx, cy) = (MARGIN + card.width / 2., MARGIN + card.height / 2.);
    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><defs><radialGradient id="b" gradientUnits="userSpaceOnUse" cx="{cx}" cy="{cy}" r="{radius}">"#
    );
    for (index, stop) in gradient_stops(picture.center, picture.edge)
        .iter()
        .enumerate()
    {
        let offset = index as f64 / GRADIENT_STEPS as f64;
        let _ = write!(
            svg,
            r#"<stop offset="{offset}" stop-color="{}"/>"#,
            hex(*stop)
        );
    }
    let _ = write!(
        svg,
        r#"</radialGradient><filter id="s" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="{}"/></filter></defs><rect width="{width}" height="{height}" fill="url(#b)"/>"#,
        SHADOW_RADIUS / 2.
    );
    let uris: Vec<Option<String>> = picture
        .stills
        .iter()
        .map(|still| still_uri(still))
        .collect();
    for (x, y, size, pick) in &picture.tiles {
        if let Some(Some(uri)) = uris.get(*pick) {
            let half = size / 2.;
            let _ = write!(
                svg,
                r#"<image x="{}" y="{}" width="{size}" height="{size}" xlink:href="{uri}"/>"#,
                x - half,
                y - half
            );
        }
    }
    let _ = write!(
        svg,
        r#"<rect x="{MARGIN}" y="{}" width="{w}" height="{h}" rx="{r}" fill="black" fill-opacity="{SHADOW_ALPHA}" filter="url(#s)"/><rect x="{MARGIN}" y="{MARGIN}" width="{w}" height="{h}" rx="{r}" fill="white"/>"#,
        MARGIN + SHADOW_SHIFT,
        w = card.width,
        h = card.height,
        r = card.radius,
    );
    let _ = write!(
        svg,
        r#"<g transform="translate({} {}) scale({})">{}</g>"#,
        MARGIN + card.qr.x,
        MARGIN + card.qr.y,
        card.qr.width / grid.size as f64,
        qr_body(&grid, picture.ink.to_hex())
    );
    if let Some(name) = &picture.name {
        let line = name.size * super::card::LINE;
        let band_middle = MARGIN + card.band.y + card.band.height / 2.;
        let first = band_middle - line * (name.lines.len() as f64 - 1.) / 2.;
        for (index, text) in name.lines.iter().enumerate() {
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" font-family="{}" font-size="{}" font-weight="bold" fill="{}" text-anchor="middle" dominant-baseline="central"{}>{}</text>"#,
                BOX_WIDTH / 2.,
                first + line * index as f64,
                escape(&picture.font),
                name.size,
                hex(picture.ink),
                picture
                    .name_widths
                    .get(index)
                    .filter(|width| **width > 0.)
                    .map(|width| format!(
                        r#" textLength="{width}" lengthAdjust="spacingAndGlyphs""#
                    ))
                    .unwrap_or_default(),
                escape(text)
            );
        }
    }
    svg.push_str("</svg>");
    Some(svg)
}

/// The picture as PNG bytes.
fn render_png(renderer: &SvgRenderer, picture: &Picture) -> Option<Vec<u8>> {
    let card = &picture.card;
    let (width, height) = (BOX_WIDTH, card.height + 2. * MARGIN);
    let svg = picture_svg(picture, card, width, height)?;
    let parsed = renderer.parse_svg(svg.as_bytes()).ok()?;
    let pixels = size(
        DevicePixels((width * EXPORT_SCALE).round() as i32),
        DevicePixels((height * EXPORT_SCALE).round() as i32),
    );
    let image = renderer
        .render_parsed(&parsed, SvgSize::ExactSize(pixels))
        .ok()?;
    let size = image.size(0);
    let mut bytes = image.as_bytes(0)?.to_vec();
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let rgba = image::RgbaImage::from_raw(
        u32::try_from(size.width.0).ok()?,
        u32::try_from(size.height.0).ok()?,
        bytes,
    )?;
    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

/// Where the pattern's emoji go in the saved image: its first frame, none
/// touching the card.
fn picture_tiles(count: usize, card: &CardLayout) -> Vec<(f64, f64, f64, usize)> {
    if count == 0 {
        return Vec::new();
    }
    let (width, height) = (BOX_WIDTH, card.height + 2. * MARGIN);
    let card_rect = Rect::new(MARGIN, MARGIN, card.width, card.height);
    let falloff = Falloff {
        area: Rect::new(0., 0., width, height),
        band: LAYER_BAND,
    };
    tiles(&falloff, card_rect.center(), &Metrics::LIVE, count, 0.)
        .into_iter()
        .filter(|tile| !overlaps(&card_rect, tile.x, tile.y, tile.size))
        .map(|tile| {
            (
                tile.x,
                tile.y,
                tile.size,
                pick_index(tile.row, tile.column, count),
            )
        })
        .collect()
}

/// What Copy puts on the clipboard on this platform. macOS writes only
/// the strings of an item with more than one entry, so it gets the image
/// alone, as tdesktop copies it; the X11 and Wayland clipboards take text
/// only, so they get the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyKind {
    Image,
    ImageAndLink,
    Link,
}

const COPY_KIND: CopyKind = if cfg!(target_os = "macos") {
    CopyKind::Image
} else if cfg!(target_os = "windows") {
    CopyKind::ImageAndLink
} else {
    CopyKind::Link
};

/// The clipboard item for `kind` and the toast saying what it holds.
fn copy_item(kind: CopyKind, png: Vec<u8>, link: String) -> (ClipboardItem, QrToast) {
    let image = ClipboardEntry::Image(gpui_kit::Image::from_bytes(ImageFormat::Png, png));
    let entries = match kind {
        CopyKind::Image => vec![image],
        CopyKind::ImageAndLink => vec![image, ClipboardEntry::String(ClipboardString::new(link))],
        CopyKind::Link => return (ClipboardItem::new_string(link), QrToast::LinkCopied),
    };
    (ClipboardItem { entries }, QrToast::Copied)
}

impl QuillApp {
    /// The link copy and share may use, or the toast saying why not.
    fn qr_shareable(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(ShownLink, Picture)> {
        let view = self.qr_view()?;
        let open = self.peer_qr.open.as_ref()?;
        if open.exporting {
            return None;
        }
        let shown = match open.state.check_share(view.shown.clone(), now_unix()) {
            Ok(shown) => shown,
            Err(toast) => {
                if let Some(toast) = toast {
                    self.show_qr_toast(toast, cx);
                }
                return None;
            }
        };
        let picks = open.state.emoji.clone();
        let stills = picks
            .iter()
            .filter_map(|emoji| self.qr_emoji_still(emoji))
            .collect::<Vec<_>>();
        let card = card_layout(CARD_WIDTH, view.peer_box());
        let name = view
            .label
            .as_ref()
            .filter(|_| view.peer_box())
            .and_then(|label| super::card::fit(label, &card, window));
        let picture = Picture {
            link: shown.link.clone(),
            ink: view.colors.ink,
            center: view.appearance.center,
            edge: view.appearance.edge,
            card,
            name,
            font: String::new(),
            name_widths: Vec::new(),
            tiles: picture_tiles(stills.len(), &card),
            stills,
            appearance: view.appearance,
        };
        Some((shown, picture))
    }

    /// The box still shows what was drawn: the same link in the same
    /// colors, not about to run out (tdesktop `ImageIsCurrent`,
    /// `RefuseImage`).
    fn qr_image_current(
        &mut self,
        link: &str,
        appearance: &Appearance,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.qr_view() else {
            return false;
        };
        let Some(open) = self.peer_qr.open.as_ref() else {
            return false;
        };
        match open.state.check_share(view.shown, now_unix()) {
            Ok(shown) => shown.link == link && view.appearance == *appearance,
            Err(toast) => {
                if let Some(toast) = toast {
                    self.show_qr_toast(toast, cx);
                }
                false
            }
        }
    }

    /// Render `picture` and pass its PNG through `finish` off the UI
    /// thread, then hand the result to `done`, with whether the box still
    /// shows what was drawn.
    fn qr_export<T: Send + 'static>(
        &mut self,
        mut picture: Picture,
        window: &mut Window,
        finish: impl FnOnce(Vec<u8>) -> Option<T> + Send + 'static,
        done: impl FnOnce(&mut Self, Option<T>, bool, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if let Some(name) = &picture.name {
            picture.name_widths = name
                .lines
                .iter()
                .map(|line| super::card::bold_width(line, name.size, window))
                .collect();
        }
        let family = window.text_style().font().family;
        picture.font = if family.starts_with('.') {
            FONT_FALLBACK.to_string()
        } else {
            format!("'{family}', {FONT_FALLBACK}")
        };
        let Some(open) = self.peer_qr.open.as_mut() else {
            return;
        };
        open.exporting = true;
        let box_id = open.state.id;
        let renderer = cx.svg_renderer();
        let link = picture.link.clone();
        let appearance = picture.appearance.clone();
        self.run_off_ui_then(
            move || render_png(&renderer, &picture).and_then(finish),
            move |this, result, cx| {
                // A box opened since then, even for the same link, is not
                // the one that asked.
                let open = this
                    .peer_qr
                    .open
                    .as_mut()
                    .filter(|open| open.state.id == box_id);
                let same_box = open.is_some();
                if let Some(open) = open {
                    open.exporting = false;
                }
                let current = same_box && this.qr_image_current(&link, &appearance, cx);
                done(this, result, current, cx);
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    /// Copy: the card image, and the link where the clipboard takes both;
    /// on X11 and Wayland, the link.
    pub(super) fn qr_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((shown, picture)) = self.qr_shareable(window, cx) else {
            return;
        };
        if COPY_KIND == CopyKind::Link {
            cx.write_to_clipboard(ClipboardItem::new_string(shown.link));
            self.show_qr_toast(QrToast::LinkCopied, cx);
            return;
        }
        self.qr_export(
            picture,
            window,
            Some,
            move |this, png, current, cx| match png {
                Some(png) if current => {
                    let (item, toast) = copy_item(COPY_KIND, png, shown.link);
                    cx.write_to_clipboard(item);
                    this.show_qr_toast(toast, cx);
                }
                None if current => this.show_qr_toast(QrToast::Unavailable, cx),
                _ => {}
            },
            cx,
        );
    }

    /// Share: pick a chat; the link goes into its composer with the card
    /// image attached, unsent.
    pub(super) fn qr_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((shown, picture)) = self.qr_shareable(window, cx) else {
            return;
        };
        self.qr_export(
            picture,
            window,
            |png| quill::composer::clipboard_image_attachment(&png, "png").map(|file| file.path),
            move |this, image, current, cx| {
                let open = this.peer_qr.open.as_mut().filter(|_| current);
                let Some(open) = open else {
                    if let Some(path) = image {
                        super::remove_card_file(path, cx);
                    }
                    return;
                };
                open.share_image = image;
                this.share.link_text = Some(shown.link);
                this.connection
                    .set_note(crate::ui::status_note::StatusNote::silent(tr(
                        keys::Q_MISC_CHOOSE_A_CHAT_TO_SHARE_2,
                    )));
            },
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOX_WIDTH, CARD_WIDTH, CopyKind, FONT_FALLBACK, MARGIN, Picture, QrToast, Rgb, UsernameFit,
        card_layout, copy_item, picture_svg, picture_tiles,
    };
    use gpui_kit::ClipboardEntry;
    use quill::peer_qr::appearance::Appearance;

    #[test]
    fn the_picture_holds_the_code_the_name_and_the_pattern() {
        let card = card_layout(CARD_WIDTH, true);
        let picture = Picture {
            link: "https://t.me/quill".into(),
            ink: Rgb::hex(0x22_3344),
            center: Rgb::hex(0x44_5566),
            edge: Rgb::hex(0x11_2233),
            card,
            name: Some(UsernameFit {
                size: 30.,
                lines: vec!["@QUILL".into()],
            }),
            font: FONT_FALLBACK.into(),
            name_widths: vec![120.],
            stills: Vec::new(),
            tiles: picture_tiles(1, &card),
            appearance: Appearance {
                center: Rgb::hex(0x44_5566),
                edge: Rgb::hex(0x11_2233),
                emoji: Vec::new(),
            },
        };
        let svg = picture_svg(&picture, &card, BOX_WIDTH, card.height + 2. * MARGIN).expect("svg");
        assert!(svg.contains("@QUILL"));
        assert!(svg.contains(r#"textLength="120""#));
        assert!(svg.contains("fill=\"#223344\""));
        assert!(svg.contains("radialGradient"));
        assert!(!picture.tiles.is_empty());
    }

    #[test]
    fn copy_holds_what_the_platform_clipboard_can_take() {
        let png = b"\x89PNG".to_vec();
        let link = "https://t.me/quill".to_string();
        let (item, toast) = copy_item(CopyKind::Image, png.clone(), link.clone());
        assert_eq!(toast, QrToast::Copied);
        assert!(matches!(
            item.entries.as_slice(),
            [ClipboardEntry::Image(_)]
        ));
        let (item, toast) = copy_item(CopyKind::ImageAndLink, png.clone(), link.clone());
        assert_eq!(toast, QrToast::Copied);
        assert!(matches!(
            item.entries.as_slice(),
            [ClipboardEntry::Image(_), ClipboardEntry::String(text)] if text.text == link
        ));
        let (item, toast) = copy_item(CopyKind::Link, png, link.clone());
        assert_eq!(toast, QrToast::LinkCopied);
        assert_eq!(item.text(), Some(link));
    }
}

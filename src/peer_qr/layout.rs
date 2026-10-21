//! The box's geometry in logical pixels: the card (tdesktop
//! `ComputeCardLayout`), the controls around it (`relayout`), where the
//! card goes while a panel is open (`ComputeOpenLayout`) and the panel's
//! motion (`PeerQrPanel`).

use super::pattern::Rect;

/// `peerQrBoxWidth`.
pub const BOX_WIDTH: f64 = 390.;
/// `peerQrCardWidth`.
pub const CARD_WIDTH: f64 = 288.;
/// `peerQrCardSkip`.
pub const CARD_SKIP: f64 = 20.;
/// `peerQrButtonSkip`: the round buttons' distance from the box edges.
pub const BUTTON_SKIP: f64 = 16.;
/// `peerQrButton`: the close, copy and share buttons.
pub const BUTTON: f64 = 40.;
/// `peerQrColors`: one button of the colors and emoji island.
pub const ISLAND_BUTTON: f64 = 48.;
/// `peerQrPillSkip`.
pub const PILL_SKIP: f64 = 13.;
/// `peerQrPillHeight`.
pub const PILL_HEIGHT: f64 = 26.;
/// `peerQrUsernameSkip`: the name's side margin on the card.
pub const USERNAME_SKIP: f64 = 18.;
/// `peerQrPanelMargin`.
pub const PANEL_MARGIN: f64 = 10.;
/// `peerQrPanelPadding`.
pub const PANEL_PADDING: f64 = 11.;
/// `peerQrPanelRadius`.
pub const PANEL_RADIUS: f64 = 24.;
/// `peerQrPanelSkip`.
pub const PANEL_SKIP: f64 = 12.;
/// `peerQrSwatchSkip`.
pub const SWATCH_SKIP: f64 = 12.;
/// Swatches per row.
pub const SWATCH_COLUMNS: usize = 6;
/// `peerQrCardShadowRadius`.
pub const SHADOW_RADIUS: f64 = 21.;
/// `peerQrCardShadowShift`.
pub const SHADOW_SHIFT: f64 = 7.;
/// `kShadowAlpha`.
pub const SHADOW_ALPHA: f32 = 0.13;
/// The username fonts, largest first, and the smallest a two-line name
/// may shrink to (`peerQrUsername*`).
pub const USERNAME_SIZES: [f64; 3] = [36., 30., 23.];
pub const USERNAME_MIN: f64 = 14.;
/// `kOpenDuration` / `kCloseDuration` of a panel.
pub const PANEL_OPEN_MS: f64 = 420.;
pub const PANEL_CLOSE_MS: f64 = 360.;
/// The panel's content fades in after the glass starts to grow.
pub const CONTENT_IN_DELAY_MS: f64 = 60.;
pub const CONTENT_IN_MS: f64 = 220.;

const CARD_HEIGHT_RATIO: f64 = 1.07;
const CARD_RADIUS_RATIO: f64 = 0.125;
const QR_SIDE_RATIO: f64 = 0.70;
const QR_TOP_RATIO: f64 = 0.12;
const LOGO_RATIO: f64 = 0.175;
const PANEL_ROWS: f64 = 3.;
const PANEL_PEEK: f64 = 0.4;

/// The card and what sits on it, from the card's top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardLayout {
    pub width: f64,
    pub height: f64,
    pub qr: Rect,
    pub logo: Rect,
    /// The strip under the code where the name goes.
    pub band: Rect,
    pub radius: f64,
}

impl CardLayout {
    /// The same card drawn `by` times smaller or larger.
    pub fn scaled(&self, by: f64) -> CardLayout {
        let rect = |r: Rect| Rect::new(r.x * by, r.y * by, r.width * by, r.height * by);
        CardLayout {
            width: self.width * by,
            height: self.height * by,
            qr: rect(self.qr),
            logo: rect(self.logo),
            band: rect(self.band),
            radius: self.radius * by,
        }
    }
}

/// tdesktop `ComputeCardLayout`: a card with a name is a little taller
/// than wide; one without is square with the code in the middle.
pub fn card_layout(width: f64, with_username: bool) -> CardLayout {
    let part = |ratio: f64| (width * ratio).round();
    let height = if with_username {
        part(CARD_HEIGHT_RATIO)
    } else {
        width
    };
    let side = part(QR_SIDE_RATIO);
    let left = ((width - side) / 2.).floor();
    let top = if with_username {
        part(QR_TOP_RATIO)
    } else {
        left
    };
    let logo = (side * LOGO_RATIO).round();
    let inset = ((side - logo) / 2.).floor();
    let bottom = top + side;
    CardLayout {
        width,
        height,
        qr: Rect::new(left, top, side, side),
        logo: Rect::new(left + inset, top + inset, logo, logo),
        band: Rect::new(0., bottom, width, height - bottom),
        radius: part(CARD_RADIUS_RATIO),
    }
}

/// The swatch circle's diameter in a panel `width` wide.
pub fn swatch_size(width: f64) -> f64 {
    let columns = SWATCH_COLUMNS as f64;
    ((width - 2. * PANEL_PADDING - (columns - 1.) * SWATCH_SKIP) / columns).max(0.)
}

/// tdesktop `PeerQrColorsPanel::HeightForWidth`: three rows and a peek
/// of the fourth. The emoji panel is as tall.
pub fn panel_height(width: f64) -> f64 {
    let circle = swatch_size(width);
    PANEL_PADDING + (PANEL_ROWS * (circle + SWATCH_SKIP) + PANEL_PEEK * circle).round()
}

/// A peer's box with every panel closed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxLayout {
    pub width: f64,
    pub height: f64,
    pub card: Rect,
    pub close: Rect,
    pub copy: Rect,
    pub share: Rect,
    /// The colors and emoji buttons between copy and share.
    pub island: Rect,
    /// The free-code pill's top.
    pub pill_top: f64,
}

/// tdesktop `relayout` for a peer's own code: close at the top right, the
/// card, room for the pill, then copy, the island and share.
pub fn box_layout(with_username: bool, island_buttons: usize) -> BoxLayout {
    let card = card_layout(CARD_WIDTH, with_username);
    let card_top = BUTTON_SKIP + BUTTON + CARD_SKIP;
    let card_rect = Rect::new(
        ((BOX_WIDTH - CARD_WIDTH) / 2.).floor(),
        card_top,
        card.width,
        card.height,
    );
    let card_bottom = card_top + card.height;
    let pill_top = card_bottom + PILL_SKIP;
    let row_top = card_bottom + PILL_SKIP + PILL_HEIGHT + CARD_SKIP;
    let island_width = ISLAND_BUTTON * island_buttons as f64;
    BoxLayout {
        width: BOX_WIDTH,
        height: row_top + BUTTON + BUTTON_SKIP,
        card: card_rect,
        close: Rect::new(
            BOX_WIDTH - BUTTON_SKIP - BUTTON,
            BUTTON_SKIP,
            BUTTON,
            BUTTON,
        ),
        copy: Rect::new(BUTTON_SKIP, row_top, BUTTON, BUTTON),
        share: Rect::new(BOX_WIDTH - BUTTON_SKIP - BUTTON, row_top, BUTTON, BUTTON),
        island: Rect::new(
            ((BOX_WIDTH - island_width) / 2.).floor(),
            row_top,
            island_width,
            BUTTON,
        ),
        pill_top,
    }
}

/// Where things are for a panel `progress` of the way open (0 closed,
/// 1 open).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub card: Rect,
    pub scale: f64,
    pub pill_top: f64,
    /// The glass: the island growing into the panel.
    pub panel: Rect,
}

/// tdesktop `ComputeOpenLayout` blended with the closed layout.
pub fn placement(layout: &BoxLayout, progress: f64) -> Placement {
    let progress = progress.clamp(0., 1.);
    let width = layout.width - 2. * PANEL_MARGIN;
    let height = panel_height(width);
    let panel = Rect::new(
        PANEL_MARGIN,
        layout.height - PANEL_MARGIN - height,
        width,
        height,
    );
    let pill_top = panel.y - PANEL_SKIP - PILL_HEIGHT;
    let bottom = pill_top - PILL_SKIP;
    let card = layout.card;
    let scale_for = |top: f64| {
        if card.height <= 0. {
            1.
        } else {
            ((bottom - top) / card.height).clamp(0., 1.)
        }
    };
    let mut scale = scale_for(BUTTON_SKIP);
    if (layout.width + card.width * scale) / 2. + PANEL_SKIP > layout.width - BUTTON_SKIP - BUTTON {
        scale = scale_for(BUTTON_SKIP + BUTTON + PANEL_SKIP);
    }
    let open_card = Rect::new(
        (layout.width - card.width * scale) / 2.,
        bottom - card.height * scale,
        card.width * scale,
        card.height * scale,
    );
    let lerp = |from: f64, till: f64| from + (till - from) * progress;
    let island = layout.island;
    Placement {
        card: Rect::new(
            lerp(card.x, open_card.x),
            lerp(card.y, open_card.y),
            lerp(card.width, open_card.width),
            lerp(card.height, open_card.height),
        ),
        scale: lerp(1., scale),
        pill_top: lerp(layout.pill_top, pill_top),
        panel: Rect::new(
            lerp(island.x, panel.x),
            lerp(island.y, panel.y),
            lerp(island.width, panel.width),
            lerp(island.height, panel.height),
        ),
    }
}

/// `cubic-bezier(0.16, 1, 0.3, 1)`, the panel's easing (tdesktop
/// `kCurve*`).
pub fn panel_ease(t: f64) -> f64 {
    const X1: f64 = 0.16;
    const X2: f64 = 0.3;
    let t = t.clamp(0., 1.);
    let curve = |u: f64, a: f64, b: f64| {
        let v = 1. - u;
        3. * v * v * u * a + 3. * v * u * u * b + u * u * u
    };
    let mut low = 0.;
    let mut high = 1.;
    for _ in 0..32 {
        let middle = (low + high) / 2.;
        if curve(middle, X1, X2) < t {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve((low + high) / 2., 1., 1.)
}

/// How far the panel's content has faded in, `elapsed_ms` after it
/// started to open (tdesktop `ContentIn`).
pub fn content_in(elapsed_ms: f64) -> f64 {
    let t = ((elapsed_ms - CONTENT_IN_DELAY_MS) / CONTENT_IN_MS).clamp(0., 1.);
    1. - (1. - t).powi(3)
}

/// The name under the code: its font size and one or two lines.
#[derive(Debug, Clone, PartialEq)]
pub struct UsernameFit {
    pub size: f64,
    pub lines: Vec<String>,
}

/// The split of `text` into two lines whose wider line is narrowest
/// (tdesktop `SplitInTwoLines`).
fn split_in_two(text: &str, measure: &dyn Fn(&str) -> f64) -> Vec<String> {
    let mut best: Option<(f64, usize)> = None;
    for (at, _) in text.char_indices().skip(1) {
        let (left, right) = text.split_at(at);
        let wider = measure(left).max(measure(right));
        if best.is_none_or(|(width, _)| wider < width) {
            best = Some((wider, at));
        }
    }
    match best {
        Some((_, at)) => {
            let (left, right) = text.split_at(at);
            vec![left.to_string(), right.to_string()]
        }
        None => vec![text.to_string()],
    }
}

/// tdesktop `FitUsername`: the largest font the name fits in on one
/// line, else two lines in a font shrunk until they fit (or reach
/// [`USERNAME_MIN`]). `measure(text, size)` is the text's width;
/// `line_height(size)` its line height.
pub fn fit_username(
    text: &str,
    available: f64,
    band_height: f64,
    measure: &dyn Fn(&str, f64) -> f64,
    line_height: &dyn Fn(f64) -> f64,
) -> Option<UsernameFit> {
    if text.is_empty() {
        return None;
    }
    for size in USERNAME_SIZES {
        if measure(text, size) <= available && line_height(size) <= band_height {
            return Some(UsernameFit {
                size,
                lines: vec![text.to_string()],
            });
        }
    }
    let mut size = USERNAME_SIZES.last().copied().unwrap_or(USERNAME_MIN);
    loop {
        let lines = split_in_two(text, &|part| measure(part, size));
        let fits = 2. * line_height(size) <= band_height
            && lines.iter().all(|line| measure(line, size) <= available);
        if fits || size <= USERNAME_MIN {
            return Some(UsernameFit { size, lines });
        }
        size -= 1.;
    }
}

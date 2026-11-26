//! The window layout (Settings › Appearance › Layout): Classic, today's
//! panels divided by lines, or Floating, rounded cards and pills over the
//! chat's wallpaper (docs/decisions/floating-layout.md).
//!
//! One stored choice, [`LayoutStyle`], and a pure rule, [`resolve`], that
//! turns it and what the window reads this frame into the [`Chrome`] every
//! render site draws with. Render sites read numbers from
//! [`ChromeMetrics`] or pass their element through the frame helpers in
//! `ui/chrome`; they never ask which layout is on
//! (`scripts/check-layout-style.py`).
#![deny(clippy::indexing_slicing)]

use serde::{Deserialize, Serialize};

/// What the person picked. Classic is the default, so a settings file
/// written before the choice existed loads as Classic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutStyle {
    #[default]
    Classic,
    Floating,
}

impl LayoutStyle {
    /// `classic` or `floating`, in any case, with spaces around.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "classic" => Some(Self::Classic),
            "floating" => Some(Self::Floating),
            _ => None,
        }
    }

    /// `QUILL_DEMO_LAYOUT`, for demos and captures: it wins over the
    /// stored choice. Read once: it never changes after launch.
    pub fn demo_override() -> Option<Self> {
        static VALUE: std::sync::OnceLock<Option<LayoutStyle>> = std::sync::OnceLock::new();
        *VALUE.get_or_init(|| {
            std::env::var("QUILL_DEMO_LAYOUT")
                .ok()
                .and_then(|value| Self::parse(&value))
        })
    }

    /// A stored value that is not a layout loads as Classic, without
    /// failing the rest of the file it sits in.
    pub fn deserialize_or_classic<'de, D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(value
            .as_str()
            .and_then(Self::parse)
            .unwrap_or(Self::Classic))
    }
}

/// The choice in force: the override when one is set, else the stored one.
pub fn choice(stored: LayoutStyle, overridden: Option<LayoutStyle>) -> LayoutStyle {
    overridden.unwrap_or(stored)
}

/// Web K's `$small-screen`: a window this wide or narrower is one column
/// and draws Classic's chrome, interface pixels.
pub const ONE_COLUMN_TO: f32 = 600.;
/// Web K's `$floating-left-sidebar`: from just above [`ONE_COLUMN_TO`] up
/// to this width the chat list is a drawer over the chat.
pub const DRAWER_TO: f32 = 925.;
/// The narrowest window that shows the Floating layout.
pub const FLOATING_FROM: f32 = ONE_COLUMN_TO + 1.;
/// Under this contrast ratio between a card's fill and the plain window
/// background, a card gets a hairline edge: its shadow alone does not set
/// it apart (dark themes hide shadows).
pub const CARD_CONTRAST_FLOOR: f32 = 1.15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contrast {
    Normal,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Normal,
    /// Every power-saving flag is forced (`battery_saving::forcing`).
    Saver,
}

/// What lies behind the cards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackdropKind {
    /// A wallpaper: the cards stand out against it.
    Picture,
    /// The theme's plain background; `card_contrast` is the contrast ratio
    /// between the card fill and that background.
    Plain { card_contrast: f32 },
}

/// What the window can tell the rules this frame, beyond its width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readings {
    pub contrast: Contrast,
    pub power: Power,
    pub backdrop: BackdropKind,
}

/// A card's outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    None,
    Hairline,
}

/// Whether cards and pills cast their shadow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elevation {
    Flat,
    Shadowed,
}

/// What a window draws its regions as. Classic's lines: panels divided
/// by hairlines, the bars stacked over and under the history, which
/// paints the wallpaper. Floating's cards: rounded cards and pills over
/// the window's wallpaper, the bars over the history, which scrolls
/// under them; menus pop and wear Web K's shape; pages cross.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Lines,
    Cards,
}

/// How the open chat's row in the list is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedRow {
    Tint,
    Solid,
}

impl Look {
    /// Lines tint the open chat's row; cards fill it with the accent.
    pub fn selected_row(self) -> SelectedRow {
        match self {
            Look::Lines => SelectedRow::Tint,
            Look::Cards => SelectedRow::Solid,
        }
    }
}

/// Row sizes in Standard density.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowScale {
    /// Quill's own (64 rows, 46 avatars).
    Quill,
    /// Web K's (72 rows, 54 avatars).
    WebK,
}

/// The easing of slides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    /// `ease_out_cubic`, Quill's slides today.
    OutCubic,
    /// Web K's standard curve, `cubic-bezier(.4, 0, .2, 1)`.
    Standard,
}

impl Curve {
    /// The eased progress at `t` (0 to 1, clamped).
    pub fn ease(self, t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        match self {
            Curve::OutCubic => 1. - (1. - t).powi(3),
            Curve::Standard => cubic_bezier(0.4, 0.0, 0.2, 1.0, t),
        }
    }
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `x`: the curve's `y` where its `x`
/// is `x`, found by bisection on the curve parameter (x is monotonic for
/// control points inside 0 to 1).
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let at = |a: f32, b: f32, s: f32| {
        let r = 1. - s;
        3. * r * r * s * a + 3. * r * s * s * b + s * s * s
    };
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..32 {
        let mid = (lo + hi) / 2.;
        if at(x1, x2, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at(y1, y2, (lo + hi) / 2.)
}

/// The layout's look and the numbers that vary with it. Data, like
/// `density::DensityMetrics`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromeMetrics {
    pub look: Look,
    /// Window edge to card, card to conversation column.
    pub gutter: f32,
    /// Between stacked floating bars.
    pub bar_gap: f32,
    /// Card and pill radius, in multiples of the theme radius.
    pub radius: f32,
    /// Widest conversation column.
    pub chat_column: Option<f32>,
    /// The list is a drawer at this width and under.
    pub drawer_to: Option<f32>,
    /// Row sizes in Standard density.
    pub rows: RowScale,
}

/// Today's layout, value for value.
pub const CLASSIC: ChromeMetrics = ChromeMetrics {
    look: Look::Lines,
    gutter: 0.,
    bar_gap: 0.,
    radius: 0.,
    chat_column: None,
    drawer_to: None,
    rows: RowScale::Quill,
};

/// Web K's layout in Quill's units.
pub const FLOATING: ChromeMetrics = ChromeMetrics {
    look: Look::Cards,
    gutter: 16.,
    bar_gap: 8.,
    radius: 4.,
    chat_column: Some(696.),
    drawer_to: Some(DRAWER_TO),
    rows: RowScale::WebK,
};

/// What is drawn: a pure function of the choice and the readings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chrome {
    pub metrics: &'static ChromeMetrics,
    pub edge: Edge,
    pub elevation: Elevation,
}

impl Default for Chrome {
    fn default() -> Self {
        Self::CLASSIC
    }
}

impl Chrome {
    pub const CLASSIC: Chrome = Chrome {
        metrics: &CLASSIC,
        edge: Edge::None,
        elevation: Elevation::Flat,
    };
}

/// The look a window this wide draws: Floating's cards only from
/// [`FLOATING_FROM`] up.
pub fn look_for(stored: LayoutStyle, overridden: Option<LayoutStyle>, width: f32) -> Look {
    match choice(stored, overridden) {
        LayoutStyle::Floating if width >= FLOATING_FROM => Look::Cards,
        LayoutStyle::Classic | LayoutStyle::Floating => Look::Lines,
    }
}

/// The metrics of a look.
pub fn metrics_of(look: Look) -> &'static ChromeMetrics {
    match look {
        Look::Lines => &CLASSIC,
        Look::Cards => &FLOATING,
    }
}

/// The chrome for `look` in a window that reads `readings` (only cards
/// read them). The first matching row wins
/// (docs/decisions/floating-layout.md).
pub fn resolve(look: Look, readings: impl FnOnce() -> Readings) -> Chrome {
    if look == Look::Lines {
        return Chrome::CLASSIC;
    }
    let readings = readings();
    let plain_low = matches!(
        readings.backdrop,
        BackdropKind::Plain { card_contrast } if card_contrast < CARD_CONTRAST_FLOOR
    );
    let plain = matches!(readings.backdrop, BackdropKind::Plain { .. });
    let (edge, elevation) = match (readings.contrast, readings.power) {
        (Contrast::High, _) => (Edge::Hairline, Elevation::Flat),
        (Contrast::Normal, Power::Saver) => (
            if plain { Edge::Hairline } else { Edge::None },
            Elevation::Flat,
        ),
        (Contrast::Normal, Power::Normal) => (
            if plain_low {
                Edge::Hairline
            } else {
                Edge::None
            },
            Elevation::Shadowed,
        ),
    };
    Chrome {
        metrics: &FLOATING,
        edge,
        elevation,
    }
}

/// The WCAG contrast ratio of two relative luminances (each 0 to 1).
pub fn contrast_ratio(a: f32, b: f32) -> f32 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
#[path = "layout_style_tests.rs"]
mod tests;

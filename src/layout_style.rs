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

/// `QUILL_DEMO_LAYOUT`, for demos and captures: `classic` or `floating`
/// wins over the stored choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutOverride {
    Classic,
    Floating,
}

impl LayoutOverride {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "classic" => Some(Self::Classic),
            "floating" => Some(Self::Floating),
            _ => None,
        }
    }

    /// The override in the environment, read once: it never changes after
    /// launch.
    pub fn from_env() -> Option<Self> {
        static VALUE: std::sync::OnceLock<Option<LayoutOverride>> = std::sync::OnceLock::new();
        *VALUE.get_or_init(|| {
            std::env::var("QUILL_DEMO_LAYOUT")
                .ok()
                .and_then(|value| Self::parse(&value))
        })
    }

    pub fn style(self) -> LayoutStyle {
        match self {
            Self::Classic => LayoutStyle::Classic,
            Self::Floating => LayoutStyle::Floating,
        }
    }
}

/// The choice in force: the override when one is set, else the stored one.
pub fn choice(stored: LayoutStyle, overridden: Option<LayoutOverride>) -> LayoutStyle {
    overridden.map_or(stored, LayoutOverride::style)
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

/// What the window can tell the rules this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readings {
    /// The window's content width, interface pixels.
    pub width: f32,
    /// The narrowest width at which a window shows the Floating layout
    /// ([`FLOATING_FROM`]).
    pub floating_from: f32,
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

/// Where the conversation's header and composer sit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Stacked above and below the history, divided by lines.
    Stacked,
    /// Pills over the history, which scrolls under them.
    Overlaid,
}

/// Who paints the chat wallpaper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    /// The history, inside its own pane.
    History,
    /// The window, behind every card.
    Window,
}

/// How the open chat's row in the list is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedRow {
    Tint,
    Solid,
}

/// The folder tabs' indicator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderTabs {
    Underline,
    Pill,
}

/// The search field's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchShape {
    Field,
    Pill,
}

/// How side panels sit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelDock {
    /// A column divided by a line.
    Column,
    /// A card, docked or over the chat.
    Card,
}

/// The title strip's lower edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleStrip {
    Lined,
    Bare,
}

/// Row sizes in Standard density.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowScale {
    /// Quill's own (64 rows, 46 avatars).
    Quill,
    /// Web K's (72 rows, 54 avatars).
    WebK,
}

/// Whether cards and pills clip their content to their corners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    Square,
    Rounded,
}

/// The easing of slides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    /// `ease_out_cubic`, Quill's slides today.
    OutCubic,
    /// Web K's standard curve, `cubic-bezier(.4, 0, .2, 1)`.
    Standard,
}

/// How a bar above the history appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarReveal {
    /// Opens by height under a rectangular clip.
    Slide,
    /// Rises into place and fades in (a pill cannot be clipped open).
    Rise,
}

/// How a menu enters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEnter {
    /// The kit's fade and slide.
    FadeSlide,
    /// Scale from .8 and fade, from its anchor's corner.
    Pop,
}

/// How one page gives way to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSwitch {
    Cut,
    Cross,
}

/// What a sliding card is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideDraw {
    Live,
    Snapshot,
}

/// Timings and shapes of the layout's motion (docs/decisions/floating-layout.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Motion {
    pub curve: Curve,
    pub panel_in_ms: u16,
    pub panel_out_ms: u16,
    pub sidebar_ms: u16,
    pub bar_reveal: BarReveal,
    pub menu: MenuEnter,
    pub chat_switch: PageSwitch,
    pub folder_page: PageSwitch,
    /// The composer's height glide; 0 follows the measured height at once.
    pub composer_ms: u16,
    pub slides: SlideDraw,
}

/// The layout's numbers and shapes. Data, like `density::DensityMetrics`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromeMetrics {
    /// Window edge to card, card to conversation column.
    pub gutter: f32,
    /// Between stacked floating bars.
    pub bar_gap: f32,
    /// Card and pill radius, in multiples of the theme radius.
    pub radius: f32,
    /// Divider between regions.
    pub divider: f32,
    /// Widest conversation column.
    pub chat_column: Option<f32>,
    /// Shortest bar pill; 0 is the bar's natural height.
    pub bar_height: f32,
    pub placement: Placement,
    pub backdrop: Backdrop,
    pub selected_row: SelectedRow,
    pub folder_tabs: FolderTabs,
    pub search: SearchShape,
    pub panels: PanelDock,
    pub title: TitleStrip,
    /// Row sizes in Standard density.
    pub rows: RowScale,
    pub clip: Clip,
    /// The list is a drawer at this width and under.
    pub drawer_to: Option<f32>,
    pub motion: Motion,
}

/// Today's layout, value for value.
pub const CLASSIC: ChromeMetrics = ChromeMetrics {
    gutter: 0.,
    bar_gap: 0.,
    radius: 0.,
    divider: 1.,
    chat_column: None,
    bar_height: 0.,
    placement: Placement::Stacked,
    backdrop: Backdrop::History,
    selected_row: SelectedRow::Tint,
    folder_tabs: FolderTabs::Underline,
    search: SearchShape::Field,
    panels: PanelDock::Column,
    title: TitleStrip::Lined,
    rows: RowScale::Quill,
    clip: Clip::Square,
    drawer_to: None,
    motion: Motion {
        curve: Curve::OutCubic,
        panel_in_ms: 200,
        panel_out_ms: 200,
        sidebar_ms: 200,
        bar_reveal: BarReveal::Slide,
        menu: MenuEnter::FadeSlide,
        chat_switch: PageSwitch::Cut,
        folder_page: PageSwitch::Cut,
        composer_ms: 0,
        slides: SlideDraw::Live,
    },
};

/// Web K's layout in Quill's units.
pub const FLOATING: ChromeMetrics = ChromeMetrics {
    gutter: 16.,
    bar_gap: 8.,
    radius: 4.,
    divider: 0.,
    chat_column: Some(696.),
    bar_height: 48.,
    placement: Placement::Overlaid,
    backdrop: Backdrop::Window,
    selected_row: SelectedRow::Solid,
    folder_tabs: FolderTabs::Pill,
    search: SearchShape::Pill,
    panels: PanelDock::Card,
    title: TitleStrip::Bare,
    rows: RowScale::WebK,
    clip: Clip::Rounded,
    drawer_to: Some(DRAWER_TO),
    motion: Motion {
        curve: Curve::Standard,
        panel_in_ms: 300,
        panel_out_ms: 250,
        sidebar_ms: 200,
        bar_reveal: BarReveal::Rise,
        menu: MenuEnter::Pop,
        chat_switch: PageSwitch::Cross,
        folder_page: PageSwitch::Cross,
        composer_ms: 250,
        slides: SlideDraw::Snapshot,
    },
};

/// What is drawn: a pure function of the choice and the readings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chrome {
    pub metrics: &'static ChromeMetrics,
    pub edge: Edge,
    pub elevation: Elevation,
}

impl Chrome {
    pub const CLASSIC: Chrome = Chrome {
        metrics: &CLASSIC,
        edge: Edge::None,
        elevation: Elevation::Flat,
    };
}

/// The chrome for `choice` (after the override) in a window that reads
/// `readings`. The first matching row wins (docs/decisions/floating-layout.md).
/// The metrics a window this wide draws: [`resolve`]'s table without
/// the edge and elevation, for callers that have no theme to read (the
/// shell's columns).
pub fn metrics_for(
    stored: LayoutStyle,
    overridden: Option<LayoutOverride>,
    width: f32,
    floating_from: f32,
) -> &'static ChromeMetrics {
    if choice(stored, overridden) == LayoutStyle::Classic || width < floating_from {
        &CLASSIC
    } else {
        &FLOATING
    }
}

pub fn resolve(
    stored: LayoutStyle,
    overridden: Option<LayoutOverride>,
    readings: Readings,
) -> Chrome {
    if std::ptr::eq(
        metrics_for(stored, overridden, readings.width, readings.floating_from),
        &CLASSIC,
    ) {
        return Chrome::CLASSIC;
    }
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

//! Message widgets (TDLib `messageWidget`): what Quill decides about a
//! widget apart from its runtime. The runtime itself (the QuickJS sandbox,
//! the litehtml renderer and Quill's painter) is `src/ui/widget_runtime/`;
//! the bubble is `src/ui/widget_bubble.rs`. Telegram Desktop's behaviour and
//! the design: docs/decisions/message-widgets.md.
#![deny(clippy::indexing_slicing)]

pub mod storage;

use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::TdError;
use std::time::Duration;

/// One widget message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetKey {
    pub chat_id: ChatId,
    pub message_id: MessageId,
}

/// Live sandboxes at once, as in Telegram Desktop (`kMaxAlive`): more widgets
/// on screen than this each still get one; off screen the oldest hidden one
/// is released first.
pub const MAX_LIVE: usize = 8;

/// Fatal stops in a row after which a widget is not started again until its
/// content changes (Telegram Desktop's `kMaxCrashes`).
pub const MAX_CRASHES: u8 = 2;

/// The height a widget with no size of its own takes until it reports one
/// (Telegram Desktop's `tgwPlaceholderHeight`), CSS px.
pub const PLACEHOLDER_HEIGHT: f32 = 160.0;
/// The bounds of a widget's height, CSS px (widget-sandbox's shared bounds).
pub const MIN_HEIGHT: f32 = 50.0;
pub const MAX_HEIGHT: f32 = 600.0;
/// The narrowest a widget with a width of its own is drawn, CSS px.
pub const MIN_WIDTH: f32 = 200.0;

/// The faces behind a widget's generic font families, first installed
/// wins: each OS's interface text (macOS, Windows, GNOME, then common
/// Linux faces), as browsers pick them.
pub const SANS_FONTS: &[&str] = &[
    "Helvetica Neue",
    "Segoe UI",
    "Cantarell",
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Arial",
];
pub const SERIF_FONTS: &[&str] = &[
    "Times",
    "Times New Roman",
    "Noto Serif",
    "DejaVu Serif",
    "Liberation Serif",
];
pub const MONO_FONTS: &[&str] = &[
    "SF Mono",
    "Menlo",
    "Consolas",
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Courier New",
];

/// Where one widget's runtime is. Every stage has a way out: off screen a
/// running widget is released (keeping its last frame) or kept, back on
/// screen a released or stopped one starts again, and new content starts
/// over from `Idle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifecycle {
    /// Never on screen yet: nothing runs and nothing was drawn.
    Idle,
    /// On screen, waiting for one of the [`MAX_LIVE`] sandboxes.
    Waiting,
    /// The sandbox boots and runs the widget's scripts; its first frame has
    /// not come yet.
    Loading,
    /// Running; its frames show.
    Running,
    /// Released off screen to stay within [`MAX_LIVE`]: its last frame stays
    /// and it starts again when it comes back.
    Released,
    /// The runtime stopped itself (a CPU, memory or validator limit): the
    /// last frame with "Widget stopped" over it. It starts again when it
    /// comes back on screen, unless it stopped [`MAX_CRASHES`] times in a row.
    Stopped { reason: String },
    /// It could not start at all (no bundles, the sandbox refused the
    /// document): the card with "Widget stopped".
    Failed { reason: String },
}

impl Lifecycle {
    /// A runtime exists for it now.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Loading | Self::Running)
    }

    /// The bubble shows "Widget stopped".
    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped { .. } | Self::Failed { .. })
    }
}

/// What the pool needs to know about one widget to pick whom to release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolEntry {
    pub key: WidgetKey,
    pub live: bool,
    pub visible: bool,
    /// When it went off screen (a counter), 0 while on screen.
    pub hidden_at: u64,
}

/// The live widget to release so that one more can start, as Telegram
/// Desktop picks it (`FindVictim`): the live one off screen the longest.
/// None when every live widget is on screen (the newcomer then waits).
pub fn victim(entries: &[PoolEntry]) -> Option<WidgetKey> {
    entries
        .iter()
        .filter(|entry| entry.live && !entry.visible)
        .min_by_key(|entry| entry.hidden_at)
        .map(|entry| entry.key)
}

/// How many sandboxes may run: [`MAX_LIVE`], or more while more widgets
/// are on screen at once (each visible one gets one).
pub fn capacity(visible: usize) -> usize {
    MAX_LIVE.max(visible)
}

/// Why a query to the bot did not get its answer, as the widget is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryFailure {
    /// The bot did not answer in time (TDLib's 502).
    Timeout,
    /// A flood wait.
    RateLimited,
    Failed,
}

impl QueryFailure {
    /// The failure `getWidgetQueryAnswer` ended with, as Telegram Desktop
    /// maps it: a timeout, a flood wait, anything else.
    pub fn of(err: &TdError) -> Self {
        if err.code == 502 || err.server_type.as_deref() == Some("WIDGET_RESPONSE_TIMEOUT") {
            Self::Timeout
        } else if err.is_flood() || err.flood_wait_secs.is_some() {
            Self::RateLimited
        } else {
            Self::Failed
        }
    }

    /// The code `qw_respond` takes.
    pub fn code(self) -> i32 {
        match self {
            Self::Timeout => 1,
            Self::RateLimited => 2,
            Self::Failed => 3,
        }
    }
}

/// The answer to one query, as it goes back to the widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryOutcome {
    Answered(String),
    Failed(QueryFailure),
}

/// What TDLib answered for a widget: the session keeps these until the UI
/// takes them ([`crate::state::BotsState::widget_answers`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetAnswer {
    /// `getWidgetQueryAnswer` for the widget's request `token`.
    Query {
        key: WidgetKey,
        token: u64,
        outcome: QueryOutcome,
    },
    /// `getWidgetPublicData`: the shared data and when to ask again, or
    /// `None` when the request failed (the next poll waits a period).
    PublicData {
        key: WidgetKey,
        data: Option<(String, Duration)>,
    },
    /// The widget message was deleted: what the widget kept goes too.
    Deleted { key: WidgetKey },
}

/// Seconds between public data polls when the server names none.
pub const DEFAULT_POLL_PERIOD: Duration = Duration::from_secs(30);

/// The server's `poll_period`, held to Telegram Desktop's bounds
/// (5 s to an hour); the default when it names none.
pub fn poll_period(seconds: i32) -> Duration {
    if seconds <= 0 {
        return DEFAULT_POLL_PERIOD;
    }
    Duration::from_secs(u64::from(seconds.unsigned_abs()).clamp(5, 3600))
}

/// The widget's height in CSS px before it reports one: the bot's fixed
/// height, then the last height the widget had (kept with its storage),
/// then [`PLACEHOLDER_HEIGHT`].
pub fn initial_height(fixed: i32, stored: Option<f32>) -> f32 {
    if fixed > 0 {
        return (fixed as f32).clamp(MIN_HEIGHT, MAX_HEIGHT);
    }
    stored
        .filter(|height| (MIN_HEIGHT..=MAX_HEIGHT).contains(height))
        .unwrap_or(PLACEHOLDER_HEIGHT)
}

/// The card's width in CSS px for a bubble at most `max_width` wide: the
/// bot's width, at least [`MIN_WIDTH`]; the whole bubble for 0.
pub fn card_width(width: i32, max_width: f32) -> f32 {
    if width <= 0 {
        return max_width;
    }
    (width as f32).clamp(MIN_WIDTH.min(max_width), max_width)
}

#[cfg(test)]
mod tests;

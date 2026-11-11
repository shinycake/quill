//! What the panel is open as: closed, composing (over the composer, the
//! story viewer's reply bar or a title box), or one of the two selectors
//! that reuse it, the reaction selector and the emoji status selector.
//!
//! One value, so the panel cannot be both selectors at once (opening the
//! reaction selector over the status selector left the status target set,
//! and the readers disagreed on which one won), nor a selector while
//! closed.
#![deny(clippy::indexing_slicing)]

use super::status::StatusTarget;
use super::*;

/// The panel's mode; see the module docs.
#[derive(Clone, Copy, Default, PartialEq)]
pub(in crate::ui) enum PanelMode {
    #[default]
    Closed,
    /// The emoji / sticker / GIF panel, composing.
    Compose,
    /// Picking a reaction for a message (tdesktop's expanded reaction
    /// selector), anchored where the message menu was.
    Reaction(ReactionTarget),
    /// The emoji status selector, anchored at the main menu's corner.
    Status(StatusTarget),
}

/// What an open panel lists, for the rows' key: the kind of a mode
/// without its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PanelUse {
    Compose,
    Reaction,
    Status,
}

impl PanelMode {
    fn selector(self) -> bool {
        matches!(self, Self::Reaction(_) | Self::Status(_))
    }
}

impl MediaPanel {
    pub(in crate::ui) fn is_open(&self) -> bool {
        self.panel_mode != PanelMode::Closed
    }

    /// The message the reaction selector picks for.
    pub(in crate::ui) fn reaction(&self) -> Option<ReactionTarget> {
        match self.panel_mode {
            PanelMode::Reaction(target) => Some(target),
            _ => None,
        }
    }

    /// Where the status selector is anchored.
    pub(in crate::ui) fn status(&self) -> Option<StatusTarget> {
        match self.panel_mode {
            PanelMode::Status(target) => Some(target),
            _ => None,
        }
    }

    /// What the open panel lists (closed: what it listed composing).
    pub(super) fn usage(&self) -> PanelUse {
        match self.panel_mode {
            PanelMode::Reaction(_) => PanelUse::Reaction,
            PanelMode::Status(_) => PanelUse::Status,
            PanelMode::Closed | PanelMode::Compose => PanelUse::Compose,
        }
    }

    /// Where the panel shows when it is a selector (reaction or status).
    pub(in crate::ui) fn selector_anchor(&self) -> Option<Point<Pixels>> {
        match self.panel_mode {
            PanelMode::Reaction(target) => Some(target.position),
            PanelMode::Status(target) => Some(target.position),
            PanelMode::Closed | PanelMode::Compose => None,
        }
    }
}

impl QuillApp {
    /// Open a selector (reactions, statuses): its mode, on the emoji tab,
    /// from the first section. The GIF gate follows the tab, so a panel
    /// that was on the GIFs tab stops downloading them.
    pub(in crate::ui) fn enter_selector_mode(&mut self, mode: PanelMode) {
        self.set_media_panel_mode(mode);
        let panel = &mut self.pickers.media_panel;
        panel.key = None;
        panel.active_section = 0;
        panel.category = None;
        panel.tab = PanelTab::Emoji;
        self.sync_gif_downloads();
    }

    /// The one writer of the panel's mode. Leaving a selector drops the
    /// rows it listed; leaving the status selector also stops resolving
    /// its choices.
    pub(in crate::ui) fn set_media_panel_mode(&mut self, mode: PanelMode) {
        let old = std::mem::replace(&mut self.pickers.media_panel.panel_mode, mode);
        // Opening the panel again retries a failed category strip.
        if old == PanelMode::Closed
            && let Some(session) = self.session_mut()
        {
            session.stickers.emoji.sticker_categories.clear_error();
        }
        if old.selector() && std::mem::discriminant(&old) != std::mem::discriminant(&mode) {
            self.pickers.media_panel.key = None;
        }
        if matches!(old, PanelMode::Status(_))
            && !matches!(mode, PanelMode::Status(_))
            && let Some(live) = self.live.as_mut()
        {
            live.driver.close_emoji_status_choices();
        }
    }
}

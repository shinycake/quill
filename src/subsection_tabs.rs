//! Subsection tabs: the topic tabs Telegram Desktop shows for bots with
//! topics and for forum supergroups with `has_forum_tabs`
//! (`history_view_subsection_tabs.cpp`). Pure model; the UI lives in
//! `ui/subsection_tabs.rs`.

use serde::{Deserialize, Serialize};

/// Telegram Desktop's `SubsectionTabsMode` (Top = 0, Left = 1, Bottom = 2),
/// saved per chat (`subsectionTabsMode(peerId)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubsectionTabsMode {
    /// A horizontal strip under the chat header.
    #[default]
    Top,
    /// A narrow vertical column left of the history.
    Left,
    /// A horizontal strip just above the composer.
    Bottom,
}

impl SubsectionTabsMode {
    /// The toggle button's cycle, `SubsectionTabs::toggleModes`:
    /// Top → Bottom → Left → Top.
    pub fn next(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Left,
            Self::Left => Self::Top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SubsectionTabsMode;

    #[test]
    fn toggle_cycles_top_bottom_left() {
        let mode = SubsectionTabsMode::default();
        assert_eq!(mode, SubsectionTabsMode::Top);
        assert_eq!(mode.next(), SubsectionTabsMode::Bottom);
        assert_eq!(mode.next().next(), SubsectionTabsMode::Left);
        assert_eq!(mode.next().next().next(), SubsectionTabsMode::Top);
    }
}

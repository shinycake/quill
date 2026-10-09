//! Folder icons and the tab layout options (tdesktop `ui/filter_icons.cpp`,
//! `ui/filter_icon_panel.cpp`, Settings > Folders "Tabs view" / "Tabs
//! appearance"). Pure logic: the UI maps each name to a drawn glyph.

use crate::telegram::envelope::ChatFolderSpec;
use serde::{Deserialize, Serialize};

/// The icon names of `chatFolderIcon` (`schema/td_api.tl:3770`), in the
/// order of tdesktop's icon panel (`kIcons`, six per row).
pub const ICON_NAMES: [&str; 30] = [
    "Cat", "Book", "Money", "Game", "Light", "Like", "Note", "Palette", "Travel", "Sport",
    "Favorite", "Study", "Airplane", "Private", "Groups", "All", "Unread", "Bots", "Crown",
    "Flower", "Home", "Love", "Mask", "Party", "Trade", "Work", "Unmuted", "Channels", "Custom",
    "Setup",
];

/// Icons per row in the picker (tdesktop `kIconsPerRow`).
pub const ICONS_PER_ROW: usize = 6;

/// Whether `name` is one of the schema's icon names.
pub fn is_known_icon(name: &str) -> bool {
    ICON_NAMES.contains(&name)
}

/// tdesktop `ComputeDefaultFilterIcon`: the icon a folder shows when none is
/// chosen, from its rules. TDLib does the same on the server side
/// (`getChatFolderDefaultIconName`); this offline copy drives the editor's
/// "default" preview.
pub fn default_icon_name(spec: &ChatFolderSpec) -> &'static str {
    let types = (
        spec.include_contacts,
        spec.include_non_contacts,
        spec.include_groups,
        spec.include_channels,
        spec.include_bots,
    );
    let none = types == (false, false, false, false, false);
    if !spec.included_chat_ids.is_empty() || !spec.excluded_chat_ids.is_empty() || none {
        return "Custom";
    }
    match types {
        (true, false, false, false, false)
        | (false, true, false, false, false)
        | (true, true, false, false, false) => "Private",
        (false, false, true, false, false) => "Groups",
        (false, false, false, true, false) => "Channels",
        (false, false, false, false, true) => "Bots",
        _ if spec.exclude_read && !spec.exclude_muted => "Unread",
        _ if spec.exclude_muted && !spec.exclude_read => "Unmuted",
        _ => "Custom",
    }
}

/// The icon a tab shows: the folder's icon name when it is a known one,
/// otherwise the generic `Custom` folder (tdesktop falls back the same way
/// for names it does not know).
pub fn display_icon_name(icon_name: &str) -> &str {
    if is_known_icon(icon_name) {
        icon_name
    } else {
        "Custom"
    }
}

/// tdesktop `chatFiltersHorizontal` (inverted): where the folder tabs sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FolderTabsView {
    /// A strip above the chat list.
    #[default]
    Top,
    /// A column of tabs left of the chat list ("Tabs on the left").
    Left,
}

/// tdesktop `ChatsFiltersTabsMode` ("Tabs appearance").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FolderTabsMode {
    /// Text on a top strip, icon with text on a left column.
    #[default]
    Default,
    TextOnly,
    TextAndIcons,
    IconsOnly,
}

impl FolderTabsMode {
    pub const ALL: [FolderTabsMode; 4] = [
        FolderTabsMode::Default,
        FolderTabsMode::TextOnly,
        FolderTabsMode::TextAndIcons,
        FolderTabsMode::IconsOnly,
    ];

    /// tdesktop `lng_filters_tabs_*`.
    pub fn label(self) -> &'static str {
        match self {
            FolderTabsMode::Default => "Default",
            FolderTabsMode::TextOnly => "Text only",
            FolderTabsMode::TextAndIcons => "Text and icons",
            FolderTabsMode::IconsOnly => "Icons only",
        }
    }

    /// Resolve `Default` against the layout: the left column shows icon and
    /// text, the top strip text only (tdesktop: icons live in the vertical
    /// menu by default).
    pub fn resolved(self, view: FolderTabsView) -> (bool, bool) {
        let (icon, text) = match self {
            FolderTabsMode::Default => match view {
                FolderTabsView::Left => (true, true),
                FolderTabsView::Top => (false, true),
            },
            FolderTabsMode::TextOnly => (false, true),
            FolderTabsMode::TextAndIcons => (true, true),
            FolderTabsMode::IconsOnly => (true, false),
        };
        (icon, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_lists_every_schema_icon_once() {
        let schema = [
            "All", "Unread", "Unmuted", "Bots", "Channels", "Groups", "Private", "Custom", "Setup",
            "Cat", "Crown", "Favorite", "Flower", "Game", "Home", "Love", "Mask", "Party", "Sport",
            "Study", "Trade", "Travel", "Work", "Airplane", "Book", "Light", "Like", "Money",
            "Note", "Palette",
        ];
        assert_eq!(ICON_NAMES.len(), schema.len());
        for name in schema {
            assert_eq!(
                ICON_NAMES.iter().filter(|n| **n == name).count(),
                1,
                "{name}"
            );
        }
        assert_eq!(ICON_NAMES.len() % ICONS_PER_ROW, 0);
    }

    #[test]
    fn default_icon_follows_the_rules() {
        let mut spec = ChatFolderSpec::default();
        assert_eq!(default_icon_name(&spec), "Custom");
        spec.include_contacts = true;
        assert_eq!(default_icon_name(&spec), "Private");
        spec.include_non_contacts = true;
        assert_eq!(default_icon_name(&spec), "Private");
        spec.include_groups = true;
        assert_eq!(default_icon_name(&spec), "Custom");
        let mut groups = ChatFolderSpec {
            include_groups: true,
            ..ChatFolderSpec::default()
        };
        assert_eq!(default_icon_name(&groups), "Groups");
        groups.included_chat_ids.push(7);
        assert_eq!(default_icon_name(&groups), "Custom");
        let all = ChatFolderSpec {
            include_contacts: true,
            include_non_contacts: true,
            include_groups: true,
            include_channels: true,
            include_bots: true,
            exclude_read: true,
            ..ChatFolderSpec::default()
        };
        assert_eq!(default_icon_name(&all), "Unread");
        let muted = ChatFolderSpec {
            exclude_muted: true,
            exclude_read: false,
            ..all
        };
        assert_eq!(default_icon_name(&muted), "Unmuted");
    }

    #[test]
    fn unknown_icon_names_fall_back_to_custom() {
        assert_eq!(display_icon_name("Work"), "Work");
        assert_eq!(display_icon_name("Rocket"), "Custom");
        assert_eq!(display_icon_name(""), "Custom");
    }

    #[test]
    fn tab_modes_resolve_per_layout() {
        use FolderTabsMode as M;
        use FolderTabsView as V;
        assert_eq!(M::Default.resolved(V::Top), (false, true));
        assert_eq!(M::Default.resolved(V::Left), (true, true));
        assert_eq!(M::TextOnly.resolved(V::Left), (false, true));
        assert_eq!(M::TextAndIcons.resolved(V::Top), (true, true));
        assert_eq!(M::IconsOnly.resolved(V::Top), (true, false));
    }
}

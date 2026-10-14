//! The macOS Dock icon menu, after Telegram Desktop's `Menu::RefreshDockMenu`
//! (`menu/menu_dock.cpp`): with more than one account (and the app not
//! locked) a "Profiles" header and one row per signed-in account, then
//! "Disable notifications" / "Enable notifications", then the player's
//! Previous / Pause or Resume / Next while a track is loaded.
//!
//! This file is the pure part: what the menu holds for a given state. The
//! `MenuItem`s and their actions are built in `super`.

use quill::ids::AccountKey;

/// tdesktop `kMaxLength`: longer account names are cut and get an ellipsis.
pub(crate) const MAX_NAME_UTF16: usize = 30;

/// One account the Dock menu offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DockAccount {
    pub(crate) key: AccountKey,
    pub(crate) name: String,
}

/// The player rows, shown while a track is loaded (playing or paused).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DockPlayer {
    pub(crate) paused: bool,
    pub(crate) can_previous: bool,
    pub(crate) can_next: bool,
}

/// Everything the Dock menu depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DockModel {
    /// Every registered account, in registry order.
    pub(crate) registered: usize,
    /// The signed-in accounts, in registry order.
    pub(crate) accounts: Vec<DockAccount>,
    /// The local passcode lock covers the window.
    pub(crate) locked: bool,
    /// Desktop notifications are on.
    pub(crate) notifications: bool,
    pub(crate) player: Option<DockPlayer>,
}

/// One row of the Dock menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DockEntry {
    Separator,
    /// The disabled "Profiles" header.
    ProfilesHeader,
    Account {
        key: AccountKey,
        label: String,
    },
    /// "Disable notifications" when on, "Enable notifications" when off.
    ToggleNotifications {
        on: bool,
    },
    Previous {
        enabled: bool,
    },
    /// "Pause" while playing, "Resume" while paused.
    PlayPause {
        paused: bool,
    },
    Next {
        enabled: bool,
    },
}

impl DockEntry {
    /// The row's text (tdesktop `lng_mac_menu_*` and the tray strings).
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Separator => String::new(),
            Self::ProfilesHeader => "Profiles".into(),
            Self::Account { label, .. } => label.clone(),
            Self::ToggleNotifications { on: true } => "Disable notifications".into(),
            Self::ToggleNotifications { on: false } => "Enable notifications".into(),
            Self::Previous { .. } => "Previous".into(),
            Self::PlayPause { paused: true } => "Resume".into(),
            Self::PlayPause { paused: false } => "Pause".into(),
            Self::Next { .. } => "Next".into(),
        }
    }
}

/// The Dock menu for `model`, top to bottom.
pub(crate) fn dock_entries(model: &DockModel) -> Vec<DockEntry> {
    let mut entries = Vec::new();
    if model.registered > 1 && !model.locked {
        entries.push(DockEntry::Separator);
        entries.push(DockEntry::ProfilesHeader);
        entries.extend(model.accounts.iter().map(|account| DockEntry::Account {
            key: account.key.clone(),
            label: shorten_name(&account.name),
        }));
        entries.push(DockEntry::Separator);
    }
    entries.push(DockEntry::ToggleNotifications {
        on: model.notifications,
    });
    if let Some(player) = model.player {
        entries.push(DockEntry::Separator);
        entries.push(DockEntry::Previous {
            enabled: player.can_previous,
        });
        entries.push(DockEntry::PlayPause {
            paused: player.paused,
        });
        entries.push(DockEntry::Next {
            enabled: player.can_next,
        });
    }
    entries
}

/// tdesktop cuts a name longer than 30 UTF-16 units to its first 30 and adds
/// "…". The cut never splits a character here.
pub(crate) fn shorten_name(name: &str) -> String {
    if name.encode_utf16().count() <= MAX_NAME_UTF16 {
        return name.to_string();
    }
    let mut units = 0usize;
    let mut out = String::new();
    for ch in name.chars() {
        units = units.saturating_add(ch.len_utf16());
        if units > MAX_NAME_UTF16 {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{
        DockAccount, DockEntry, DockModel, DockPlayer, MAX_NAME_UTF16, dock_entries, shorten_name,
    };
    use quill::ids::AccountKey;

    fn account(key: &str, name: &str) -> DockAccount {
        DockAccount {
            key: AccountKey(key.into()),
            name: name.into(),
        }
    }

    #[test]
    fn one_account_shows_only_the_notification_toggle() {
        let model = DockModel {
            registered: 1,
            accounts: vec![account("primary", "Ada")],
            notifications: true,
            ..DockModel::default()
        };
        assert_eq!(
            dock_entries(&model),
            vec![DockEntry::ToggleNotifications { on: true }]
        );
        assert_eq!(dock_entries(&model)[0].label(), "Disable notifications");
    }

    #[test]
    fn several_accounts_get_a_profiles_section_unless_locked() {
        let mut model = DockModel {
            registered: 3,
            accounts: vec![account("primary", "Ada"), account("work", "Grace")],
            notifications: false,
            ..DockModel::default()
        };
        let entries = dock_entries(&model);
        assert_eq!(
            entries,
            vec![
                DockEntry::Separator,
                DockEntry::ProfilesHeader,
                DockEntry::Account {
                    key: AccountKey("primary".into()),
                    label: "Ada".into()
                },
                DockEntry::Account {
                    key: AccountKey("work".into()),
                    label: "Grace".into()
                },
                DockEntry::Separator,
                DockEntry::ToggleNotifications { on: false },
            ]
        );
        assert_eq!(entries[5].label(), "Enable notifications");
        model.locked = true;
        assert_eq!(
            dock_entries(&model),
            vec![DockEntry::ToggleNotifications { on: false }]
        );
    }

    #[test]
    fn player_rows_follow_the_track_state() {
        let model = DockModel {
            registered: 1,
            notifications: true,
            player: Some(DockPlayer {
                paused: true,
                can_previous: false,
                can_next: true,
            }),
            ..DockModel::default()
        };
        let entries = dock_entries(&model);
        assert_eq!(
            entries,
            vec![
                DockEntry::ToggleNotifications { on: true },
                DockEntry::Separator,
                DockEntry::Previous { enabled: false },
                DockEntry::PlayPause { paused: true },
                DockEntry::Next { enabled: true },
            ]
        );
        assert_eq!(entries[3].label(), "Resume");
        assert_eq!(DockEntry::PlayPause { paused: false }.label(), "Pause");
    }

    #[test]
    fn long_names_are_cut_at_thirty_utf16_units() {
        assert_eq!(shorten_name("Short"), "Short");
        let exact = "a".repeat(MAX_NAME_UTF16);
        assert_eq!(shorten_name(&exact), exact);
        let long = "b".repeat(MAX_NAME_UTF16 + 5);
        assert_eq!(
            shorten_name(&long),
            format!("{}…", "b".repeat(MAX_NAME_UTF16))
        );
        // An emoji takes two UTF-16 units and is never split.
        let emoji = format!("{}😀😀", "c".repeat(MAX_NAME_UTF16 - 1));
        assert_eq!(
            shorten_name(&emoji),
            format!("{}…", "c".repeat(MAX_NAME_UTF16 - 1))
        );
    }
}

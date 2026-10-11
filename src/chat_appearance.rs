//! Appearance of a channel or supergroup (tdesktop
//! `boxes/peers/edit_peer_color_box.cpp`, opened from the chat's edit box
//! as "Appearance"): name color and reply icon (channels only), profile
//! color and profile icon, and the emoji status.
//!
//! Unlike the account's own colors, nothing here needs Telegram Premium.
//! The chat needs boosts instead: every part has a minimum boost level
//! (`PeerColors::requiredLevelFor` and `LevelLimits` in tdesktop). Apply
//! checks the chat's current level first; when it is too low, nothing is
//! sent and the box asks for boosts ("Level N required").

use crate::telegram::envelope::{ParsedBoostFeatures, ParsedBoostLevelFeatures};
use crate::telegram::{NameAccentColor, ProfileAccentColor};

/// Which kind of chat is edited. Basic groups cannot be boosted and have no
/// appearance box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAppearanceKind {
    Channel,
    Supergroup,
}

/// Fallbacks when the level table of the chat has not arrived: tdesktop's
/// defaults for `channel_bg_icon_level_min`, `channel_profile_bg_icon_level_min`,
/// `group_profile_bg_icon_level_min` and the emoji status minimums.
const DEFAULT_BG_ICON_LEVEL: i32 = 4;
const DEFAULT_CHANNEL_PROFILE_ICON_LEVEL: i32 = 7;
const DEFAULT_GROUP_PROFILE_ICON_LEVEL: i32 = 5;
const DEFAULT_STATUS_LEVEL: i32 = 8;
/// tdesktop's answer for a color the palette does not list.
const UNLISTED_COLOR_LEVEL: i32 = 1;

/// What the user edits. `-1` is "no profile color", `0` is "no icon" and
/// "no status".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatAppearanceDraft {
    pub name_color_id: i32,
    pub name_emoji_id: i64,
    pub profile_color_id: i32,
    pub profile_emoji_id: i64,
    pub status_emoji_id: i64,
}

/// Which section of the box a thing belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatAppearanceTab {
    #[default]
    Profile,
    Name,
}

/// Minimum boost levels of the icon and status features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureLevels {
    pub reply_icon: i32,
    pub profile_icon: i32,
    pub status: i32,
}

impl FeatureLevels {
    /// From the chat's level table (`getChatBoostFeatures`): the first level
    /// that unlocks each feature. Missing table: tdesktop's defaults.
    pub fn from_features(features: Option<&ParsedBoostFeatures>, kind: ChatAppearanceKind) -> Self {
        let first = |unlocks: fn(&ParsedBoostLevelFeatures) -> bool, fallback: i32| {
            features
                .and_then(|table| table.levels.iter().find(|level| unlocks(level)))
                .map_or(fallback, |level| level.level)
        };
        Self {
            reply_icon: first(|f| f.can_set_background_custom_emoji, DEFAULT_BG_ICON_LEVEL),
            profile_icon: first(
                |f| f.can_set_profile_background_custom_emoji,
                match kind {
                    ChatAppearanceKind::Channel => DEFAULT_CHANNEL_PROFILE_ICON_LEVEL,
                    ChatAppearanceKind::Supergroup => DEFAULT_GROUP_PROFILE_ICON_LEVEL,
                },
            ),
            status: first(|f| f.can_set_emoji_status, DEFAULT_STATUS_LEVEL),
        }
    }
}

/// The level a channel needs for name color `id`: none for the color it
/// already has, the palette's minimum otherwise (1 when unlisted).
pub fn name_color_level(id: i32, current_id: i32, palette: &[NameAccentColor]) -> i32 {
    if id == current_id {
        return 0;
    }
    palette
        .iter()
        .find(|color| color.id == id)
        .map_or(UNLISTED_COLOR_LEVEL, |color| color.min_channel_level)
}

/// The level a chat needs for profile color `id`. "No color" is free.
pub fn profile_color_level(
    id: i32,
    current_id: i32,
    kind: ChatAppearanceKind,
    palette: &[ProfileAccentColor],
) -> i32 {
    if id == current_id || id < 0 {
        return 0;
    }
    palette
        .iter()
        .find(|color| color.id == id)
        .map_or(UNLISTED_COLOR_LEVEL, |color| match kind {
            ChatAppearanceKind::Channel => color.min_channel_level,
            ChatAppearanceKind::Supergroup => color.min_supergroup_level,
        })
}

/// What a draft needs, per part, as boost levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequiredLevels {
    pub name: i32,
    pub profile: i32,
    pub status: i32,
}

impl RequiredLevels {
    pub fn max(&self) -> i32 {
        self.name.max(self.profile).max(self.status)
    }
}

/// The part of the palettes and tables the rules read.
pub struct AppearanceRules<'a> {
    pub kind: ChatAppearanceKind,
    pub features: FeatureLevels,
    pub name_palette: &'a [NameAccentColor],
    pub profile_palette: &'a [ProfileAccentColor],
}

/// What Apply sends: `None` for a part that did not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyPlan {
    /// `setChatAccentColor`: color id and reply icon.
    pub name: Option<(i32, i64)>,
    /// `setChatProfileAccentColor`: color id (-1 none) and profile icon.
    pub profile: Option<(i32, i64)>,
    /// `setChatEmojiStatus`: the custom emoji, `None` to remove.
    pub status: Option<Option<i64>>,
}

/// What pressing Apply does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Nothing changed: just close.
    Unchanged,
    /// The chat's boost level has not arrived yet; ask for it and wait.
    LevelUnknown,
    /// The chat has too few boosts for the changed parts (send nothing).
    NeedsBoosts {
        required: i32,
        have: i32,
    },
    Send(ApplyPlan),
}

impl ChatAppearanceDraft {
    /// Levels the changed parts need. Unchanged parts need nothing.
    pub fn required_levels(&self, current: &Self, rules: &AppearanceRules<'_>) -> RequiredLevels {
        let mut required = RequiredLevels::default();
        if rules.kind == ChatAppearanceKind::Channel
            && (self.name_color_id != current.name_color_id
                || self.name_emoji_id != current.name_emoji_id)
        {
            let icon = if self.name_emoji_id != 0 {
                rules.features.reply_icon
            } else {
                0
            };
            required.name = name_color_level(
                self.name_color_id,
                current.name_color_id,
                rules.name_palette,
            )
            .max(icon);
        }
        if self.profile_color_id != current.profile_color_id
            || self.profile_emoji_id != current.profile_emoji_id
        {
            let icon = if self.profile_emoji_id != 0 {
                rules.features.profile_icon
            } else {
                0
            };
            required.profile = profile_color_level(
                self.profile_color_id,
                current.profile_color_id,
                rules.kind,
                rules.profile_palette,
            )
            .max(icon);
        }
        if self.status_emoji_id != current.status_emoji_id && self.status_emoji_id != 0 {
            required.status = rules.features.status;
        }
        required
    }

    /// Apply against the chat's `current` look and boost `level` (`None`
    /// until `getChatBoostStatus` answered).
    pub fn apply_outcome(
        &self,
        current: &Self,
        level: Option<i32>,
        rules: &AppearanceRules<'_>,
    ) -> ApplyOutcome {
        let name = rules.kind == ChatAppearanceKind::Channel
            && (self.name_color_id != current.name_color_id
                || self.name_emoji_id != current.name_emoji_id);
        let profile = self.profile_color_id != current.profile_color_id
            || self.profile_emoji_id != current.profile_emoji_id;
        let status = self.status_emoji_id != current.status_emoji_id;
        if !name && !profile && !status {
            return ApplyOutcome::Unchanged;
        }
        let Some(have) = level else {
            return ApplyOutcome::LevelUnknown;
        };
        let required = self.required_levels(current, rules).max();
        if have < required {
            return ApplyOutcome::NeedsBoosts { required, have };
        }
        ApplyOutcome::Send(ApplyPlan {
            name: name.then_some((self.name_color_id, self.name_emoji_id)),
            profile: profile.then_some((self.profile_color_id, self.profile_emoji_id)),
            status: status.then_some((self.status_emoji_id != 0).then_some(self.status_emoji_id)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_color(id: i32, min: i32) -> NameAccentColor {
        NameAccentColor {
            id,
            built_in_id: 0,
            light_colors: vec![],
            dark_colors: vec![],
            min_channel_level: min,
        }
    }

    fn profile_color(id: i32, group: i32, channel: i32) -> ProfileAccentColor {
        ProfileAccentColor {
            id,
            light_colors: vec![],
            dark_colors: vec![],
            min_supergroup_level: group,
            min_channel_level: channel,
        }
    }

    fn table() -> ParsedBoostFeatures {
        let level = |level, bg, profile, status| ParsedBoostLevelFeatures {
            level,
            can_set_background_custom_emoji: bg,
            can_set_profile_background_custom_emoji: profile,
            can_set_emoji_status: status,
            ..ParsedBoostLevelFeatures::default()
        };
        ParsedBoostFeatures {
            levels: vec![
                level(1, false, false, false),
                level(3, true, false, false),
                level(6, true, true, false),
                level(9, true, true, true),
            ],
        }
    }

    fn draft() -> ChatAppearanceDraft {
        ChatAppearanceDraft {
            name_color_id: 2,
            name_emoji_id: 0,
            profile_color_id: -1,
            profile_emoji_id: 0,
            status_emoji_id: 0,
        }
    }

    fn rules<'a>(
        kind: ChatAppearanceKind,
        features: &'a ParsedBoostFeatures,
        names: &'a [NameAccentColor],
        profiles: &'a [ProfileAccentColor],
    ) -> AppearanceRules<'a> {
        AppearanceRules {
            kind,
            features: FeatureLevels::from_features(Some(features), kind),
            name_palette: names,
            profile_palette: profiles,
        }
    }

    #[test]
    fn feature_levels_come_from_the_level_table_or_tdesktop_defaults() {
        let from_table = FeatureLevels::from_features(Some(&table()), ChatAppearanceKind::Channel);
        assert_eq!(
            from_table,
            FeatureLevels {
                reply_icon: 3,
                profile_icon: 6,
                status: 9
            }
        );
        let channel = FeatureLevels::from_features(None, ChatAppearanceKind::Channel);
        assert_eq!(
            (channel.reply_icon, channel.profile_icon, channel.status),
            (4, 7, 8)
        );
        let group = FeatureLevels::from_features(None, ChatAppearanceKind::Supergroup);
        assert_eq!(group.profile_icon, 5);
    }

    #[test]
    fn colors_cost_their_palette_level_except_the_current_one() {
        let names = [name_color(2, 0), name_color(8, 5)];
        assert_eq!(name_color_level(2, 2, &names), 0);
        assert_eq!(name_color_level(8, 2, &names), 5);
        assert_eq!(name_color_level(30, 2, &names), 1);
        let profiles = [profile_color(1, 2, 6)];
        let kinds = (ChatAppearanceKind::Channel, ChatAppearanceKind::Supergroup);
        assert_eq!(profile_color_level(1, -1, kinds.0, &profiles), 6);
        assert_eq!(profile_color_level(1, -1, kinds.1, &profiles), 2);
        assert_eq!(profile_color_level(-1, 1, kinds.0, &profiles), 0);
        assert_eq!(profile_color_level(1, 1, kinds.0, &profiles), 0);
        assert_eq!(profile_color_level(9, -1, kinds.0, &profiles), 1);
    }

    #[test]
    fn an_unchanged_draft_just_closes_even_before_the_level_is_known() {
        let features = table();
        let rules = rules(ChatAppearanceKind::Channel, &features, &[], &[]);
        assert_eq!(
            draft().apply_outcome(&draft(), None, &rules),
            ApplyOutcome::Unchanged
        );
    }

    #[test]
    fn a_change_waits_for_the_boost_level() {
        let features = table();
        let rules = rules(ChatAppearanceKind::Channel, &features, &[], &[]);
        let mut edited = draft();
        edited.name_emoji_id = 5;
        assert_eq!(
            edited.apply_outcome(&draft(), None, &rules),
            ApplyOutcome::LevelUnknown
        );
    }

    #[test]
    fn too_few_boosts_send_nothing_and_name_the_highest_level() {
        let features = table();
        let names = [name_color_id_free(), name_color(8, 5)];
        let profiles = [profile_color(3, 1, 4)];
        let rules = rules(ChatAppearanceKind::Channel, &features, &names, &profiles);
        let mut edited = draft();
        edited.name_color_id = 8; // level 5
        edited.name_emoji_id = 11; // reply icon: level 3
        edited.profile_color_id = 3; // channel level 4
        edited.profile_emoji_id = 12; // profile icon: level 6
        edited.status_emoji_id = 13; // status: level 9
        let required = edited.required_levels(&draft(), &rules);
        assert_eq!(
            required,
            RequiredLevels {
                name: 5,
                profile: 6,
                status: 9
            }
        );
        assert_eq!(
            edited.apply_outcome(&draft(), Some(8), &rules),
            ApplyOutcome::NeedsBoosts {
                required: 9,
                have: 8
            }
        );
        assert!(matches!(
            edited.apply_outcome(&draft(), Some(9), &rules),
            ApplyOutcome::Send(_)
        ));
    }

    fn name_color_id_free() -> NameAccentColor {
        name_color(2, 0)
    }

    #[test]
    fn only_changed_parts_are_sent() {
        let features = table();
        let profiles = [profile_color(3, 1, 4)];
        let rules = rules(ChatAppearanceKind::Channel, &features, &[], &profiles);
        let mut edited = draft();
        edited.profile_color_id = 3;
        let ApplyOutcome::Send(plan) = edited.apply_outcome(&draft(), Some(4), &rules) else {
            panic!("level 4 is enough for the channel color");
        };
        assert_eq!(plan.name, None);
        assert_eq!(plan.profile, Some((3, 0)));
        assert_eq!(plan.status, None);

        // Removing a status needs no level at all.
        let mut current = draft();
        current.status_emoji_id = 40;
        let ApplyOutcome::Send(plan) = draft().apply_outcome(&current, Some(0), &rules) else {
            panic!("removing a status is free");
        };
        assert_eq!(plan.status, Some(None));
    }

    #[test]
    fn supergroups_have_no_name_section_and_use_group_levels() {
        let features = table();
        let profiles = [profile_color(3, 1, 4)];
        let rules = rules(ChatAppearanceKind::Supergroup, &features, &[], &profiles);
        let mut edited = draft();
        edited.name_color_id = 5;
        edited.name_emoji_id = 9;
        assert_eq!(
            edited.apply_outcome(&draft(), None, &rules),
            ApplyOutcome::Unchanged
        );
        edited.profile_color_id = 3;
        let ApplyOutcome::Send(plan) = edited.apply_outcome(&draft(), Some(1), &rules) else {
            panic!("supergroup profile color 3 needs level 1");
        };
        assert_eq!(plan.name, None);
        assert_eq!(plan.profile, Some((3, 0)));
    }
}

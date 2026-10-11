//! `chatBoostStatus` details and `chatBoostFeatures` (the level table of
//! tdesktop's boost box, `ui/boxes/boost_box.cpp` `AddFeaturesList`, and
//! the overview of `info/channel_statistics/boosts/`).
use super::*;
use serde_json::Value;

/// `chatBoostStatus` (schema/td_api.tl:7278) without the slot ids and
/// prepaid giveaways.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedBoostStatus {
    pub level: i32,
    pub boost_count: i32,
    pub gift_code_boost_count: i32,
    /// Boosts that were needed to reach the current level.
    pub current_level_boost_count: i32,
    /// Boosts needed for the next level; 0 when there is none.
    pub next_level_boost_count: i32,
    pub premium_member_count: i32,
    pub premium_member_percentage: f64,
}

impl ParsedBoostStatus {
    /// Share of the way from the current level to the next, 0.0 to 1.0.
    /// A chat at its top level (or with no next-level target) is full.
    pub fn level_fraction(&self) -> f64 {
        if self.next_level_boost_count <= 0 {
            return 1.0;
        }
        let span = self.next_level_boost_count - self.current_level_boost_count;
        if span <= 0 {
            return 1.0;
        }
        let done = self.boost_count - self.current_level_boost_count;
        (f64::from(done) / f64::from(span)).clamp(0.0, 1.0)
    }

    /// Boosts still missing for the next level (tdesktop "Boosts to level
    /// up"); `None` at the top level.
    pub fn boosts_to_next_level(&self) -> Option<i32> {
        (self.next_level_boost_count > 0)
            .then(|| (self.next_level_boost_count - self.boost_count).max(0))
    }

    /// "Premium subscribers" value, e.g. "12 (3.5%)".
    pub fn premium_audience_text(&self) -> String {
        let pct = self.premium_member_percentage;
        let pct = if pct.fract().abs() < 0.05 {
            format!("{pct:.0}%")
        } else {
            format!("{pct:.1}%")
        };
        format!("{} ({pct})", self.premium_member_count)
    }
}

pub(crate) fn parse_boost_status(value: &Value) -> ParsedBoostStatus {
    let num = |name: &str| -> i32 { int53(value.get(name)).unwrap_or(0).sat_i32() };
    ParsedBoostStatus {
        level: num("level"),
        boost_count: num("boost_count"),
        gift_code_boost_count: num("gift_code_boost_count"),
        current_level_boost_count: num("current_level_boost_count"),
        next_level_boost_count: num("next_level_boost_count"),
        premium_member_count: num("premium_member_count"),
        premium_member_percentage: value
            .get("premium_member_percentage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    }
}

/// `chatBoostLevelFeatures` (schema/td_api.tl:7223): what a chat can do
/// at one level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedBoostLevelFeatures {
    pub level: i32,
    pub story_per_day_count: i32,
    pub custom_emoji_reaction_count: i32,
    pub title_color_count: i32,
    pub profile_accent_color_count: i32,
    pub can_set_profile_background_custom_emoji: bool,
    pub accent_color_count: i32,
    pub can_set_background_custom_emoji: bool,
    pub can_set_emoji_status: bool,
    pub chat_theme_background_count: i32,
    pub can_set_custom_background: bool,
    pub can_set_custom_emoji_sticker_set: bool,
    pub can_enable_automatic_translation: bool,
    pub can_recognize_speech: bool,
    pub can_disable_sponsored_messages: bool,
}

impl ParsedBoostLevelFeatures {
    /// What this level adds over `previous` (the level below; `None` for
    /// the first one), worded for a channel or a group. Mirrors the rows
    /// of tdesktop's "Level N Unlocks:" list. Counts that grow (stories,
    /// reactions) show the new total; colors and wallpapers show how many
    /// are added.
    pub fn unlocks(&self, previous: Option<&Self>, group: bool) -> Vec<String> {
        let base = Self::default();
        let before = previous.unwrap_or(&base);
        let added = |now: i32, then: i32| (now > then).then_some(now - then);
        let mut lines = Vec::new();
        if self.story_per_day_count > before.story_per_day_count {
            lines.push(plural(
                self.story_per_day_count,
                "story per day",
                "stories per day",
            ));
        }
        if self.custom_emoji_reaction_count > before.custom_emoji_reaction_count {
            lines.push(plural(
                self.custom_emoji_reaction_count,
                "custom emoji reaction",
                "custom emoji reactions",
            ));
        }
        if let Some(n) = added(self.title_color_count, before.title_color_count) {
            lines.push(plural(n, "name color", "name colors"));
        }
        if !group && let Some(n) = added(self.accent_color_count, before.accent_color_count) {
            lines.push(plural(n, "link style", "link styles"));
        }
        if let Some(n) = added(
            self.profile_accent_color_count,
            before.profile_accent_color_count,
        ) {
            lines.push(plural(n, "profile color", "profile colors"));
        }
        if self.can_set_profile_background_custom_emoji
            && !before.can_set_profile_background_custom_emoji
        {
            lines.push("Profile logo".into());
        }
        if self.can_set_background_custom_emoji && !before.can_set_background_custom_emoji {
            lines.push("Link logo".into());
        }
        if self.can_set_emoji_status && !before.can_set_emoji_status {
            lines.push("Emoji status".into());
        }
        if let Some(n) = added(
            self.chat_theme_background_count,
            before.chat_theme_background_count,
        ) {
            lines.push(plural(n, "wallpaper", "wallpapers"));
        }
        if self.can_set_custom_background && !before.can_set_custom_background {
            lines.push("Custom wallpaper".into());
        }
        if group
            && self.can_set_custom_emoji_sticker_set
            && !before.can_set_custom_emoji_sticker_set
        {
            lines.push("Custom emoji pack".into());
        }
        if self.can_enable_automatic_translation && !before.can_enable_automatic_translation {
            lines.push("Autotranslation of messages".into());
        }
        if self.can_recognize_speech && !before.can_recognize_speech {
            lines.push("Voice-to-text conversion".into());
        }
        if !group && self.can_disable_sponsored_messages && !before.can_disable_sponsored_messages {
            lines.push("Switch off ads".into());
        }
        lines
    }
}

fn plural(count: i32, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// `chatBoostFeatures` (schema/td_api.tl:7236): the per-level table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedBoostFeatures {
    pub levels: Vec<ParsedBoostLevelFeatures>,
}

impl ParsedBoostFeatures {
    /// The rows of the table: each level with what it unlocks, skipping
    /// levels that unlock nothing new.
    pub fn table(&self, group: bool) -> Vec<(i32, Vec<String>)> {
        let mut rows = Vec::new();
        let mut previous = None;
        for level in &self.levels {
            let unlocks = level.unlocks(previous, group);
            if !unlocks.is_empty() {
                rows.push((level.level, unlocks));
            }
            previous = Some(level);
        }
        rows
    }
}

fn parse_level_features(value: &Value) -> ParsedBoostLevelFeatures {
    let num = |name: &str| -> i32 { int53(value.get(name)).unwrap_or(0).sat_i32() };
    let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    ParsedBoostLevelFeatures {
        level: num("level"),
        story_per_day_count: num("story_per_day_count"),
        custom_emoji_reaction_count: num("custom_emoji_reaction_count"),
        title_color_count: num("title_color_count"),
        profile_accent_color_count: num("profile_accent_color_count"),
        can_set_profile_background_custom_emoji: flag("can_set_profile_background_custom_emoji"),
        accent_color_count: num("accent_color_count"),
        can_set_background_custom_emoji: flag("can_set_background_custom_emoji"),
        can_set_emoji_status: flag("can_set_emoji_status"),
        chat_theme_background_count: num("chat_theme_background_count"),
        can_set_custom_background: flag("can_set_custom_background"),
        can_set_custom_emoji_sticker_set: flag("can_set_custom_emoji_sticker_set"),
        can_enable_automatic_translation: flag("can_enable_automatic_translation"),
        can_recognize_speech: flag("can_recognize_speech"),
        can_disable_sponsored_messages: flag("can_disable_sponsored_messages"),
    }
}

pub(crate) fn parse_boost_features(value: &Value) -> ParsedBoostFeatures {
    let mut levels: Vec<_> = value
        .get("features")
        .and_then(Value::as_array)
        .map(|list| list.iter().map(parse_level_features).collect())
        .unwrap_or_default();
    levels.sort_by_key(|level| level.level);
    ParsedBoostFeatures { levels }
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedBoostFeatures, ParsedBoostStatus, Value, parse_boost_features, parse_boost_status,
    };

    fn status(boosts: i32, current: i32, next: i32) -> ParsedBoostStatus {
        ParsedBoostStatus {
            level: 2,
            boost_count: boosts,
            current_level_boost_count: current,
            next_level_boost_count: next,
            ..ParsedBoostStatus::default()
        }
    }

    #[test]
    fn level_fraction_is_progress_between_levels() {
        assert_eq!(status(15, 10, 20).level_fraction(), 0.5);
        assert_eq!(status(10, 10, 20).level_fraction(), 0.0);
        assert_eq!(status(25, 10, 20).level_fraction(), 1.0);
        assert_eq!(status(5, 10, 20).level_fraction(), 0.0);
    }

    #[test]
    fn top_level_is_full_with_nothing_to_go() {
        let top = status(500, 400, 0);
        assert_eq!(top.level_fraction(), 1.0);
        assert_eq!(top.boosts_to_next_level(), None);
        assert_eq!(status(15, 10, 20).boosts_to_next_level(), Some(5));
        assert_eq!(status(30, 10, 20).boosts_to_next_level(), Some(0));
    }

    #[test]
    fn premium_audience_text_trims_whole_percentages() {
        let mut s = ParsedBoostStatus {
            premium_member_count: 12,
            premium_member_percentage: 3.5,
            ..ParsedBoostStatus::default()
        };
        assert_eq!(s.premium_audience_text(), "12 (3.5%)");
        s.premium_member_percentage = 4.0;
        assert_eq!(s.premium_audience_text(), "12 (4%)");
    }

    #[test]
    fn parses_status_details() {
        let v: Value = serde_json::from_str(
            r#"{"@type":"chatBoostStatus","level":3,"boost_count":42,"gift_code_boost_count":5,
                "current_level_boost_count":30,"next_level_boost_count":50,
                "premium_member_count":9,"premium_member_percentage":1.25}"#,
        )
        .unwrap();
        let s = parse_boost_status(&v);
        assert_eq!(
            (s.level, s.boost_count, s.gift_code_boost_count),
            (3, 42, 5)
        );
        assert_eq!(s.boosts_to_next_level(), Some(8));
        assert_eq!(s.premium_member_count, 9);
        assert_eq!(s.premium_member_percentage, 1.25);
    }

    fn features() -> ParsedBoostFeatures {
        let v: Value = serde_json::from_str(
            r#"{"@type":"chatBoostFeatures","features":[
              {"level":2,"story_per_day_count":2,"title_color_count":7,"can_set_emoji_status":true},
              {"level":1,"story_per_day_count":1,"title_color_count":7},
              {"level":3,"story_per_day_count":3,"title_color_count":7,"can_set_emoji_status":true},
              {"level":4,"story_per_day_count":3,"title_color_count":9,"can_set_emoji_status":true,
               "can_disable_sponsored_messages":true}]}"#,
        )
        .unwrap();
        parse_boost_features(&v)
    }

    #[test]
    fn features_are_sorted_by_level() {
        let levels: Vec<i32> = features().levels.iter().map(|l| l.level).collect();
        assert_eq!(levels, [1, 2, 3, 4]);
    }

    #[test]
    fn table_lists_only_what_each_level_adds() {
        let table = features().table(false);
        assert_eq!(
            table[0],
            (
                1,
                vec!["1 story per day".to_string(), "7 name colors".into()]
            )
        );
        assert_eq!(
            table[1],
            (
                2,
                vec!["2 stories per day".to_string(), "Emoji status".into()]
            )
        );
        assert_eq!(table[2], (3, vec!["3 stories per day".to_string()]));
        assert_eq!(
            table[3],
            (
                4,
                vec!["2 name colors".to_string(), "Switch off ads".into()]
            )
        );
    }

    #[test]
    fn groups_do_not_get_the_channel_only_rows() {
        let table = features().table(true);
        assert_eq!(table[3], (4, vec!["2 name colors".to_string()]));
    }
}

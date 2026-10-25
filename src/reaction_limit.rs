//! How many reactions one user may leave on a message, and the local trim
//! when one more would pass that.
//!
//! The limit is the app config's `reactions_user_max_premium` or
//! `reactions_user_max_default` (tdesktop `SentReactionsLimit`,
//! `data/data_message_reactions.cpp`). TDLib keeps the options of the same
//! name internal, so Quill reads them from `getApplicationConfig`; until that
//! answer arrives there is no limit to trim to and TDLib's own update does it.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::{
    MessageInteractionInfo, MessageReaction, MessageReactions, ReactionType,
};
use serde_json::Value;

/// The per-user reaction limits from the app config.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReactionLimits {
    /// `reactions_user_max_default`.
    pub default: Option<usize>,
    /// `reactions_user_max_premium`.
    pub premium: Option<usize>,
}

impl ReactionLimits {
    /// Both keys of a `jsonValueObject` app config; a missing key stays
    /// unknown.
    pub fn from_app_config(config: &Value) -> Self {
        let read = |key: &str| {
            config
                .get("members")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|member| member.get("key").and_then(Value::as_str) == Some(key))
                .and_then(|member| member.get("value")?.get("value")?.as_f64())
                .filter(|count| count.is_finite() && *count >= 1.0)
                .map(|count| count.min(1000.0) as usize)
        };
        Self {
            default: read("reactions_user_max_default"),
            premium: read("reactions_user_max_premium"),
        }
    }

    pub fn for_account(self, premium: bool) -> Option<usize> {
        if premium { self.premium } else { self.default }
    }
}

/// The message's reactions with the user's `reaction` added (`add`) or
/// taken back. Adding past `limit` first takes back the own reactions after
/// the first `limit - 1`, in list order, as tdesktop's
/// `MessageReactions::add` does. Stars are never touched.
pub fn with_own_reaction(
    current: Option<&MessageInteractionInfo>,
    reaction: &ReactionType,
    add: bool,
    limit: Option<usize>,
) -> MessageInteractionInfo {
    let mut info = current.cloned().unwrap_or_default();
    let mut reactions: MessageReactions = info.reactions.take().unwrap_or_default();
    let chosen = reactions
        .reactions
        .iter()
        .any(|one| one.is_chosen && one.reaction_type == *reaction);
    if chosen == add {
        info.reactions = Some(reactions);
        return info;
    }
    if chosen {
        if let Some(one) = reactions
            .reactions
            .iter_mut()
            .find(|one| one.reaction_type == *reaction)
        {
            one.is_chosen = false;
            one.total_count = (one.total_count - 1).max(0);
        }
    } else {
        if let Some(limit) = limit {
            let keep = limit.saturating_sub(1);
            let mut mine = 0;
            for one in reactions
                .reactions
                .iter_mut()
                .filter(|one| one.is_chosen && one.reaction_type != ReactionType::Paid)
            {
                mine += 1;
                if mine > keep {
                    one.is_chosen = false;
                    one.total_count = (one.total_count - 1).max(0);
                }
            }
        }
        match reactions
            .reactions
            .iter_mut()
            .find(|one| one.reaction_type == *reaction)
        {
            Some(one) => {
                one.is_chosen = true;
                one.total_count += 1;
            }
            None => reactions.reactions.push(MessageReaction {
                reaction_type: reaction.clone(),
                total_count: 1,
                is_chosen: true,
                recent_senders: Vec::new(),
            }),
        }
    }
    reactions.reactions.retain(|one| one.total_count > 0);
    info.reactions = (!reactions.reactions.is_empty()).then_some(reactions);
    info
}

#[cfg(test)]
mod tests {
    use super::{ReactionLimits, with_own_reaction};
    use crate::telegram::envelope::{
        MessageInteractionInfo, MessageReaction, MessageReactions, ReactionType,
    };

    fn reaction(emoji: &str, count: i32, chosen: bool) -> MessageReaction {
        MessageReaction {
            reaction_type: ReactionType::emoji(emoji),
            total_count: count,
            is_chosen: chosen,
            recent_senders: Vec::new(),
        }
    }

    fn info(list: Vec<MessageReaction>) -> MessageInteractionInfo {
        MessageInteractionInfo {
            reactions: Some(MessageReactions {
                reactions: list,
                ..MessageReactions::default()
            }),
            ..MessageInteractionInfo::default()
        }
    }

    fn state(info: &MessageInteractionInfo) -> Vec<(String, i32, bool)> {
        info.reactions
            .iter()
            .flat_map(|r| &r.reactions)
            .map(|one| {
                let name = match &one.reaction_type {
                    ReactionType::Emoji { emoji } => emoji.clone(),
                    ReactionType::CustomEmoji { custom_emoji_id } => custom_emoji_id.to_string(),
                    ReactionType::Paid => "paid".into(),
                    ReactionType::Unknown => "?".into(),
                };
                (name, one.total_count, one.is_chosen)
            })
            .collect()
    }

    #[test]
    fn reads_the_limits_from_the_app_config() {
        let config: serde_json::Value = serde_json::from_str(
            r#"{"@type":"jsonValueObject","members":[
              {"key":"reactions_user_max_default","value":{"@type":"jsonValueNumber","value":1}},
              {"key":"reactions_user_max_premium","value":{"@type":"jsonValueNumber","value":3}}]}"#,
        )
        .expect("json");
        let limits = ReactionLimits::from_app_config(&config);
        assert_eq!(limits.for_account(false), Some(1));
        assert_eq!(limits.for_account(true), Some(3));
        let empty: serde_json::Value =
            serde_json::from_str(r#"{"@type":"jsonValueObject","members":[]}"#).expect("json");
        assert_eq!(
            ReactionLimits::from_app_config(&empty),
            ReactionLimits::default()
        );
    }

    #[test]
    fn a_free_account_swaps_its_one_reaction() {
        let before = info(vec![reaction("👍", 4, true), reaction("❤", 1, false)]);
        let after = with_own_reaction(Some(&before), &ReactionType::emoji("❤"), true, Some(1));
        assert_eq!(
            state(&after),
            vec![("👍".into(), 3, false), ("❤".into(), 2, true)]
        );
    }

    #[test]
    fn premium_keeps_the_first_two_and_adds_the_third() {
        let before = info(vec![
            reaction("👍", 1, true),
            reaction("❤", 2, true),
            reaction("🔥", 1, true),
        ]);
        let after = with_own_reaction(Some(&before), &ReactionType::emoji("🎉"), true, Some(3));
        assert_eq!(
            state(&after),
            vec![
                ("👍".into(), 1, true),
                ("❤".into(), 2, true),
                ("🎉".into(), 1, true),
            ],
            "the own reaction past the limit is gone with its count"
        );
    }

    #[test]
    fn stars_do_not_count_toward_the_limit() {
        let mut stars = reaction("", 5, true);
        stars.reaction_type = ReactionType::Paid;
        let before = info(vec![stars, reaction("👍", 1, false)]);
        let after = with_own_reaction(Some(&before), &ReactionType::emoji("👍"), true, Some(1));
        assert_eq!(
            state(&after),
            vec![("paid".into(), 5, true), ("👍".into(), 2, true)]
        );
    }

    #[test]
    fn adding_an_own_reaction_again_changes_nothing() {
        let before = info(vec![reaction("👍", 2, true), reaction("❤", 1, true)]);
        let after = with_own_reaction(Some(&before), &ReactionType::emoji("❤"), true, Some(1));
        assert_eq!(after, before);
        let after = with_own_reaction(Some(&before), &ReactionType::emoji("🔥"), false, Some(1));
        assert_eq!(after, before);
    }

    #[test]
    fn taking_one_back_and_an_unknown_limit_trim_nothing() {
        let before = info(vec![reaction("👍", 1, true), reaction("❤", 3, true)]);
        let after = with_own_reaction(Some(&before), &ReactionType::emoji("❤"), false, Some(1));
        assert_eq!(
            state(&after),
            vec![("👍".into(), 1, true), ("❤".into(), 2, false)]
        );
        let after = with_own_reaction(Some(&before), &ReactionType::emoji("🔥"), true, None);
        assert_eq!(state(&after).len(), 3);
        let custom = ReactionType::CustomEmoji {
            custom_emoji_id: 77,
        };
        let added = with_own_reaction(None, &custom, true, Some(1));
        assert_eq!(state(&added), vec![("77".into(), 1, true)]);
        assert_eq!(
            with_own_reaction(Some(&added), &custom, false, Some(1)).reactions,
            None
        );
    }
}

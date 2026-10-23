use super::*;
use crate::telegram::envelope::ParsedBoostLevelFeatures;

fn some(reactions: Vec<ReactionType>) -> ChatAvailableReactions {
    ChatAvailableReactions::Some {
        reactions,
        max_reaction_count: 11,
    }
}

fn table(levels: &[(i32, i32, bool)]) -> ParsedBoostFeatures {
    ParsedBoostFeatures {
        levels: levels
            .iter()
            .map(|(level, custom, translate)| ParsedBoostLevelFeatures {
                level: *level,
                custom_emoji_reaction_count: *custom,
                can_enable_automatic_translation: *translate,
                ..ParsedBoostLevelFeatures::default()
            })
            .collect(),
    }
}

#[test]
fn reactions_value_counts_emoji_and_a_lone_paid_reaction() {
    assert_eq!(
        ReactionsValue::of(&ChatAvailableReactions::All {
            max_reaction_count: 11
        }),
        ReactionsValue::All
    );
    assert_eq!(ReactionsValue::of(&some(vec![])), ReactionsValue::Off);
    assert_eq!(
        ReactionsValue::of(&some(vec![ReactionType::Paid])),
        ReactionsValue::Count(1)
    );
    assert_eq!(
        ReactionsValue::of(&some(vec![
            ReactionType::emoji("👍"),
            ReactionType::Paid,
            ReactionType::emoji("🔥"),
        ])),
        ReactionsValue::Count(2)
    );
}

#[test]
fn counts_show_only_above_zero() {
    assert_eq!(positive_count(0), None);
    assert_eq!(positive_count(-3), None);
    assert_eq!(positive_count(4).as_deref(), Some("4"));
}

#[test]
fn auto_translate_needs_the_first_level_that_unlocks_it() {
    let features = table(&[(1, 1, false), (3, 3, true), (5, 5, true)]);
    assert_eq!(auto_translate_level(Some(&features)), Some(3));
    assert_eq!(auto_translate_level(None), None);
    assert_eq!(auto_translate_level(Some(&table(&[(1, 1, false)]))), None);
}

#[test]
fn auto_translate_is_locked_below_the_level_or_while_unknown() {
    let at = |level, required| AutoTranslate {
        on: false,
        level,
        required,
    };
    assert!(at(Some(2), Some(3)).locked());
    assert!(!at(Some(3), Some(3)).locked());
    assert!(at(None, Some(3)).locked());
    assert!(at(Some(9), None).locked());
}

#[test]
fn a_locked_switch_asks_for_boosts_but_still_turns_off() {
    let locked = AutoTranslate {
        on: false,
        level: Some(1),
        required: Some(3),
    };
    assert_eq!(locked.click(), AutoTranslateClick::AskBoost(3));
    let on = AutoTranslate { on: true, ..locked };
    assert_eq!(on.click(), AutoTranslateClick::Toggle(false));
    let unknown = AutoTranslate {
        level: None,
        ..locked
    };
    assert_eq!(unknown.click(), AutoTranslateClick::Check);
    let open = AutoTranslate {
        level: Some(4),
        ..locked
    };
    assert_eq!(open.click(), AutoTranslateClick::Toggle(true));
}

#[test]
fn profiles_follow_signing() {
    let off = Signatures::new(false, true);
    assert_eq!(off, Signatures::default());
    assert!(!off.shows_profiles());
    assert_eq!(off.about(), keys::EDIT_SIGN_MESSAGES_ABOUT);
    let sign = off.toggle_sign();
    assert!(sign.shows_profiles());
    assert!(!sign.profiles);
    assert_eq!(sign.about(), keys::EDIT_SIGN_PROFILES_ABOUT);
    let profiles = sign.toggle_profiles();
    assert!(profiles.profiles);
    assert_eq!(profiles.toggle_sign(), Signatures::default());
    assert_eq!(off.toggle_profiles(), off);
}

#[test]
fn history_texts() {
    assert_eq!(history_value(true), keys::MANAGE_HISTORY_VISIBILITY_SHOWN);
    assert_eq!(history_value(false), keys::MANAGE_HISTORY_VISIBILITY_HIDDEN);
    assert_eq!(
        history_hidden_about(true),
        keys::MANAGE_HISTORY_VISIBILITY_HIDDEN_LEGACY
    );
    assert_eq!(
        history_hidden_about(false),
        keys::MANAGE_HISTORY_VISIBILITY_HIDDEN_ABOUT
    );
}

#[test]
fn custom_reactions_over_the_level_show_the_level_text() {
    let features = table(&[(1, 1, false), (2, 2, false), (5, 5, false)]);
    assert_eq!(custom_reactions_allowed(Some(&features), 2), Some(2));
    assert_eq!(custom_reactions_allowed(Some(&features), 0), Some(0));
    assert_eq!(custom_reactions_allowed(None, 2), None);
    let three = some(vec![
        ReactionType::CustomEmoji { custom_emoji_id: 1 },
        ReactionType::CustomEmoji { custom_emoji_id: 2 },
        ReactionType::CustomEmoji { custom_emoji_id: 3 },
        ReactionType::emoji("👍"),
    ]);
    assert_eq!(custom_reactions_over_level(Some(&three), Some(2)), Some(3));
    assert_eq!(custom_reactions_over_level(Some(&three), Some(3)), None);
    assert_eq!(custom_reactions_over_level(Some(&three), None), None);
    assert_eq!(custom_reactions_over_level(None, Some(0)), None);
}

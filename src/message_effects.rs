//! Message effects (tdesktop `Data::Reactions::Type::Effects`, the effect
//! badge in `HistoryView::BottomInfo` and the send menu's effect picker).
//!
//! A message sent with an effect in a private chat carries `effect_id`.
//! TDLib has no list request: `updateAvailableMessageEffects` names the ids
//! of the effects the account can use (reaction effects and Premium sticker
//! effects, each in the server's order) and `getMessageEffect` describes one.
//! An effect is either an emoji reaction effect (a small select animation
//! and a TGS drawn over the chat) or a Premium sticker effect (the
//! sticker's own Premium animation).
#![deny(clippy::indexing_slicing)]

use crate::ids::{FileId, RequestId};
use crate::telegram::envelope::{ParsedFile, StickerFormat, StickerItem, parse_sticker_value};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// How an effect looks and what it plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectKind {
    /// `messageEffectTypeEmojiReaction`.
    Emoji {
        select_animation: Option<StickerItem>,
        effect_animation: Option<StickerItem>,
    },
    /// `messageEffectTypePremiumSticker`: the effect is the sticker's
    /// Premium animation.
    Sticker { sticker: Option<StickerItem> },
}

/// `messageEffect id static_icon emoji is_premium type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEffect {
    pub id: i64,
    /// `static_icon` (a WEBP sticker); `None` when there is none.
    pub static_icon: Option<StickerItem>,
    /// The emoji standing for the effect when the icon is not there.
    pub emoji: String,
    /// Telegram Premium is needed to send with the effect.
    pub is_premium: bool,
    pub kind: EffectKind,
}

/// What plays: the file and its format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectAnimation {
    pub file: FileId,
    pub format: StickerFormat,
}

impl MessageEffect {
    /// The animation drawn over the chat: a reaction effect's
    /// `effect_animation`, a sticker effect's Premium animation
    /// (tdesktop `aroundAnimation`, else the sticker's video thumbnail).
    pub fn animation(&self) -> Option<EffectAnimation> {
        match &self.kind {
            EffectKind::Emoji {
                effect_animation, ..
            } => effect_animation
                .as_ref()
                .filter(|sticker| sticker.file_id.0 != 0)
                .map(|sticker| EffectAnimation {
                    file: sticker.file_id,
                    format: sticker.format,
                }),
            EffectKind::Sticker { sticker } => sticker
                .as_ref()
                .and_then(|sticker| sticker.premium_animation)
                .filter(|file| file.0 != 0)
                .map(|file| EffectAnimation {
                    file,
                    format: StickerFormat::Tgs,
                }),
        }
    }

    /// The picker lists reaction effects first and sticker effects after
    /// them (tdesktop `LookupPossibleEffects`: `recent` is the ones with
    /// an around animation).
    pub fn is_sticker_effect(&self) -> bool {
        matches!(self.kind, EffectKind::Sticker { .. })
    }

    /// The small picture standing for the effect: the static icon's file.
    pub fn icon_file(&self) -> Option<FileId> {
        self.static_icon
            .as_ref()
            .and_then(StickerItem::display_file_id)
    }

    /// Files the picker and the message badge need before they can draw.
    pub fn display_files(&self) -> Vec<FileId> {
        self.icon_file().into_iter().collect()
    }
}

/// `getMessageEffect effect_id:int64 = MessageEffect`.
pub fn get_message_effect(extra: RequestId, effect_id: i64) -> String {
    json!({
        "@type": "getMessageEffect",
        "@extra": extra.as_extra(),
        "effect_id": effect_id.to_string(),
    })
    .to_string()
}

fn sticker_of(value: Option<&Value>, files: &mut Vec<ParsedFile>) -> Option<StickerItem> {
    let (item, mut found) = parse_sticker_value(value.filter(|value| !value.is_null()));
    files.append(&mut found);
    item
}

/// Parse a `messageEffect` and the files of its stickers.
pub fn parse_message_effect(value: &Value) -> Option<(MessageEffect, Vec<ParsedFile>)> {
    if value.get("@type").and_then(Value::as_str) != Some("messageEffect") {
        return None;
    }
    let id = match value.get("id")? {
        Value::String(text) => text.parse::<i64>().ok()?,
        Value::Number(number) => number.as_i64()?,
        _ => return None,
    };
    if id == 0 {
        return None;
    }
    let mut files = Vec::new();
    let static_icon = sticker_of(value.get("static_icon"), &mut files);
    let kind_value = value.get("type");
    let kind = match kind_value
        .and_then(|kind| kind.get("@type"))
        .and_then(Value::as_str)
    {
        Some("messageEffectTypeEmojiReaction") => {
            let kind_value = kind_value?;
            EffectKind::Emoji {
                select_animation: sticker_of(kind_value.get("select_animation"), &mut files),
                effect_animation: sticker_of(kind_value.get("effect_animation"), &mut files),
            }
        }
        Some("messageEffectTypePremiumSticker") => EffectKind::Sticker {
            sticker: sticker_of(kind_value?.get("sticker"), &mut files),
        },
        _ => return None,
    };
    Some((
        MessageEffect {
            id,
            static_icon,
            emoji: value
                .get("emoji")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            is_premium: value
                .get("is_premium")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            kind,
        },
        files,
    ))
}

/// The ids of `updateAvailableMessageEffects`' two lists.
pub fn parse_available_effect_ids(value: &Value) -> (Vec<i64>, Vec<i64>) {
    let ids = |key: &str| -> Vec<i64> {
        value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|id| match id {
                Value::String(text) => text.parse().ok(),
                Value::Number(number) => number.as_i64(),
                _ => None,
            })
            .filter(|id| *id != 0)
            .collect()
    };
    (ids("reaction_effect_ids"), ids("sticker_effect_ids"))
}

/// The effects the session knows.
#[derive(Debug, Default)]
pub struct MessageEffects {
    by_id: HashMap<i64, MessageEffect>,
    /// `updateAvailableMessageEffects`, in the server's order.
    reaction_ids: Vec<i64>,
    sticker_ids: Vec<i64>,
    /// Asked for and not answered yet.
    requested: HashSet<i64>,
    /// `getMessageEffect` failed (404 for an effect that is gone).
    failed: HashSet<i64>,
    /// Bumped when an effect arrives, so a view can tell it changed.
    revision: u64,
    /// Which messages have not played their effect here yet.
    pub watch: crate::message_effect_watch::EffectWatch,
}

impl MessageEffects {
    pub fn set_available(&mut self, reaction_ids: Vec<i64>, sticker_ids: Vec<i64>) {
        self.reaction_ids = reaction_ids;
        self.sticker_ids = sticker_ids;
        self.revision += 1;
    }

    pub fn accept(&mut self, effect: MessageEffect) {
        self.requested.remove(&effect.id);
        self.failed.remove(&effect.id);
        self.by_id.insert(effect.id, effect);
        self.revision += 1;
    }

    pub fn fail(&mut self, id: i64) {
        self.requested.remove(&id);
        self.failed.insert(id);
        self.revision += 1;
    }

    pub fn get(&self, id: i64) -> Option<&MessageEffect> {
        self.by_id.get(&id)
    }

    /// `getMessageEffect` failed for `id`: there is nothing to wait for.
    pub fn is_gone(&self, id: i64) -> bool {
        self.failed.contains(&id)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The server's ids for the picker: reaction effects, then sticker
    /// effects.
    pub fn available_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.reaction_ids
            .iter()
            .chain(self.sticker_ids.iter())
            .copied()
    }

    /// Whether the server has told which effects exist.
    pub fn available_known(&self) -> bool {
        !(self.reaction_ids.is_empty() && self.sticker_ids.is_empty())
    }

    /// Of `ids`, those worth a `getMessageEffect` now.
    pub fn wanted(&self, ids: impl IntoIterator<Item = i64>) -> Vec<i64> {
        let mut seen = HashSet::new();
        ids.into_iter()
            .filter(|id| *id != 0)
            .filter(|id| {
                !self.by_id.contains_key(id)
                    && !self.requested.contains(id)
                    && !self.failed.contains(id)
                    && seen.insert(*id)
            })
            .collect()
    }

    pub fn mark_requested(&mut self, id: i64) {
        self.requested.insert(id);
    }

    /// The effects the picker lists, as tdesktop's `LookupPossibleEffects`:
    /// loaded ones only, each once, the Premium ones left out where Premium
    /// cannot be had. Reaction effects first, then sticker effects.
    pub fn for_picker(&self, premium_possible: bool) -> (Vec<&MessageEffect>, Vec<&MessageEffect>) {
        let mut added = HashSet::new();
        let mut reactions = Vec::new();
        let mut stickers = Vec::new();
        for id in self.available_ids() {
            let Some(effect) = self.by_id.get(&id) else {
                continue;
            };
            if (!premium_possible && effect.is_premium) || !added.insert(id) {
                continue;
            }
            if effect.is_sticker_effect() {
                stickers.push(effect);
            } else {
                reactions.push(effect);
            }
        }
        (reactions, stickers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sticker(file: i32, premium: Option<i32>) -> Value {
        json!({
            "@type": "sticker", "id": "1", "set_id": "2", "width": 512, "height": 512,
            "emoji": "x", "format": {"@type": "stickerFormatTgs"},
            "full_type": {"@type": "stickerFullTypeRegular",
                "premium_animation": premium.map(|id| json!({"@type": "file", "id": id, "size": 3}))},
            "thumbnail": null,
            "sticker": {"@type": "file", "id": file, "size": 5}
        })
    }

    fn reaction_effect(id: &str, premium: bool) -> Value {
        json!({
            "@type": "messageEffect", "id": id, "emoji": "\u{1F389}", "is_premium": premium,
            "static_icon": {"@type": "sticker", "id": "9", "set_id": "0", "width": 100,
                "height": 100, "emoji": "x", "format": {"@type": "stickerFormatWebp"},
                "full_type": {"@type": "stickerFullTypeRegular", "premium_animation": null},
                "thumbnail": null, "sticker": {"@type": "file", "id": 31, "size": 2}},
            "type": {"@type": "messageEffectTypeEmojiReaction",
                "select_animation": sticker(32, None),
                "effect_animation": sticker(33, None)}
        })
    }

    fn sticker_effect(id: &str) -> Value {
        json!({
            "@type": "messageEffect", "id": id, "emoji": "x", "is_premium": true,
            "static_icon": null,
            "type": {"@type": "messageEffectTypePremiumSticker", "sticker": sticker(41, Some(42))}
        })
    }

    #[test]
    fn request_json_sends_the_id_as_a_string() {
        let v: Value =
            serde_json::from_str(&get_message_effect(RequestId(5), 5104841245755180586)).unwrap();
        assert_eq!(v["@type"], "getMessageEffect");
        assert_eq!(v["effect_id"], "5104841245755180586");
    }

    #[test]
    fn a_reaction_effect_plays_its_effect_animation() {
        let (effect, files) = parse_message_effect(&reaction_effect("77", false)).unwrap();
        assert_eq!(effect.id, 77);
        assert!(!effect.is_premium);
        assert_eq!(effect.icon_file(), Some(FileId(31)));
        assert_eq!(
            effect.animation(),
            Some(EffectAnimation {
                file: FileId(33),
                format: StickerFormat::Tgs
            })
        );
        assert!(files.iter().any(|f| f.id == FileId(31)));
        assert!(files.iter().any(|f| f.id == FileId(33)));
    }

    #[test]
    fn a_sticker_effect_plays_the_stickers_premium_animation() {
        let (effect, _) = parse_message_effect(&sticker_effect("78")).unwrap();
        assert!(effect.is_sticker_effect());
        assert!(effect.is_premium);
        assert_eq!(effect.animation().map(|a| a.file), Some(FileId(42)));
        assert_eq!(effect.icon_file(), None);
    }

    #[test]
    fn effects_without_a_known_type_or_id_are_dropped() {
        assert!(
            parse_message_effect(&json!({"@type": "messageEffect", "id": "0",
            "type": {"@type": "messageEffectTypePremiumSticker"}}))
            .is_none()
        );
        assert!(
            parse_message_effect(&json!({"@type": "messageEffect", "id": "5",
            "type": {"@type": "messageEffectTypeNew"}}))
            .is_none()
        );
        assert!(parse_message_effect(&json!({"@type": "ok"})).is_none());
    }

    #[test]
    fn available_ids_parse_both_lists() {
        let (reactions, stickers) = parse_available_effect_ids(&json!({
            "@type": "updateAvailableMessageEffects",
            "reaction_effect_ids": ["1", "2", 0],
            "sticker_effect_ids": ["3"]
        }));
        assert_eq!(reactions, vec![1, 2]);
        assert_eq!(stickers, vec![3]);
    }

    #[test]
    fn only_unknown_effects_are_asked_for_once() {
        let mut effects = MessageEffects::default();
        effects.set_available(vec![1, 2], vec![3]);
        assert_eq!(
            effects.wanted(effects.available_ids().collect::<Vec<_>>()),
            vec![1, 2, 3]
        );
        effects.mark_requested(1);
        let (loaded, _) = parse_message_effect(&reaction_effect("2", false)).unwrap();
        effects.accept(loaded);
        effects.fail(3);
        assert!(effects.wanted([1, 2, 3, 0]).is_empty());
        assert_eq!(effects.wanted([4, 4, 5]), vec![4, 5]);
    }

    #[test]
    fn the_picker_lists_reaction_effects_before_sticker_effects() {
        let mut effects = MessageEffects::default();
        effects.set_available(vec![10, 11, 11], vec![20, 99]);
        for value in [
            reaction_effect("10", false),
            reaction_effect("11", true),
            sticker_effect("20"),
        ] {
            effects.accept(parse_message_effect(&value).unwrap().0);
        }
        let (reactions, stickers) = effects.for_picker(true);
        assert_eq!(
            reactions.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![10, 11]
        );
        assert_eq!(stickers.iter().map(|e| e.id).collect::<Vec<_>>(), vec![20]);
        // Where Premium cannot be had, its effects are not offered.
        let (reactions, stickers) = effects.for_picker(false);
        assert_eq!(reactions.iter().map(|e| e.id).collect::<Vec<_>>(), vec![10]);
        assert!(stickers.is_empty());
    }
}

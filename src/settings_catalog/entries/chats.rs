//! Chats page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page, Platforms};

const P: Page = Page::Chats;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("chats.send_key", P, keys::Q_PREFS_SEND_MESSAGES_WITH).keywords(&[
        "enter",
        "ctrl+enter",
        "cmd+enter",
        "newline",
    ]),
    Entry::new("chats.double_click", P, keys::Q_SETTINGS_CHATS_DOUBLE_CLICK).keywords(&[
        "reply",
        "react",
        "quick action",
    ]),
    Entry::new(
        "chats.quick_reaction",
        P,
        keys::SETTINGS_CHAT_REACTIONS_TITLE,
    )
    .keywords(&["default reaction", "emoji"]),
    Entry::new("chats.reply_button", P, keys::SETTINGS_CHAT_CORNER_REPLY).keywords(&["hover"]),
    Entry::new(
        "chats.reaction_button",
        P,
        keys::SETTINGS_CHAT_CORNER_REACTION,
    )
    .keywords(&["hover"]),
    Entry::new(
        "chats.swipe_next_channel",
        P,
        keys::SETTINGS_PULL_TO_NEXT_CHANNEL,
    )
    .keywords(&["pull", "next channel", "scroll"]),
    Entry::new(
        "chats.chat_list_action",
        P,
        keys::SETTINGS_QUICK_DIALOG_ACTION_TITLE,
    )
    .keywords(&["swipe", "middle click", "archive", "pin"]),
    Entry::new("chats.autoplay_gifs", P, keys::Q_PREFS_AUTOPLAY_GIFS)
        .section(keys::MEDIA_TYPE_MEDIA)
        .keywords(&["animations"]),
    Entry::new("chats.autoplay_videos", P, keys::Q_PREFS_AUTOPLAY_VIDEOS)
        .section(keys::MEDIA_TYPE_MEDIA),
    Entry::new("chats.loop_stickers", P, keys::SETTINGS_LOOP_STICKERS)
        .section(keys::MEDIA_TYPE_MEDIA)
        .keywords(&["repeat", "animated"]),
    Entry::new(
        "chats.hq_round_videos",
        P,
        keys::Q_PREFS_RECORD_HQ_ROUND_VIDEOS,
    )
    .section(keys::MEDIA_TYPE_MEDIA)
    .keywords(&["video notes", "quality", "circles"]),
    Entry::new("chats.instant_view", P, keys::Q_PREFS_INSTANT_VIEW)
        .section(keys::MEDIA_TYPE_MEDIA)
        .keywords(&["articles", "links", "reader"]),
    Entry::new("chats.large_emoji", P, keys::Q_PREFS_LARGE_EMOJI)
        .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
        .keywords(&["big emoji"]),
    Entry::new(
        "chats.suggest_emoji",
        P,
        keys::Q_PREFS_SUGGEST_EMOJI_REPLACEMENTS,
    )
    .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
    .keywords(&["autocomplete", "shortcodes"]),
    Entry::new(
        "chats.replace_emoji",
        P,
        keys::Q_PREFS_REPLACE_EMOJI_AUTOMATICALLY,
    )
    .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
    .keywords(&["smileys", "emoticons"]),
    Entry::new("chats.emoji_set", P, keys::EMOJI_MANAGE_SETS)
        .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
        .keywords(&["emoji style", "apple", "noto", "twemoji"]),
    Entry::new("chats.emoji_packs", P, keys::Q_PREFS_EMOJI_SETS)
        .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
        .keywords(&["custom emoji", "packs"]),
    Entry::new(
        "chats.dynamic_pack_order",
        P,
        keys::Q_MEDIA_DYNAMIC_EMOJI_PACK_ORDER,
    )
    .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
    .keywords(&["recent first"]),
    Entry::new(
        "chats.sticker_suggestions",
        P,
        keys::Q_MEDIA_SUGGEST_STICKERS_BY_EMOJI,
    )
    .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
    .keywords(&["sticker mode"]),
    Entry::new("chats.archived_stickers", P, keys::STICKERS_YOU_HAVE)
        .section(keys::Q_SETTINGS_CHATS_SEC_EMOJI)
        .keywords(&["sticker packs", "archived", "trending", "reorder"]),
    Entry::new("chats.spellcheck", P, keys::Q_PREFS_CHECK_SPELLING)
        .section(keys::Q_PREFS_SPELLING)
        .keywords(&["typos", "autocorrect", "grammar"]),
    Entry::new(
        "chats.system_text_replace",
        P,
        keys::SETTINGS_SYSTEM_TEXT_REPLACE,
    )
    .section(keys::Q_PREFS_SPELLING)
    .keywords(&["text replacements", "shortcuts", "substitutions", "replace"])
    .only(Platforms::MACOS),
    Entry::new(
        "chats.spellcheck_languages",
        P,
        keys::Q_SETTINGS_CHATS_SPELLCHECK_LANGUAGES,
    )
    .section(keys::Q_PREFS_SPELLING)
    .keywords(&["dictionary language"]),
    Entry::new("chats.dictionaries", P, keys::SETTINGS_MANAGE_DICTIONARIES)
        .section(keys::Q_PREFS_SPELLING)
        .keywords(&["hunspell", "download"]),
];

//! Offline plain-emoji keyboard data (Unicode 17.0, Unicode data license).
use crate::settings::MediaPrefs;

pub const CATEGORIES: &[&str] = &[
    "Smileys & Emotion",
    "People & Body",
    "Animals & Nature",
    "Food & Drink",
    "Travel & Places",
    "Activities",
    "Objects",
    "Symbols",
    "Flags",
];
pub const RECENT_LIMIT: usize = 48;

#[derive(Clone, Copy)]
pub struct EmojiEntry {
    pub category: &'static str,
    pub emoji: &'static str,
    pub name: &'static str,
}

pub fn catalog() -> impl Iterator<Item = EmojiEntry> {
    include_str!("../assets/emoji/emoji-17.tsv")
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            Some(EmojiEntry {
                category: fields.next()?,
                emoji: fields.next()?,
                name: fields.next()?,
            })
        })
}

pub fn search<'a>(
    category: Option<&'a str>,
    query: &'a str,
) -> impl Iterator<Item = EmojiEntry> + 'a {
    let query = query.trim().to_lowercase();
    catalog().filter(move |entry| {
        category.is_none_or(|category| entry.category == category)
            && (query.is_empty()
                || entry.emoji == query
                || query.split_whitespace().all(|word| {
                    entry.name.to_lowercase().contains(word)
                        || entry.category.to_lowercase().contains(word)
                }))
    })
}

/// One to three plain emoji, counting flags, skin tones and ZWJ sequences as units.
/// [`big_emoji_count`] for a message with entities: each custom-emoji span
/// counts as one emoji, and any other entity rules big emoji out.
pub fn big_emoji_count_with_entities(
    text: &str,
    entities: &[crate::text::TextEntity],
) -> Option<usize> {
    use crate::text::TextEntityKind;
    let mut rest = String::with_capacity(text.len());
    let mut custom = 0;
    let mut at = 0;
    let mut spans: Vec<_> = entities.iter().collect();
    spans.sort_by_key(|e| e.utf8_start);
    for entity in spans {
        if !matches!(entity.kind, TextEntityKind::CustomEmoji { .. })
            || entity.utf8_start < at
            || entity.utf8_end > text.len()
        {
            return None;
        }
        rest.push_str(text.get(at..entity.utf8_start)?);
        custom += 1;
        at = entity.utf8_end;
    }
    rest.push_str(text.get(at..)?);
    let plain = if rest.trim().is_empty() {
        0
    } else {
        big_emoji_count(&rest)?
    };
    let count = custom + plain;
    (count > 0 && count <= 3).then_some(count)
}

pub fn big_emoji_count(text: &str) -> Option<usize> {
    use std::collections::HashSet;
    use std::sync::OnceLock;
    use unicode_segmentation::UnicodeSegmentation;
    static KNOWN: OnceLock<HashSet<String>> = OnceLock::new();
    let known = KNOWN.get_or_init(|| {
        catalog()
            .map(|entry| entry.emoji.replace('\u{fe0f}', ""))
            .collect()
    });
    let mut count = 0;
    for grapheme in text.graphemes(true) {
        if grapheme.chars().all(char::is_whitespace) {
            continue;
        }
        if !known.contains(&grapheme.replace('\u{fe0f}', "")) {
            return None;
        }
        count += 1;
        if count > 3 {
            return None;
        }
    }
    (count > 0).then_some(count)
}

impl MediaPrefs {
    pub fn remember_emoji(&mut self, emoji: &str) {
        if !catalog().any(|entry| entry.emoji == emoji) {
            return;
        }
        self.recent_emoji.retain(|item| item != emoji);
        self.recent_emoji.insert(0, emoji.to_owned());
        self.recent_emoji.truncate(RECENT_LIMIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_emoji_count_as_big_emoji() {
        use crate::text::{TextEntity, TextEntityKind};
        let custom = |start: usize, end: usize| TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind: TextEntityKind::CustomEmoji { custom_emoji_id: 1 },
        };
        // "🔠🔠" (4 bytes each), both custom.
        assert_eq!(
            big_emoji_count_with_entities("🔠🔠", &[custom(0, 4), custom(4, 8)]),
            Some(2)
        );
        // A custom emoji plus a plain emoji.
        assert_eq!(
            big_emoji_count_with_entities("🔠 😀", &[custom(0, 4)]),
            Some(2)
        );
        // Text next to it, or a non-emoji entity, is not big.
        assert_eq!(
            big_emoji_count_with_entities("🔠 hi", &[custom(0, 4)]),
            None
        );
        let bold = TextEntity {
            utf8_start: 0,
            utf8_end: 4,
            kind: TextEntityKind::Bold,
        };
        assert_eq!(big_emoji_count_with_entities("😀", &[bold]), None);
        assert_eq!(big_emoji_count_with_entities("😀", &[]), Some(1));
    }

    #[test]
    fn big_emoji_uses_graphemes_and_excludes_text() {
        assert_eq!(big_emoji_count(" 👩🏽‍💻 🇺🇸 ❤️ "), Some(3));
        assert_eq!(big_emoji_count("❤"), Some(1));
        assert_eq!(big_emoji_count("1️⃣"), Some(1));
        assert_eq!(big_emoji_count("hello 😀"), None);
        assert_eq!(big_emoji_count("123"), None);
        assert_eq!(big_emoji_count("😀😀😀😀"), None);
        assert_eq!(big_emoji_count(""), None);
        assert_eq!(big_emoji_count("  "), None);
        assert_eq!(big_emoji_count("❤\u{fe0e}"), None);
    }
    #[test]
    fn catalog_search_and_account_recents() {
        let entries: Vec<_> = catalog().collect();
        assert_eq!(entries.len(), 3944);
        let unique: std::collections::HashSet<_> =
            entries.iter().map(|entry| entry.emoji).collect();
        assert_eq!(unique.len(), entries.len());
        assert!(
            entries
                .iter()
                .all(|entry| CATEGORIES.contains(&entry.category))
        );
        assert!(search(Some("Flags"), "United States").any(|entry| entry.emoji == "🇺🇸"));
        assert!(search(None, "woman technologist").any(|entry| entry.emoji == "👩‍💻"));
        assert!(search(None, "👩🏽‍💻").any(|entry| entry.emoji == "👩🏽‍💻"));
        assert_eq!(search(Some("Food & Drink"), "technologist").count(), 0);
        let mut prefs = MediaPrefs::default();
        for entry in &entries[..60] {
            prefs.remember_emoji(entry.emoji);
        }
        assert_eq!(prefs.recent_emoji.len(), RECENT_LIMIT);
        prefs.remember_emoji(entries[59].emoji);
        assert_eq!(prefs.recent_emoji.len(), RECENT_LIMIT);
        assert_eq!(prefs.recent_emoji[0], entries[59].emoji);
        prefs.remember_emoji("not an emoji");
        assert_eq!(prefs.recent_emoji.len(), RECENT_LIMIT);
        prefs.big_emoji = false;
        let stored = serde_json::to_string(&prefs).unwrap();
        let restored: MediaPrefs = serde_json::from_str(&stored).unwrap();
        assert_eq!(prefs, restored);
        assert!(MediaPrefs::default().recent_emoji.is_empty());
        let mut old = serde_json::to_value(&prefs).unwrap();
        old.as_object_mut().unwrap().remove("recent_emoji");
        old.as_object_mut().unwrap().remove("big_emoji");
        assert!(
            serde_json::from_value::<MediaPrefs>(old.clone())
                .unwrap()
                .big_emoji
        );
        assert!(
            serde_json::from_value::<MediaPrefs>(old)
                .unwrap()
                .recent_emoji
                .is_empty()
        );
    }
}

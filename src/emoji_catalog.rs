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
        let stored = serde_json::to_string(&prefs).unwrap();
        let restored: MediaPrefs = serde_json::from_str(&stored).unwrap();
        assert_eq!(prefs, restored);
        assert!(MediaPrefs::default().recent_emoji.is_empty());
        let mut old = serde_json::to_value(&prefs).unwrap();
        old.as_object_mut().unwrap().remove("recent_emoji");
        assert!(
            serde_json::from_value::<MediaPrefs>(old)
                .unwrap()
                .recent_emoji
                .is_empty()
        );
    }
}

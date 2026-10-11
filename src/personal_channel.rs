//! The personal channel card on a profile (`userFullInfo.personal_chat_id`,
//! TDLib 1.8.68). Follows tdesktop's `makePersonalChannel`
//! (`info_profile_actions.cpp`): the label reads "Personal channel" and,
//! when the channel has more than one subscriber, appends the count; the
//! value is the channel name with the latest post under it. TDLib gives no
//! post id for the card, so the channel's own last message stands in. See
//! `docs/decisions/codex-personal-channel.md`.

use crate::premium_hub::group_digits;

/// The text shown for a personal channel on a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalChannelCard {
    pub label: String,
    pub value: String,
}

/// "Personal channel", plus " • 1,234 subscribers" once the channel has
/// more than one (tdesktop hides the count for 0 and 1).
pub fn card_label(subscribers: i32) -> String {
    if subscribers > 1 {
        format!(
            "Personal channel \u{2022} {} subscribers",
            group_digits(i64::from(subscribers))
        )
    } else {
        "Personal channel".to_string()
    }
}

/// The channel name, with the latest post's one-line preview below it when
/// there is one.
pub fn card_value(title: &str, preview: &str) -> String {
    let preview = preview.trim();
    if preview.is_empty() {
        title.to_string()
    } else {
        format!("{title}\n{}", preview.replace(['\n', '\r'], " "))
    }
}

pub fn card(title: &str, subscribers: i32, preview: &str) -> PersonalChannelCard {
    PersonalChannelCard {
        label: card_label(subscribers),
        value: card_value(title, preview),
    }
}

#[cfg(test)]
mod tests {
    use crate::personal_channel::{card, card_label, card_value};

    #[test]
    fn label_shows_the_count_only_above_one() {
        assert_eq!(card_label(0), "Personal channel");
        assert_eq!(card_label(1), "Personal channel");
        assert_eq!(
            card_label(12_345),
            "Personal channel \u{2022} 12,345 subscribers"
        );
    }

    #[test]
    fn value_puts_the_latest_post_under_the_name() {
        assert_eq!(card_value("News", ""), "News");
        assert_eq!(card_value("News", "  \n"), "News");
        assert_eq!(card_value("News", "Hello\nworld"), "News\nHello world");
    }

    #[test]
    fn card_combines_both() {
        let c = card("News", 2, "Hi");
        assert_eq!(c.label, "Personal channel \u{2022} 2 subscribers");
        assert_eq!(c.value, "News\nHi");
    }
}

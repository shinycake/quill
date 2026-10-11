//! The "Is this still your number?" prompt (tdesktop `settings_main.cpp`,
//! `SetupValidatePhoneNumberSuggestion`). TDLib raises it with
//! `updateSuggestedActions(suggestedActionCheckPhoneNumber)`; "Yes" sends
//! `hideSuggestedAction`, "No" explains where to change the number (the
//! official app on the phone: TDLib offers no way to change it from here).

/// tdesktop `lng_settings_suggestion_phone_number_about_link`.
pub const LEARN_MORE_URL: &str =
    "https://telegram.org/faq#q-i-have-a-new-phone-number-what-do-i-do";

/// tdesktop `lng_settings_suggestion_phone_number_about`, without the link.
pub const ABOUT: &str = "Keep your number up to date to ensure you can always log into Telegram.";

/// tdesktop `lng_settings_suggestion_phone_number_change`.
pub const CHANGE_NOTE: &str = "Please change your phone number in the official Telegram app on your phone as soon as possible.";

/// `lng_settings_suggestion_phone_number_title` for a formatted number.
pub fn title(formatted_phone: &str) -> String {
    format!(
        "Is +{} still your number?",
        formatted_phone.trim_start_matches('+')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_adds_one_plus() {
        assert_eq!(
            title("+1 555 010 1031"),
            "Is +1 555 010 1031 still your number?"
        );
        assert_eq!(title("15550101031"), "Is +15550101031 still your number?");
    }

    #[test]
    fn hide_request_names_the_phone_action() {
        let json = crate::telegram::requests_privacy::hide_suggested_action(
            crate::ids::RequestId(3),
            crate::chatlist_suggestions::ACTION_PHONE,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "hideSuggestedAction");
        assert_eq!(v["action"]["@type"], "suggestedActionCheckPhoneNumber");
    }
}

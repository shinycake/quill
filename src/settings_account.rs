//! The help rows and the version line at the bottom of Settings
//! (tdesktop `settings_main.cpp`: Telegram FAQ, Telegram Features,
//! Ask a Question).

/// tdesktop `lng_settings_faq_link`.
pub const FAQ_URL: &str = "https://telegram.org/faq#general-questions";

/// tdesktop `lng_telegram_features_url`.
pub const FEATURES_URL: &str = "https://t.me/TelegramTips";

/// Telegram's privacy policy.
pub const PRIVACY_POLICY_URL: &str = "https://telegram.org/privacy";

/// Where each Quill release's notes are published.
pub const CHANGELOG_URL: &str = crate::updater::RELEASES_URL;

/// tdesktop `lng_settings_ask_sure`.
pub const ASK_QUESTION_NOTE: &str = "Telegram Support is run by volunteers. They answer as fast as they can, but it may take a while.\n\nThe Telegram FAQ has troubleshooting tips and answers to most questions, so it is worth a look first.";

/// The Settings footer line, for example `Quill 2026.10.9`.
pub fn version_line(version: &str) -> String {
    format!("Quill {version}")
}

/// The footer line for the running build.
pub fn current_version_line() -> String {
    version_line(crate::version::APP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_shows_the_running_version() {
        assert_eq!(version_line("2026.10.9"), "Quill 2026.10.9");
        assert_eq!(
            current_version_line(),
            format!("Quill {}", crate::version::APP)
        );
    }

    #[test]
    fn help_links_are_https() {
        for url in [FAQ_URL, FEATURES_URL, PRIVACY_POLICY_URL, CHANGELOG_URL] {
            assert!(url.starts_with("https://"), "{url}");
        }
    }

    #[test]
    fn changelog_points_at_quill_releases() {
        assert!(CHANGELOG_URL.contains("shinycake/quill"));
    }
}

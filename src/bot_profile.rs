//! What a bot's profile offers besides its description: Bot Help, Bot
//! Settings and the privacy policy. Mirrors tdesktop's `addBotCommandActions`
//! (`info/profile/info_profile_actions.cpp`): a row shows only when the bot
//! lists that command, and the privacy row opens the bot's policy link, else
//! sends `/privacy`, else opens the default page.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::BotCommand;

/// The bot's own spelling of `wanted` (matched without case), so
/// `/Help` is sent as the bot registered it.
pub fn find_command<'a>(commands: &'a [BotCommand], wanted: &str) -> Option<&'a str> {
    commands
        .iter()
        .find(|command| command.command.eq_ignore_ascii_case(wanted))
        .map(|command| command.command.as_str())
}

/// Where Bot Privacy Policy leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyTarget {
    /// `botInfo.privacy_policy_url`.
    Url(String),
    /// The bot's own `/privacy` command, sent to the chat.
    Command(String),
    /// Neither: the default policy page for third-party bots.
    Default,
}

/// `openPrivacyPolicy`: the published link first, then `/privacy`, then
/// the default page.
pub fn privacy_target(policy_url: &str, commands: &[BotCommand]) -> PrivacyTarget {
    if !policy_url.is_empty() {
        return PrivacyTarget::Url(policy_url.to_string());
    }
    match find_command(commands, "privacy") {
        Some(command) => PrivacyTarget::Command(command.to_string()),
        None => PrivacyTarget::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::{PrivacyTarget, find_command, privacy_target};
    use crate::telegram::envelope::BotCommand;

    fn commands(names: &[&str]) -> Vec<BotCommand> {
        names
            .iter()
            .map(|name| BotCommand {
                command: (*name).to_string(),
                description: String::new(),
                is_ephemeral: false,
            })
            .collect()
    }

    #[test]
    fn commands_match_without_case_and_keep_the_bots_spelling() {
        let list = commands(&["start", "Help"]);
        assert_eq!(find_command(&list, "help"), Some("Help"));
        assert_eq!(find_command(&list, "settings"), None);
    }

    #[test]
    fn privacy_prefers_the_link_then_the_command_then_the_default() {
        let with_command = commands(&["privacy"]);
        assert_eq!(
            privacy_target("https://example.com/p", &with_command),
            PrivacyTarget::Url("https://example.com/p".into())
        );
        assert_eq!(
            privacy_target("", &with_command),
            PrivacyTarget::Command("privacy".into())
        );
        assert_eq!(
            privacy_target("", &commands(&["start"])),
            PrivacyTarget::Default
        );
    }
}

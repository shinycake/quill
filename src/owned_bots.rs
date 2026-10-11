//! Bots the viewer owns: editing one and creating a managed one.
//!
//! Telegram Desktop has no "My Bots" list (it never calls the owned-bots
//! method), so Quill has none either. What it does have:
//!
//! - `boxes/peers/edit_peer_info_box.cpp`: a bot whose `canEditInformation`
//!   is set gets the group-style edit screen: photo, name (`bots.setBotInfo`
//!   with `f_name`) and "about" (the same call with `f_about`), plus rows
//!   that open @BotFather with `<username>-intro`, `<username>-commands`
//!   and `<username>` for the intro, the command list and the settings.
//! - `boxes/peers/create_managed_bot_box.cpp` and
//!   `core/deep_links/deep_links_new.cpp`: "Create Bot" is reached only from a
//!   `tg://newbot?manager=&username=&name=` link (TDLib
//!   `internalLinkTypeRequestManagedBot`). The manager bot must have
//!   `can_manage_bots`; the box has a name and a username field with the
//!   fixed `bot` ending, checks the username, then creates the bot.
//!
//! This module holds the rules and request bodies; the dialogs live in
//! `ui/bot_edit_dialog.rs` and `ui/bot_create_dialog.rs`.

use serde_json::json;

/// The bot's name length limit (`setBotName`: 0-64 characters).
pub const NAME_MAX: usize = 64;
/// tdesktop caps the edit field at `kMaxChannelDescription`.
pub const ABOUT_MAX: usize = 255;
/// tdesktop `kMinUsernameLength` and `kMaxUsernameLength`.
pub const USERNAME_MIN: usize = 5;
pub const USERNAME_MAX: usize = 32;
/// The ending every created bot's username has; the dialog appends what is
/// missing of it.
pub const BOT_SUFFIX: &str = "bot";
/// The bot that owns the edit commands in tdesktop (`kBotManagerUsername`).
pub const BOT_MANAGER_USERNAME: &str = "BotFather";

/// Which @BotFather screen a row opens (tdesktop `fillBotEdit*Button`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerScreen {
    Intro,
    Commands,
    Settings,
}

impl ManagerScreen {
    pub const ALL: [ManagerScreen; 3] = [Self::Intro, Self::Commands, Self::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Self::Intro => "Edit intro",
            Self::Commands => "Edit commands",
            Self::Settings => "Change bot settings",
        }
    }

    /// The `/start` parameter @BotFather expects for this bot.
    pub fn start_parameter(self, bot_username: &str) -> String {
        match self {
            Self::Intro => format!("{bot_username}-intro"),
            Self::Commands => format!("{bot_username}-commands"),
            Self::Settings => bot_username.to_string(),
        }
    }
}

/// How many characters of `value` already spell the start of "bot", in any
/// capitalization: "..b" is 1, "..bo" is 2, "..bot" is 3.
fn bot_overlap(value: &str) -> usize {
    let lower: Vec<char> = value.to_lowercase().chars().collect();
    (1..=BOT_SUFFIX.len().min(lower.len()))
        .rev()
        .find(|&k| {
            lower[lower.len() - k..]
                .iter()
                .copied()
                .eq(BOT_SUFFIX.chars().take(k))
        })
        .unwrap_or(0)
}

/// What the dialog appends to the typed username, empty once the text
/// already ends with "bot".
pub fn missing_suffix(value: &str) -> &'static str {
    &BOT_SUFFIX[bot_overlap(value)..]
}

/// The typed text without a leading `@` or spaces.
pub fn clean_username(raw: &str) -> String {
    raw.trim().trim_start_matches('@').trim().to_string()
}

/// The username that gets checked and created: typed text plus the missing
/// part of "bot".
pub fn full_username(raw: &str) -> String {
    let clean = clean_username(raw);
    let suffix = missing_suffix(&clean);
    format!("{clean}{suffix}")
}

/// Keep as many typed characters as fit together with the shortest suffix
/// that is still to be added.
pub fn fit_username(raw: &str) -> String {
    let mut kept: String = raw.to_string();
    while !kept.is_empty() && kept.chars().count() + missing_suffix(&kept).len() > USERNAME_MAX {
        kept.pop();
    }
    kept
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsernameProblem {
    Empty,
    BadSymbols,
    TooShort,
}

impl UsernameProblem {
    /// The line under the field (tdesktop `lng_create_bot_username_*`);
    /// empty for an empty field.
    pub fn line(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::BadSymbols => "Username can only contain a-z, 0-9, and underscores.",
            Self::TooShort => "Username must be at least 5 characters.",
        }
    }
}

/// The local checks before the server is asked (tdesktop `usernameChanged`).
pub fn username_problem(raw: &str) -> Option<UsernameProblem> {
    let clean = clean_username(raw);
    if clean.is_empty() {
        return Some(UsernameProblem::Empty);
    }
    if !clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Some(UsernameProblem::BadSymbols);
    }
    if full_username(raw).chars().count() < USERNAME_MIN {
        return Some(UsernameProblem::TooShort);
    }
    None
}

/// What changed between the loaded texts and the fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditChanges {
    pub name: Option<String>,
    pub about: Option<String>,
}

impl EditChanges {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.about.is_none()
    }
}

/// Fields that differ from what the server holds. A name must not be empty
/// (the empty-language name is the fallback for everyone).
pub fn edit_changes(
    current_name: &str,
    current_about: &str,
    name: &str,
    about: &str,
) -> Result<EditChanges, &'static str> {
    let name = name.trim();
    let about = about.trim();
    if name.is_empty() {
        return Err("The name can't be empty.");
    }
    if name.chars().count() > NAME_MAX {
        return Err("The name is too long.");
    }
    if about.chars().count() > ABOUT_MAX {
        return Err("The about text is too long.");
    }
    Ok(EditChanges {
        name: (name != current_name.trim()).then(|| name.to_string()),
        about: (about != current_about.trim()).then(|| about.to_string()),
    })
}

/// `getBotName` for the default language (TDLib `schema/td_api.tl:15447`).
pub fn get_name_request(extra: &str, bot_user_id: i64) -> String {
    json!({
        "@type": "getBotName",
        "@extra": extra,
        "bot_user_id": bot_user_id,
        "language_code": "",
    })
    .to_string()
}

/// `getBotInfoShortDescription` (`schema/td_api.tl:15492`).
pub fn get_about_request(extra: &str, bot_user_id: i64) -> String {
    json!({
        "@type": "getBotInfoShortDescription",
        "@extra": extra,
        "bot_user_id": bot_user_id,
        "language_code": "",
    })
    .to_string()
}

/// `setBotName` (`schema/td_api.tl:15442`).
pub fn set_name_request(extra: &str, bot_user_id: i64, name: &str) -> String {
    json!({
        "@type": "setBotName",
        "@extra": extra,
        "bot_user_id": bot_user_id,
        "language_code": "",
        "name": name,
    })
    .to_string()
}

/// `setBotInfoShortDescription` (`schema/td_api.tl:15487`): tdesktop's
/// "about" is the short description.
pub fn set_about_request(extra: &str, bot_user_id: i64, about: &str) -> String {
    json!({
        "@type": "setBotInfoShortDescription",
        "@extra": extra,
        "bot_user_id": bot_user_id,
        "language_code": "",
        "short_description": about,
    })
    .to_string()
}

/// `setBotProfilePhoto` with a local file (`schema/td_api.tl:15450`).
pub fn set_photo_request(extra: &str, bot_user_id: i64, path: &str) -> String {
    json!({
        "@type": "setBotProfilePhoto",
        "@extra": extra,
        "bot_user_id": bot_user_id,
        "photo": {
            "@type": "inputChatPhotoStatic",
            "photo": {"@type": "inputFileLocal", "path": path},
        },
    })
    .to_string()
}

/// `checkBotUsername` for a new bot (`schema/td_api.tl:15414`).
pub fn check_username_request(extra: &str, username: &str) -> String {
    json!({
        "@type": "checkBotUsername",
        "@extra": extra,
        "username": username,
        "is_secondary": false,
    })
    .to_string()
}

/// `createBot` (`schema/td_api.tl:15425`); `via_link` is true because the
/// dialog is only reached from a managed-bot link.
pub fn create_request(
    extra: &str,
    manager_bot_user_id: i64,
    name: &str,
    username: &str,
    via_link: bool,
) -> String {
    json!({
        "@type": "createBot",
        "@extra": extra,
        "manager_bot_user_id": manager_bot_user_id,
        "name": name,
        "username": username,
        "via_link": via_link,
    })
    .to_string()
}

/// Hint under a failed `createBot` (tdesktop shows the limit with a link to
/// @BotFather's delete flow; the server's reason is not kept).
pub const CREATE_FAILED_HINT: &str =
    "If you reached your limit of bots, delete one with @BotFather and try again.";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn suffix_is_appended_only_where_missing() {
        assert_eq!(full_username("cool"), "coolbot");
        assert_eq!(full_username("coolb"), "coolbot");
        assert_eq!(full_username("coolbo"), "coolbot");
        assert_eq!(full_username("coolbot"), "coolbot");
        assert_eq!(full_username("CoolBOT"), "CoolBOT");
        assert_eq!(full_username("@cool"), "coolbot");
        assert_eq!(missing_suffix("x"), "bot");
        assert_eq!(missing_suffix("xbo"), "t");
    }

    #[test]
    fn username_rules_match_the_box() {
        assert_eq!(username_problem(""), Some(UsernameProblem::Empty));
        assert_eq!(username_problem("  @ "), Some(UsernameProblem::Empty));
        assert_eq!(username_problem("a-b"), Some(UsernameProblem::BadSymbols));
        assert_eq!(username_problem("é"), Some(UsernameProblem::BadSymbols));
        // "a" + "bot" is four characters; "ab" reuses its b: "abot".
        assert_eq!(username_problem("a"), Some(UsernameProblem::TooShort));
        assert_eq!(username_problem("abc"), None);
        assert_eq!(username_problem("a_b"), None);
    }

    #[test]
    fn long_usernames_are_cut_to_leave_room_for_the_ending() {
        let long = "a".repeat(40);
        let fitted = fit_username(&long);
        assert_eq!(fitted.len(), USERNAME_MAX - BOT_SUFFIX.len());
        assert_eq!(full_username(&fitted).len(), USERNAME_MAX);
        // Typing the ending leaves no suffix to reserve.
        let with_end = format!("{}bot", "a".repeat(29));
        assert_eq!(fit_username(&with_end), with_end);
    }

    #[test]
    fn botfather_parameters() {
        assert_eq!(
            ManagerScreen::Intro.start_parameter("my_bot"),
            "my_bot-intro"
        );
        assert_eq!(
            ManagerScreen::Commands.start_parameter("my_bot"),
            "my_bot-commands"
        );
        assert_eq!(ManagerScreen::Settings.start_parameter("my_bot"), "my_bot");
    }

    #[test]
    fn changes_list_only_edited_fields() {
        let same = edit_changes("Bot", "About", " Bot ", "About ").unwrap();
        assert!(same.is_empty());
        let both = edit_changes("Bot", "About", "New", "").unwrap();
        assert_eq!(both.name.as_deref(), Some("New"));
        assert_eq!(both.about.as_deref(), Some(""));
        assert!(edit_changes("Bot", "", "  ", "").is_err());
        assert!(edit_changes("Bot", "", &"n".repeat(NAME_MAX + 1), "").is_err());
        assert!(edit_changes("Bot", "", "Bot", &"a".repeat(ABOUT_MAX + 1)).is_err());
    }

    #[test]
    fn request_bodies() {
        let v = parse(set_name_request("7", 21, "New"));
        assert_eq!(v["@type"], "setBotName");
        assert_eq!(v["bot_user_id"], 21);
        assert_eq!(v["language_code"], "");
        assert_eq!(v["name"], "New");
        let v = parse(set_about_request("7", 21, "Hi"));
        assert_eq!(v["@type"], "setBotInfoShortDescription");
        assert_eq!(v["short_description"], "Hi");
        let v = parse(set_photo_request("7", 21, "/tmp/a.png"));
        assert_eq!(v["photo"]["@type"], "inputChatPhotoStatic");
        assert_eq!(v["photo"]["photo"]["path"], "/tmp/a.png");
        assert_eq!(parse(get_name_request("7", 21))["@type"], "getBotName");
        assert_eq!(
            parse(get_about_request("7", 21))["@type"],
            "getBotInfoShortDescription"
        );
        let v = parse(check_username_request("7", "coolbot"));
        assert_eq!(v["@type"], "checkBotUsername");
        assert_eq!(v["is_secondary"], false);
        let v = parse(create_request("7", 5, "Cool", "coolbot", true));
        assert_eq!(v["@type"], "createBot");
        assert_eq!(v["manager_bot_user_id"], 5);
        assert_eq!(v["via_link"], true);
    }
}

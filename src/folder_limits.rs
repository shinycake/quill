//! Folder limits and Premium upsells, folder tag chips, the shared-folder
//! "new chats" bar. Pure logic — no GPUI, no TDLib I/O.
//!
//! tdesktop reference: `boxes/premium_limits_box.cpp` (`FiltersLimitBox`,
//! `FilterChatsLimitBox`, `FilterLinksLimitBox`, `ShareableFiltersLimitBox`),
//! `data/data_premium_limits.cpp` (the limits and their fallbacks),
//! `api/api_chat_filters.cpp` (`ShowImportError`),
//! `ui/chat/more_chats_bar.cpp` and `boxes/filters/edit_filter_box.cpp`
//! (the tag colour row).

use crate::telegram::envelope::OptionValue;

/// Which limit a box talks about (or, for `Tags`, which Premium feature).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FolderLimitKind {
    /// Number of folders (`chat_folder_count_max`).
    Folders,
    /// Pinned plus always-included chats of one folder
    /// (`chat_folder_chosen_chat_count_max`).
    ChatsIncluded,
    /// Always-excluded chats of one folder (same option).
    ChatsExcluded,
    /// Invite links per folder (`chat_folder_invite_link_count_max`).
    InviteLinks,
    /// Shared folders added by links (`added_shareable_chat_folder_count_max`).
    SharedFolders,
    /// Folder tags are a Premium feature (not a count).
    Tags,
}

impl FolderLimitKind {
    /// The `premiumLimitType*` constructor behind the limit, if it has one.
    pub fn td_limit_type(self) -> Option<&'static str> {
        match self {
            Self::Folders => Some("premiumLimitTypeChatFolderCount"),
            Self::ChatsIncluded | Self::ChatsExcluded => {
                Some("premiumLimitTypeChatFolderChosenChatCount")
            }
            Self::InviteLinks => Some("premiumLimitTypeChatFolderInviteLinkCount"),
            Self::SharedFolders => Some("premiumLimitTypeShareableChatFolderCount"),
            Self::Tags => None,
        }
    }

    /// A small number telling the limit families apart (stamped on the
    /// `getPremiumLimit` request so a repeat can be spotted).
    pub fn request_code(self) -> i32 {
        match self.family() {
            Self::Folders => 1,
            Self::ChatsIncluded | Self::ChatsExcluded => 2,
            Self::InviteLinks => 3,
            Self::SharedFolders => 4,
            Self::Tags => 5,
        }
    }

    /// Both chat boxes share one limit.
    fn family(self) -> Self {
        match self {
            Self::ChatsExcluded => Self::ChatsIncluded,
            other => other,
        }
    }

    /// The TDLib option holding the account's current value.
    fn option_name(self) -> Option<&'static str> {
        match self {
            Self::Folders => Some("chat_folder_count_max"),
            Self::ChatsIncluded | Self::ChatsExcluded => Some("chat_folder_chosen_chat_count_max"),
            Self::InviteLinks => Some("chat_folder_invite_link_count_max"),
            Self::SharedFolders => Some("added_shareable_chat_folder_count_max"),
            Self::Tags => None,
        }
    }

    /// `(default, premium)` when neither TDLib option nor `getPremiumLimit`
    /// has answered yet (Telegram's published free / Premium limits).
    fn fallback(self) -> (i32, i32) {
        match self {
            Self::Folders => (10, 20),
            Self::ChatsIncluded | Self::ChatsExcluded => (100, 200),
            Self::InviteLinks => (3, 20),
            Self::SharedFolders => (2, 20),
            Self::Tags => (0, 0),
        }
    }
}

/// The folder-related TDLib options and `getPremiumLimit` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderLimits {
    current: Vec<(FolderLimitKind, i32)>,
    premium_pairs: Vec<(FolderLimitKind, (i32, i32))>,
    new_chats_period_secs: Option<i64>,
}

/// tdesktop `RequestUpdatesEach` falls back to an hour.
pub const DEFAULT_NEW_CHATS_PERIOD_SECS: i64 = 3600;

fn narrow(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

impl FolderLimits {
    /// Fold one `updateOption`; unrelated names are ignored.
    pub fn apply_option(&mut self, name: &str, value: &OptionValue) {
        let OptionValue::Integer(number) = value else {
            return;
        };
        if name == "chat_folder_new_chats_update_period" {
            self.new_chats_period_secs = Some(*number);
            return;
        }
        for kind in [
            FolderLimitKind::Folders,
            FolderLimitKind::ChatsIncluded,
            FolderLimitKind::InviteLinks,
            FolderLimitKind::SharedFolders,
        ] {
            if kind.option_name() == Some(name) {
                self.current.retain(|(k, _)| *k != kind);
                self.current.push((kind, narrow(*number)));
            }
        }
    }

    /// Fold one `premiumLimit` answer (`getPremiumLimit`).
    pub fn apply_premium_limit(&mut self, type_name: &str, default_value: i32, premium_value: i32) {
        for kind in [
            FolderLimitKind::Folders,
            FolderLimitKind::ChatsIncluded,
            FolderLimitKind::InviteLinks,
            FolderLimitKind::SharedFolders,
        ] {
            if kind.td_limit_type() == Some(type_name) {
                self.premium_pairs.retain(|(k, _)| *k != kind);
                self.premium_pairs
                    .push((kind, (default_value, premium_value)));
            }
        }
    }

    /// Whether `getPremiumLimit` has answered for `kind`.
    pub fn has_premium_pair(&self, kind: FolderLimitKind) -> bool {
        let kind = kind.family();
        self.premium_pairs.iter().any(|(k, _)| *k == kind)
    }

    fn pair(&self, kind: FolderLimitKind) -> (i32, i32) {
        let kind = kind.family();
        self.premium_pairs
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, pair)| *pair)
            .unwrap_or_else(|| kind.fallback())
    }

    /// The limit that applies to this account now.
    pub fn current(&self, kind: FolderLimitKind, is_premium: bool) -> i32 {
        let family = kind.family();
        if let Some((_, value)) = self.current.iter().find(|(k, _)| *k == family) {
            return *value;
        }
        let (default, premium) = self.pair(kind);
        if is_premium { premium } else { default }
    }

    /// The free limit.
    pub fn default_value(&self, kind: FolderLimitKind) -> i32 {
        self.pair(kind).0
    }

    /// The Premium limit, shown as what a subscription would give.
    pub fn premium_value(&self, kind: FolderLimitKind) -> i32 {
        self.pair(kind).1
    }

    /// Seconds between `getChatFolderNewChats` calls for one folder.
    pub fn new_chats_period_secs(&self) -> i64 {
        self.new_chats_period_secs
            .filter(|secs| *secs > 0)
            .unwrap_or(DEFAULT_NEW_CHATS_PERIOD_SECS)
    }

    /// Making one more folder would pass the limit.
    pub fn folders_full(&self, folder_count: usize, is_premium: bool) -> bool {
        folder_count as i64 >= i64::from(self.current(FolderLimitKind::Folders, is_premium))
    }

    /// One more link would pass the limit.
    pub fn links_full(&self, link_count: usize, is_premium: bool) -> bool {
        link_count as i64 >= i64::from(self.current(FolderLimitKind::InviteLinks, is_premium))
    }

    /// The chat counts of a folder spec against the chosen-chat limit: the
    /// first kind that is over, if any. Pinned and always-included chats
    /// share one allowance; the excluded ones have their own.
    pub fn chats_over(
        &self,
        pinned_and_included: usize,
        excluded: usize,
        is_premium: bool,
    ) -> Option<FolderLimitKind> {
        let max = i64::from(self.current(FolderLimitKind::ChatsIncluded, is_premium));
        if pinned_and_included as i64 > max {
            Some(FolderLimitKind::ChatsIncluded)
        } else if excluded as i64 > max {
            Some(FolderLimitKind::ChatsExcluded)
        } else {
            None
        }
    }
}

fn plural(count: i32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The text of a limit box: `(title, message)`. Without Premium the
/// message ends with the upsell sentence (tdesktop `lng_*_limit1` and
/// `lng_*_limit2`).
pub fn limit_box_text(
    kind: FolderLimitKind,
    current: i32,
    premium_value: i32,
    is_premium: bool,
) -> (String, String) {
    let upsell = |sentence: String| -> String { if is_premium { String::new() } else { sentence } };
    let join = |first: String, second: String| -> String {
        if second.is_empty() {
            first
        } else {
            format!("{first} {second}")
        }
    };
    match kind {
        FolderLimitKind::Folders => (
            "Limit Reached".into(),
            join(
                format!(
                    "You have reached the limit of {}.",
                    plural(current, "folder", "folders")
                ),
                upsell(format!(
                    "You can double the limit to {} by subscribing to Telegram Premium.",
                    plural(premium_value, "folder", "folders")
                )),
            ),
        ),
        FolderLimitKind::ChatsIncluded => (
            "Limit Reached".into(),
            join(
                format!(
                    "Sorry, you can't add more than {} to a folder.",
                    plural(current, "chat", "chats")
                ),
                upsell(format!(
                    "You can increase this limit to {premium_value} by subscribing to Telegram Premium."
                )),
            ),
        ),
        FolderLimitKind::ChatsExcluded => (
            "Limit Reached".into(),
            join(
                format!(
                    "Sorry, you can't exclude more than {} from a folder.",
                    plural(current, "chat", "chats")
                ),
                upsell(format!(
                    "You can increase this limit to {premium_value} by subscribing to Telegram Premium."
                )),
            ),
        ),
        FolderLimitKind::InviteLinks => (
            "Limit Reached".into(),
            join(
                format!(
                    "Sorry, you can't create more than {}.",
                    plural(current, "invite link", "invite links")
                ),
                upsell(format!(
                    "You can increase the limit to {} by subscribing to Telegram Premium.",
                    plural(premium_value, "link", "links")
                )),
            ),
        ),
        FolderLimitKind::SharedFolders => (
            "Limit Reached".into(),
            join(
                format!("Sorry, you can't add more than {current} shareable folders."),
                upsell(format!(
                    "You can increase the limit to {} by subscribing to Telegram Premium.",
                    plural(premium_value, "folder", "folders")
                )),
            ),
        ),
        FolderLimitKind::Tags => (
            "Tag Your Chats".into(),
            "Display folder names for each chat in the chat list. This is a Telegram Premium feature."
                .into(),
        ),
    }
}

/// What the server message of a failed folder request said about limits.
/// `TdError` keeps only this, never the message text itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitHint {
    /// A limit was hit; the message did not say which.
    Generic,
    /// Too many always-included or pinned chats.
    Include,
    /// Too many always-excluded chats.
    Exclude,
    /// Too many shared folders (`CHATLISTS_TOO_MUCH`).
    ChatLists,
}

/// Classify a server message: Telegram answers with names like
/// `FILTER_INCLUDE_TOO_MUCH`, `DIALOG_FILTERS_TOO_MUCH`,
/// `CHATLISTS_TOO_MUCH` (tdesktop `ShowImportError`); TDLib may also word
/// its own checks. Errors about being in too many channels are a different
/// limit and stay plain errors.
pub fn limit_hint(message: &str) -> Option<LimitHint> {
    let m = message.to_ascii_uppercase();
    let limit_like = m.contains("TOO_MUCH")
        || m.contains("TOO MANY")
        || m.contains("MAXIMUM NUMBER")
        || m.contains("EXCEEDED");
    if !limit_like || m.contains("CHANNELS_TOO_MUCH") {
        return None;
    }
    Some(if m.contains("CHATLISTS") {
        LimitHint::ChatLists
    } else if m.contains("EXCLUDE") {
        LimitHint::Exclude
    } else if m.contains("INCLUDE") {
        LimitHint::Include
    } else {
        LimitHint::Generic
    })
}

/// Which request an error answers (to pick the limit it talks about).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderOp {
    Create,
    Edit,
    CreateLink,
    AddByLink,
}

/// The limit a failed request ran into.
pub fn limit_kind_for_error(op: FolderOp, hint: LimitHint) -> FolderLimitKind {
    match hint {
        LimitHint::ChatLists => FolderLimitKind::SharedFolders,
        LimitHint::Exclude => FolderLimitKind::ChatsExcluded,
        LimitHint::Include => FolderLimitKind::ChatsIncluded,
        LimitHint::Generic => match op {
            FolderOp::Create => FolderLimitKind::Folders,
            FolderOp::Edit => FolderLimitKind::ChatsIncluded,
            FolderOp::CreateLink => FolderLimitKind::InviteLinks,
            FolderOp::AddByLink => FolderLimitKind::SharedFolders,
        },
    }
}

// ---------------------------------------------------------------------
// Tag colours
// ---------------------------------------------------------------------

/// Colours offered for a folder tag (`color_id` 0-6); "No tag" is -1.
pub const TAG_COLOR_COUNT: i32 = 7;

/// The colour row is shown when the account can use tags or could be sold
/// them (tdesktop: `tagsEnabled || !premium`).
pub fn tag_picker_visible(is_premium: bool, tags_enabled: bool) -> bool {
    tags_enabled || !is_premium
}

/// Choosing a colour needs Premium with folder tags switched on
/// (schema: "Can't be changed if folder tags are disabled or the current
/// user doesn't have Telegram Premium subscription").
pub fn tag_choice_allowed(is_premium: bool, tags_enabled: bool) -> bool {
    is_premium && tags_enabled
}

/// What the picker highlights: non-Premium accounts always sit on "No tag".
pub fn tag_shown_choice(color_id: i32, is_premium: bool) -> i32 {
    if is_premium {
        color_id.clamp(-1, TAG_COLOR_COUNT - 1)
    } else {
        -1
    }
}

/// One tag chip on a chat row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderTag {
    /// Upper-cased folder name (tdesktop draws tags in capitals).
    pub text: String,
    pub color_id: i32,
}

/// The tag chips of a chat: folders it is in that have a colour, in folder
/// order, minus the folder being viewed (tdesktop `hasChatsFilterTags`).
pub fn row_tags(
    chat_in_folder: impl Fn(i32) -> bool,
    folders: &[(i32, String, i32)],
    tags_enabled: bool,
    viewing: Option<i32>,
) -> Vec<FolderTag> {
    if !tags_enabled {
        return Vec::new();
    }
    folders
        .iter()
        .filter(|(id, _, color)| *color >= 0 && Some(*id) != viewing && chat_in_folder(*id))
        .map(|(_, name, color)| FolderTag {
            text: name.to_uppercase(),
            color_id: *color,
        })
        .collect()
}

// ---------------------------------------------------------------------
// New chats bar
// ---------------------------------------------------------------------

/// `(title, subtitle)` of the shared-folder bar (tdesktop
/// `lng_filters_bar_you_can` / `lng_filters_bar_view`).
pub fn new_chats_bar_text(count: usize) -> (String, String) {
    if count == 1 {
        (
            "You can join 1 new chat".into(),
            "Click here to view it".into(),
        )
    } else {
        (
            format!("You can join {count} new chats"),
            "Click here to view them".into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_set_the_current_limits() {
        let mut limits = FolderLimits::default();
        assert_eq!(limits.current(FolderLimitKind::Folders, false), 10);
        assert_eq!(limits.current(FolderLimitKind::Folders, true), 20);
        limits.apply_option("chat_folder_count_max", &OptionValue::Integer(12));
        assert_eq!(limits.current(FolderLimitKind::Folders, false), 12);
        limits.apply_option(
            "chat_folder_chosen_chat_count_max",
            &OptionValue::Integer(150),
        );
        assert_eq!(limits.current(FolderLimitKind::ChatsExcluded, true), 150);
        limits.apply_option(
            "chat_folder_invite_link_count_max",
            &OptionValue::Integer(5),
        );
        assert_eq!(limits.current(FolderLimitKind::InviteLinks, false), 5);
        limits.apply_option(
            "added_shareable_chat_folder_count_max",
            &OptionValue::Integer(2),
        );
        assert_eq!(limits.current(FolderLimitKind::SharedFolders, false), 2);
        // Other options and other value kinds leave everything alone.
        limits.apply_option("chat_folder_count_max", &OptionValue::Boolean(true));
        limits.apply_option("unrelated", &OptionValue::Integer(1));
        assert_eq!(limits.current(FolderLimitKind::Folders, false), 12);
    }

    #[test]
    fn premium_limit_answers_fill_the_upsell_value() {
        let mut limits = FolderLimits::default();
        assert!(!limits.has_premium_pair(FolderLimitKind::Folders));
        limits.apply_premium_limit("premiumLimitTypeChatFolderCount", 10, 30);
        assert!(limits.has_premium_pair(FolderLimitKind::Folders));
        assert_eq!(limits.premium_value(FolderLimitKind::Folders), 30);
        // Without the option, the pair also decides the current value.
        assert_eq!(limits.current(FolderLimitKind::Folders, false), 10);
        assert_eq!(limits.current(FolderLimitKind::Folders, true), 30);
        // Include and exclude share the chosen-chat answer.
        limits.apply_premium_limit("premiumLimitTypeChatFolderChosenChatCount", 100, 200);
        assert_eq!(limits.premium_value(FolderLimitKind::ChatsExcluded), 200);
    }

    #[test]
    fn counts_against_limits() {
        let limits = FolderLimits::default();
        assert!(!limits.folders_full(9, false));
        assert!(limits.folders_full(10, false));
        assert!(!limits.folders_full(10, true));
        assert!(limits.links_full(3, false));
        assert!(!limits.links_full(2, false));
        assert_eq!(limits.chats_over(100, 0, false), None);
        assert_eq!(
            limits.chats_over(101, 0, false),
            Some(FolderLimitKind::ChatsIncluded)
        );
        assert_eq!(
            limits.chats_over(5, 101, false),
            Some(FolderLimitKind::ChatsExcluded)
        );
        assert_eq!(limits.chats_over(101, 0, true), None);
    }

    #[test]
    fn new_chats_period_defaults_to_an_hour() {
        let mut limits = FolderLimits::default();
        assert_eq!(limits.new_chats_period_secs(), 3600);
        limits.apply_option(
            "chat_folder_new_chats_update_period",
            &OptionValue::Integer(900),
        );
        assert_eq!(limits.new_chats_period_secs(), 900);
        limits.apply_option(
            "chat_folder_new_chats_update_period",
            &OptionValue::Integer(0),
        );
        assert_eq!(limits.new_chats_period_secs(), 3600);
    }

    #[test]
    fn limit_texts_follow_tdesktop() {
        let (title, body) = limit_box_text(FolderLimitKind::Folders, 10, 20, false);
        assert_eq!(title, "Limit Reached");
        assert_eq!(
            body,
            "You have reached the limit of 10 folders. You can double the limit to 20 folders by subscribing to Telegram Premium."
        );
        // Premium accounts get no upsell sentence.
        let (_, body) = limit_box_text(FolderLimitKind::Folders, 20, 20, true);
        assert_eq!(body, "You have reached the limit of 20 folders.");
        let (_, body) = limit_box_text(FolderLimitKind::ChatsExcluded, 100, 200, false);
        assert!(body.starts_with("Sorry, you can't exclude more than 100 chats from a folder."));
        let (_, body) = limit_box_text(FolderLimitKind::InviteLinks, 1, 20, false);
        assert!(body.starts_with("Sorry, you can't create more than 1 invite link."));
        let (_, body) = limit_box_text(FolderLimitKind::SharedFolders, 2, 20, false);
        assert!(body.contains("2 shareable folders"));
        assert!(body.ends_with("by subscribing to Telegram Premium."));
        let (title, _) = limit_box_text(FolderLimitKind::Tags, 0, 0, false);
        assert_eq!(title, "Tag Your Chats");
    }

    #[test]
    fn errors_map_to_the_limit_they_name() {
        use FolderLimitKind as K;
        use FolderOp as Op;
        let kind = |op, message| limit_hint(message).map(|hint| limit_kind_for_error(op, hint));
        assert_eq!(
            kind(Op::AddByLink, "CHATLISTS_TOO_MUCH"),
            Some(K::SharedFolders)
        );
        assert_eq!(
            kind(Op::AddByLink, "FILTER_INCLUDE_TOO_MUCH"),
            Some(K::ChatsIncluded)
        );
        assert_eq!(
            kind(Op::Edit, "FILTER_EXCLUDE_TOO_MUCH"),
            Some(K::ChatsExcluded)
        );
        assert_eq!(
            kind(Op::Create, "Maximum number of folders exceeded"),
            Some(K::Folders)
        );
        assert_eq!(
            kind(Op::CreateLink, "INVITES_TOO_MUCH"),
            Some(K::InviteLinks)
        );
        assert_eq!(kind(Op::Edit, "Too many chats"), Some(K::ChatsIncluded));
        // Too many channels is another limit, other errors are not limits.
        assert_eq!(kind(Op::AddByLink, "CHANNELS_TOO_MUCH"), None);
        assert_eq!(kind(Op::Create, "FILTER_TITLE_EMPTY"), None);
    }

    #[test]
    fn tag_gating_matches_tdesktop() {
        // Premium with tags on: pick any colour.
        assert!(tag_picker_visible(true, true));
        assert!(tag_choice_allowed(true, true));
        // Premium with tags off: the row is hidden (TDLib refuses a colour).
        assert!(!tag_picker_visible(true, false));
        assert!(!tag_choice_allowed(true, false));
        // Free accounts see the row and get the upsell.
        assert!(tag_picker_visible(false, false));
        assert!(!tag_choice_allowed(false, true));
        assert_eq!(tag_shown_choice(3, true), 3);
        assert_eq!(tag_shown_choice(3, false), -1);
        assert_eq!(tag_shown_choice(99, true), TAG_COLOR_COUNT - 1);
    }

    #[test]
    fn row_tags_need_a_colour_and_skip_the_viewed_folder() {
        let folders = vec![
            (1, "Work".to_string(), 2),
            (2, "News".to_string(), -1),
            (3, "Family".to_string(), 5),
        ];
        let all = |_: i32| true;
        let tags = row_tags(all, &folders, true, None);
        assert_eq!(
            tags,
            vec![
                FolderTag {
                    text: "WORK".into(),
                    color_id: 2
                },
                FolderTag {
                    text: "FAMILY".into(),
                    color_id: 5
                },
            ]
        );
        // Viewing Work hides its own chip; tags off hides all.
        let tags = row_tags(all, &folders, true, Some(1));
        assert_eq!(tags.len(), 1);
        assert!(row_tags(all, &folders, false, None).is_empty());
        // Only folders the chat is in.
        assert!(row_tags(|id| id == 2, &folders, true, None).is_empty());
    }

    #[test]
    fn bar_text_is_singular_or_plural() {
        assert_eq!(
            new_chats_bar_text(1),
            (
                "You can join 1 new chat".to_string(),
                "Click here to view it".to_string()
            )
        );
        assert_eq!(new_chats_bar_text(3).0, "You can join 3 new chats");
    }
}

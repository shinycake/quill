//! Chat-level look and actions: backgrounds, themes, accents, action bars, deep links, reactions, send-as: the `chats` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;
use crate::telegram::envelope::{ChatAccent, ChatBackground, EmojiChatTheme};

pub struct ChatsState {
    /// `getInstalledBackgrounds` answer for the current theme; `None` until
    /// fetched.
    pub installed_backgrounds: Option<Vec<Background>>,
    /// The account's default wallpaper per theme (`false` light, `true`
    /// dark), from `updateDefaultBackground` and `setDefaultBackground`.
    pub default_backgrounds: HashMap<bool, Background>,
    /// Which theme the pending `setDefaultBackground` was for.
    pub background_set_for_dark: bool,
    /// `chat.background` / `updateChatBackground`, by chat id.
    pub chat_backgrounds: HashMap<i64, ChatBackground>,
    /// `chat.theme` (emoji theme name) / `updateChatTheme`, by chat id.
    pub chat_theme_names: HashMap<i64, String>,
    /// `chat.theme` when it is a gift theme: its settings, by chat id.
    pub chat_gift_themes: HashMap<i64, EmojiChatTheme>,
    /// `updateEmojiChatThemes`: the themes a private chat can pick.
    pub emoji_chat_themes: Vec<EmojiChatTheme>,
    /// `searchBackground` answer for a `bg/` link; `None` until it arrives.
    pub searched_background: Option<Background>,
    /// One-shot `getBackgroundUrl` answer: the link the wallpaper preview
    /// copies.
    pub background_url: Option<String>,
    /// One-shot: a wallpaper request failed; shown in Appearance.
    pub background_error: Option<String>,
    /// Acknowledged (`ok`) chat-look requests (`setChatTheme`,
    /// `setChatBackground`, `deleteChatBackground`); the chat colors dialog
    /// waits for them before it closes.
    pub chat_look_oks: u32,
    /// `updateChatOnlineMemberCount`, keyed by chat id.
    pub chat_online_counts: HashMap<i64, i32>,
    /// Slice CL1: one-shot error from a refused chat-list action
    /// (`toggleChatIsPinned`, `toggleChatIsMarkedAsUnread`,
    /// `deleteChatHistory`). The UI drains it into the status note so a
    /// refused action never looks like it worked.
    pub chat_action_error: Option<String>,
    /// Why the last channel or group appearance change was refused.
    pub appearance_error: Option<String>,
    /// Slice CL3: one-shot `reportChat` outcome (`reportChatResultOk`
    /// vs option/text/messages required). The UI drains it into the
    /// status note next to `chat_action_error`.
    pub report_chat_outcome: Option<String>,
    /// `chat.message_sender_id` / `updateChatMessageSender`: the "send as"
    /// identity selected per chat (absent when the user cannot change it).
    pub chat_message_sender: HashMap<i64, MessageSender>,
    /// `getChatAvailableMessageSenders` answers per chat.
    pub send_as_options: HashMap<i64, Vec<AvailableMessageSender>>,
    /// Batch 8: `chat.action_bar` / `updateChatActionBar` per chat — the
    /// Add contact / Block / Report spam / Share phone strip.
    pub chat_action_bars: HashMap<i64, ChatActionBar>,
    /// `chat.business_bot_manage_bar` / `updateChatBusinessBotManageBar`.
    pub business_bot_bars: HashMap<i64, crate::business_bot_bar::BusinessBotManageBar>,
    /// Chats whose content is protected (`chat.has_protected_content`,
    /// schema 1.8.67 line 3598): no saving, forwarding or copying.
    pub protected_chats: HashSet<i64>,
    /// Chats with scheduled messages (`chat.has_scheduled_messages`,
    /// `updateChatHasScheduledMessages`); drives the composer's
    /// scheduled-messages button.
    pub scheduled_chats: HashSet<i64>,
    /// Name color and reply emoji of chats that have one
    /// (`chat.accent_color_id`, `updateChatAccentColors`).
    pub chat_accents: HashMap<i64, ChatAccent>,
    /// Profile color, profile icon and emoji status of channels and groups
    /// (`chat.profile_accent_color_id`, `chat.emoji_status`).
    pub chat_looks: HashMap<i64, crate::telegram::envelope::ChatProfileLook>,
    /// Slice A12: accent palette from `updateProfileAccentColors`
    /// (schema 1.8.67, line 10964) — full `profileAccentColor` entries
    /// for swatch rendering.
    pub profile_accent_colors: Vec<ProfileAccentColor>,
    /// Name-color palette from `updateAccentColors` (sender names).
    pub name_accent_colors: Vec<crate::telegram::NameAccentColor>,
    /// Ids `setAccentColor` accepts, in server order (`updateAccentColors`):
    /// the name-color editor swatches. Empty until the update arrives.
    pub available_name_accent_ids: Vec<i32>,
    /// Slice A12: ids `setProfileAccentColor` accepts, in server order —
    /// the edit-profile accent picker rows.
    pub available_accent_color_ids: Vec<i32>,
    /// B7: `chat.available_reactions` / `updateChatAvailableReactions`,
    /// keyed by chat id.
    pub chat_available_reactions: HashMap<i64, crate::telegram::envelope::ChatAvailableReactions>,
    /// B7: finished basic group upgrades, `(old chat id, new chat id)`.
    pub chat_upgrades: Vec<(i64, i64)>,
    /// `parity:platform-deep-links`: the single active deep-link flow
    /// (launch link → `getDeepLinkInfo` → follow-up → open chat).
    pub deep_link: Option<DeepLinkState>,
    /// Generation counter for deep-link request correlation.
    pub deep_link_seq: u64,
    /// The link text being resolved by `getInternalLinkType` (the proxy
    /// hand-off needs it back).
    pub deep_link_original: String,
    /// The flow in `deep_link` was started by a step the agent UI driver
    /// injected. Set at every start of a flow, kept while it continues,
    /// taken by the UI at every terminal state; the UI re-checks what an
    /// agent's flow opens (`guard::with_injecting`).
    pub deep_link_from_agent: bool,
}

impl ChatsState {
    pub(crate) fn new() -> Self {
        Self {
            installed_backgrounds: None,
            default_backgrounds: HashMap::new(),
            background_set_for_dark: false,
            chat_backgrounds: HashMap::new(),
            chat_theme_names: HashMap::new(),
            chat_gift_themes: HashMap::new(),
            emoji_chat_themes: Vec::new(),
            searched_background: None,
            background_url: None,
            background_error: None,
            chat_look_oks: 0,
            chat_online_counts: HashMap::new(),
            chat_action_error: None,
            appearance_error: None,
            report_chat_outcome: None,
            chat_message_sender: HashMap::new(),
            send_as_options: HashMap::new(),
            chat_action_bars: HashMap::new(),
            business_bot_bars: HashMap::new(),
            protected_chats: HashSet::new(),
            scheduled_chats: HashSet::new(),
            chat_accents: HashMap::new(),
            chat_looks: HashMap::new(),
            profile_accent_colors: Vec::new(),
            name_accent_colors: Vec::new(),
            available_name_accent_ids: Vec::new(),
            available_accent_color_ids: Vec::new(),
            chat_available_reactions: HashMap::new(),
            chat_upgrades: Vec::new(),
            deep_link: None,
            deep_link_seq: 0,
            deep_link_original: String::new(),
            deep_link_from_agent: false,
        }
    }
}

//! Premium limits outside folders: the boxes for pinned chats, pinned
//! Saved Messages chats, captions, joined communities, public links, file
//! size and story posting, the toasts for saved GIFs, favorite stickers, similar channels
//! and owned bots, and the values TDLib reports for them. Pure logic, no
//! GPUI and no TDLib I/O.
//!
//! tdesktop reference: `boxes/premium_limits_box.cpp` (`SimpleLimitBox`,
//! `SimplePinsLimitBox`, `SublistsPinsLimitBox`, `ChannelsLimitBox`,
//! `PublicLinksLimitBox`, `CaptionLimitBox`, `CaptionLimitReachedBox`,
//! `FileSizeLimitBox`), `data/stickers/data_stickers.cpp` (`SavedGifsToast`,
//! `FaveStickersToast`), `history_view_similar_channels.cpp`,
//! `create_managed_bot_box.cpp` (`BOT_CREATE_LIMIT_EXCEEDED`),
//! `data/data_premium_limits.cpp` (the fallbacks used before the server
//! answers) and `Resources/langs/lang.strings` (`lng_*_limit*`). tdesktop
//! cannot post stories, so the story boxes follow the pins box and take
//! their limits from `canPostStoryResult*` and `getPremiumLimit`. Folder
//! limits live in [`crate::folder_limits`]; they keep their own box.
#![deny(clippy::indexing_slicing)]

/// Which limit a box talks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PremiumLimitKind {
    /// Pinned chats of the main list.
    Pins,
    /// Pinned chats of the archive.
    PinsArchive,
    /// Characters in a media caption.
    Caption,
    /// Saved GIFs.
    SavedGifs,
    /// Joined groups and channels.
    Channels,
    /// Reserved public usernames of owned groups and channels.
    PublicLinks,
    /// Size of one uploaded document, in gigabytes.
    FileSize,
    FavoriteStickers,
    /// Pinned chats of Saved Messages (tdesktop's sublists).
    SavedPins,
    SimilarChats,
    OwnedBots,
    ActiveStories,
    WeeklyStories,
    MonthlyStories,
}

/// The free and the Premium value of one limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitPair {
    pub default_value: i64,
    pub premium_value: i64,
}

impl PremiumLimitKind {
    /// The `/start` ref of the box's Increase Limit (tdesktop
    /// `LimitsPremiumRef`, or the plain ref the similar channels and bot
    /// toasts pass to `Settings::ShowPremium`).
    pub fn premium_ref(self) -> String {
        let addition = match self {
            Self::Pins => "dialog_pinned",
            Self::PinsArchive => "dialogs_folder_pinned",
            Self::Caption => "caption_length",
            Self::SavedGifs => "saved_gifs",
            Self::Channels => "channels",
            Self::PublicLinks => "channels_public",
            Self::FileSize => "upload_max_fileparts",
            Self::FavoriteStickers => "stickers_faved",
            Self::SavedPins => "saved_dialog_pinned",
            Self::SimilarChats => return "similar_channels".to_string(),
            Self::OwnedBots => return "managed_bots".to_string(),
            Self::ActiveStories | Self::WeeklyStories | Self::MonthlyStories => {
                return "stories".to_string();
            }
        };
        format!("double_limits__{addition}")
    }

    /// Every kind, in the order of the "Doubled limits" page.
    pub const ALL: [Self; 14] = [
        Self::Channels,
        Self::Pins,
        Self::PinsArchive,
        Self::PublicLinks,
        Self::SavedGifs,
        Self::FavoriteStickers,
        Self::Caption,
        Self::SimilarChats,
        Self::FileSize,
        Self::SavedPins,
        Self::OwnedBots,
        Self::ActiveStories,
        Self::WeeklyStories,
        Self::MonthlyStories,
    ];

    /// Whether the "Doubled limits" page lists it (tdesktop
    /// `DoubledLimitsPreviewBox` leaves out the Saved Messages pins, the
    /// bot count and the story limits).
    pub fn on_doubled_page(self) -> bool {
        !matches!(
            self,
            Self::SavedPins
                | Self::OwnedBots
                | Self::ActiveStories
                | Self::WeeklyStories
                | Self::MonthlyStories
        )
    }

    /// tdesktop tells about these with a toast, not a box: the saved GIF
    /// and favorite sticker lists drop their oldest entry, the similar
    /// channels strip and the bot creation box stay where they are.
    pub fn shows_toast(self) -> bool {
        matches!(
            self,
            Self::SavedGifs | Self::FavoriteStickers | Self::SimilarChats | Self::OwnedBots
        )
    }

    /// The TDLib option that holds the limit in force for this account,
    /// for the kinds whose option is read here.
    pub fn option_name(self) -> Option<&'static str> {
        match self {
            Self::FavoriteStickers => Some("favorite_stickers_limit"),
            Self::SavedPins => Some("pinned_saved_messages_topic_count_max"),
            Self::OwnedBots => Some("owned_bot_count_max"),
            _ => None,
        }
    }

    /// The `premiumLimitType*` constructor behind the limit. File size is
    /// a Premium feature in TDLib (`premiumFeatureIncreasedUploadFileSize`),
    /// not a limit type.
    pub fn td_limit_type(self) -> Option<&'static str> {
        match self {
            Self::Pins => Some("premiumLimitTypePinnedChatCount"),
            Self::PinsArchive => Some("premiumLimitTypePinnedArchivedChatCount"),
            Self::Caption => Some("premiumLimitTypeCaptionLength"),
            Self::SavedGifs => Some("premiumLimitTypeSavedAnimationCount"),
            Self::Channels => Some("premiumLimitTypeSupergroupCount"),
            Self::PublicLinks => Some("premiumLimitTypeCreatedPublicChatCount"),
            Self::FavoriteStickers => Some("premiumLimitTypeFavoriteStickerCount"),
            Self::SavedPins => Some("premiumLimitTypePinnedSavedMessagesTopicCount"),
            Self::SimilarChats => Some("premiumLimitTypeSimilarChatCount"),
            Self::OwnedBots => Some("premiumLimitTypeOwnedBotCount"),
            Self::ActiveStories => Some("premiumLimitTypeActiveStoryCount"),
            Self::WeeklyStories => Some("premiumLimitTypeWeeklyPostedStoryCount"),
            Self::MonthlyStories => Some("premiumLimitTypeMonthlyPostedStoryCount"),
            Self::FileSize => None,
        }
    }

    /// The reverse of [`Self::td_limit_type`].
    pub fn from_td_limit_type(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.td_limit_type() == Some(name))
    }

    /// tdesktop's values before the server answers
    /// (`data_premium_limits.cpp`), used until `getPremiumFeatures` or
    /// `getPremiumLimit` reports the real pair.
    pub fn fallback(self) -> LimitPair {
        let (default_value, premium_value) = match self {
            Self::Pins => (5, 10),
            Self::PinsArchive => (100, 200),
            Self::Caption => (1024, 4096),
            Self::SavedGifs => (200, 400),
            Self::Channels => (500, 1000),
            Self::PublicLinks => (10, 20),
            Self::FileSize => (2, 4),
            Self::FavoriteStickers => (5, 10),
            Self::SavedPins => (5, 100),
            Self::SimilarChats => (10, 100),
            Self::OwnedBots => (20, 40),
            Self::ActiveStories => (3, 100),
            Self::WeeklyStories => (7, 700),
            Self::MonthlyStories => (30, 3000),
        };
        LimitPair {
            default_value,
            premium_value,
        }
    }

    /// Row name on the "Doubled limits" page.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pins => "Pinned chats",
            Self::PinsArchive => "Pins in the archive",
            Self::Caption => "Caption length",
            Self::SavedGifs => "Saved GIFs",
            Self::Channels => "Groups and channels",
            Self::PublicLinks => "Public links",
            Self::FileSize => "File size",
            Self::FavoriteStickers => "Favorite stickers",
            Self::SavedPins => "Pinned saved chats",
            Self::SimilarChats => "Similar channels",
            Self::OwnedBots => "Bots",
            Self::ActiveStories => "Active stories",
            Self::WeeklyStories => "Stories per week",
            Self::MonthlyStories => "Stories per month",
        }
    }

    /// Unit shown after the numbers on the "Doubled limits" page.
    pub fn unit(self) -> &'static str {
        match self {
            Self::Pins | Self::PinsArchive => "chats",
            Self::Caption => "characters",
            Self::SavedGifs => "GIFs",
            Self::Channels => "communities",
            Self::PublicLinks => "links",
            Self::FileSize => "GB",
            Self::FavoriteStickers => "stickers",
            Self::SavedPins => "chats",
            Self::SimilarChats => "channels",
            Self::OwnedBots => "bots",
            Self::ActiveStories | Self::WeeklyStories | Self::MonthlyStories => "stories",
        }
    }

    /// tdesktop draws caption and file size boxes with the real position of
    /// the current value on the line; the others split it in half.
    fn complex_ratio(self) -> bool {
        matches!(self, Self::Caption | Self::FileSize)
    }
}

/// Classify the raw message of a failed request. The caller keeps only the
/// result, never the text (it can hold phone numbers or other secrets).
/// Names follow the server (`CHANNELS_TOO_MUCH`, `PINNED_DIALOGS_TOO_MUCH`,
/// `MEDIA_CAPTION_TOO_LONG`, ...) and TDLib's own wording.
pub fn kind_for_message(message: &str) -> Option<PremiumLimitKind> {
    let m = message.to_ascii_uppercase();
    if m.contains("BOT_CREATE_LIMIT_EXCEEDED") || m.contains("BOTS_TOO_MUCH") {
        return Some(PremiumLimitKind::OwnedBots);
    }
    let too_many = m.contains("TOO_MUCH") || m.contains("TOO MANY");
    // `CHANNELS_ADMIN_PUBLIC_TOO_MUCH`, `USERNAMES_ACTIVE_TOO_MUCH`
    if (m.contains("PUBLIC") && too_many) || m.contains("USERNAMES_ACTIVE_TOO_MUCH") {
        return Some(PremiumLimitKind::PublicLinks);
    }
    if m.contains("CHANNELS_TOO_MUCH") {
        return Some(PremiumLimitKind::Channels);
    }
    if m.contains("PINNED") && (too_many || m.contains("MAXIMUM") || m.contains("EXCEEDED")) {
        return Some(PremiumLimitKind::Pins);
    }
    if m.contains("CAPTION") && (m.contains("TOO_LONG") || m.contains("TOO LONG")) {
        return Some(PremiumLimitKind::Caption);
    }
    None
}

fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// What a limit box says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LimitBoxText {
    pub title: String,
    pub message: String,
    /// Whether the box offers "Increase Limit".
    pub offer_upgrade: bool,
}

/// The title and message of a limit box (tdesktop `lng_*_limit*`).
///
/// `shown` is the limit that applies to the account now. `premium` is the
/// Premium value. A Premium account, or one whose server offers no Premium,
/// gets the first sentence only and an OK button. `None` for the kinds
/// tdesktop tells about with a toast ([`PremiumLimitKind::shows_toast`]).
pub fn limit_box_text(
    kind: PremiumLimitKind,
    shown: i64,
    premium: i64,
    is_premium: bool,
    premium_possible: bool,
) -> Option<LimitBoxText> {
    let upsell = !is_premium && premium_possible;
    let join = |first: String, second: String| {
        if upsell {
            format!("{first} {second}")
        } else {
            first
        }
    };
    let (title, message) = match kind {
        PremiumLimitKind::Pins | PremiumLimitKind::PinsArchive | PremiumLimitKind::SavedPins => (
            "Limit Reached".to_string(),
            join(
                format!(
                    "Sorry, you can't pin more than {} to the top.",
                    plural(shown, "chat", "chats")
                ),
                format!(
                    "Unpin some of the currently pinned ones or subscribe to Telegram Premium to \
                     double the limit to {}.",
                    plural(premium, "chat", "chats")
                ),
            ),
        ),
        PremiumLimitKind::Channels => (
            "Too Many Communities".to_string(),
            join(
                format!(
                    "You are a member of {}.",
                    plural(shown, "group or channel", "groups and channels")
                ),
                format!(
                    "Please leave some before joining a new one, or upgrade to Telegram Premium \
                     to double the limit to {}.",
                    plural(premium, "group or channel", "groups and channels")
                ),
            ) + if upsell {
                ""
            } else {
                " Please leave some before joining a new one."
            },
        ),
        PremiumLimitKind::PublicLinks => (
            "Too Many Public Links".to_string(),
            join(
                format!(
                    "You have reserved {}.",
                    plural(shown, "public link", "public links")
                ),
                format!(
                    "Try revoking the link from an older group or channel, or subscribe to \
                     Telegram Premium to double the limit to {}.",
                    plural(premium, "public link", "public links")
                ),
            ) + if upsell {
                ""
            } else {
                " Try revoking the link from an older group or channel."
            },
        ),
        PremiumLimitKind::Caption => (
            "Limit Reached".to_string(),
            join(
                format!(
                    "Sorry, you can't use more than {} in media captions.",
                    plural(shown, "character", "characters")
                ),
                format!(
                    "Make the caption shorter or subscribe to Telegram Premium to double the \
                     limit to {}.",
                    plural(premium, "character", "characters")
                ),
            ),
        ),
        PremiumLimitKind::FileSize => (
            "File Too Large".to_string(),
            join(
                format!("The document can't be sent, because it is larger than {shown} GB."),
                format!(
                    "You can double this limit to {premium} GB per document by subscribing to \
                     Telegram Premium."
                ),
            ),
        ),
        PremiumLimitKind::ActiveStories => (
            "Limit Reached".to_string(),
            join(
                format!(
                    "Sorry, you can't have more than {} at once.",
                    plural(shown, "active story", "active stories")
                ),
                format!(
                    "Delete one or wait for the oldest to expire, or subscribe to Telegram \
                     Premium to raise the limit to {}.",
                    plural(premium, "story", "stories")
                ),
            ) + if upsell {
                ""
            } else {
                " Delete one or wait for the oldest to expire."
            },
        ),
        PremiumLimitKind::WeeklyStories | PremiumLimitKind::MonthlyStories => {
            let period = if kind == PremiumLimitKind::WeeklyStories {
                "week"
            } else {
                "month"
            };
            (
                "Limit Reached".to_string(),
                join(
                    format!(
                        "Sorry, you can't post more than {} a {period}.",
                        plural(shown, "story", "stories")
                    ),
                    format!(
                        "Subscribe to Telegram Premium to raise the limit to {} a {period}.",
                        plural(premium, "story", "stories")
                    ),
                ),
            )
        }
        PremiumLimitKind::SavedGifs
        | PremiumLimitKind::FavoriteStickers
        | PremiumLimitKind::SimilarChats
        | PremiumLimitKind::OwnedBots => return None,
    };
    Some(LimitBoxText {
        title,
        message,
        offer_upgrade: upsell,
    })
}

/// The limit now in force, given the Premium state and the pair.
pub fn shown_limit(pair: LimitPair, is_premium: bool) -> i64 {
    if is_premium {
        pair.premium_value
    } else {
        pair.default_value
    }
}

/// How far along the free-to-Premium line the current value sits, 0..=1.
/// Caption and file size use the true position (tdesktop `complexRatio`),
/// the rest sit in the middle.
pub fn line_ratio(kind: PremiumLimitKind, pair: LimitPair, current: i64) -> f32 {
    if !kind.complex_ratio() || pair.premium_value <= 0 {
        return 0.5;
    }
    (current as f32 / pair.premium_value as f32).clamp(0.05, 1.0)
}

/// The numbers as one cell of the "Doubled limits" page, `5 -> 10 chats`.
pub fn pair_text(kind: PremiumLimitKind, pair: LimitPair) -> String {
    format!(
        "{} \u{2192} {} {}",
        pair.default_value,
        pair.premium_value,
        kind.unit()
    )
}

const MIB: u64 = 1024 * 1024;

/// Largest upload for a free account: 4000 parts of 512 KiB (tdesktop
/// `uploadMaxDefault`).
pub const UPLOAD_MAX_FREE: u64 = 2000 * MIB;
/// Largest upload with Premium (`uploadMaxPremium`).
pub const UPLOAD_MAX_PREMIUM: u64 = 4000 * MIB;

/// How a file of some size fares against the upload limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileSizeCheck {
    Fits,
    /// Over the free limit; Premium would send it.
    NeedsPremium,
    /// Over even the Premium limit.
    TooLarge,
}

pub fn check_file_size(bytes: u64, is_premium: bool) -> FileSizeCheck {
    if bytes > UPLOAD_MAX_PREMIUM {
        FileSizeCheck::TooLarge
    } else if bytes > UPLOAD_MAX_FREE && !is_premium {
        FileSizeCheck::NeedsPremium
    } else {
        FileSizeCheck::Fits
    }
}

/// A saved GIF or favorite sticker list was already at its limit when one
/// more was added: TDLib drops the oldest entry silently, so the toast is
/// the only notice (tdesktop `Stickers::addSavedGif`, `checkFavedLimit`).
pub fn saved_list_full(saved_count: usize, limit: i64) -> bool {
    limit > 0 && i64::try_from(saved_count).unwrap_or(i64::MAX) >= limit
}

/// What TDLib said about the limits beyond `getPremiumFeatures`: the
/// `getPremiumLimit` answers, which ones were asked for, and the options
/// that hold the limit in force for this account.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LimitStore {
    answered: Vec<(PremiumLimitKind, LimitPair)>,
    asked: Vec<PremiumLimitKind>,
    options: Vec<(PremiumLimitKind, i64)>,
}

impl LimitStore {
    /// Fold one `premiumLimit` answer; other limit types are ignored.
    pub fn apply_premium_limit(&mut self, type_name: &str, default_value: i64, premium_value: i64) {
        let Some(kind) = PremiumLimitKind::from_td_limit_type(type_name) else {
            return;
        };
        self.answered.retain(|(k, _)| *k != kind);
        self.answered.push((
            kind,
            LimitPair {
                default_value,
                premium_value,
            },
        ));
    }

    pub fn answered(&self, kind: PremiumLimitKind) -> Option<LimitPair> {
        self.answered
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, pair)| *pair)
    }

    /// Whether `getPremiumLimit` should be sent for `kind`: it has a limit
    /// type and was neither answered nor asked for (an account that cannot
    /// buy Premium gets an error, so a refused one is not asked again).
    pub fn should_ask(&self, kind: PremiumLimitKind) -> bool {
        kind.td_limit_type().is_some()
            && self.answered(kind).is_none()
            && !self.asked.contains(&kind)
    }

    pub fn note_asked(&mut self, kind: PremiumLimitKind) {
        if !self.asked.contains(&kind) {
            self.asked.push(kind);
        }
    }

    /// A send failed: `kind` may be asked again.
    pub fn forget_asked(&mut self, kind: PremiumLimitKind) {
        self.asked.retain(|k| *k != kind);
    }

    /// An integer `updateOption`; only the options of
    /// [`PremiumLimitKind::option_name`] are kept.
    pub fn apply_option(&mut self, name: &str, value: i64) {
        let Some(kind) = PremiumLimitKind::ALL
            .into_iter()
            .find(|kind| kind.option_name() == Some(name))
        else {
            return;
        };
        self.options.retain(|(k, _)| *k != kind);
        if value > 0 {
            self.options.push((kind, value));
        }
    }

    pub fn option(&self, kind: PremiumLimitKind) -> Option<i64> {
        self.options
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, value)| *value)
    }
}

/// The toast after `createBot` hit the bot limit (tdesktop
/// `create_managed_bot_box.cpp`): the Premium offer for a free account
/// that can buy Premium, else the final notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotsLimitNotice {
    /// `lng_bots_create_limit_final`: `count` is the limit in force.
    Final { count: i64 },
    /// `lng_bots_create_limit`.
    Promo { count: i64, premium_count: i64 },
}

/// `count` is the limit in force (`owned_bot_count_max`), `premium_count`
/// the Premium value.
pub fn bots_limit_notice(
    count: i64,
    premium_count: i64,
    is_premium: bool,
    premium_possible: bool,
) -> BotsLimitNotice {
    if is_premium || !premium_possible {
        BotsLimitNotice::Final { count }
    } else {
        BotsLimitNotice::Promo {
            count,
            premium_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BotsLimitNotice, LimitPair, LimitStore, PremiumLimitKind as Kind, bots_limit_notice,
        kind_for_message, limit_box_text, line_ratio, pair_text, saved_list_full, shown_limit,
    };

    #[test]
    fn td_types_round_trip() {
        for kind in Kind::ALL {
            if let Some(name) = kind.td_limit_type() {
                assert_eq!(Kind::from_td_limit_type(name), Some(kind));
            }
        }
        assert_eq!(Kind::from_td_limit_type("premiumLimitTypeBioLength"), None);
        assert_eq!(Kind::FileSize.td_limit_type(), None);
    }

    #[test]
    fn messages_route_to_the_limit() {
        assert_eq!(kind_for_message("CHANNELS_TOO_MUCH"), Some(Kind::Channels));
        assert_eq!(
            kind_for_message("CHANNELS_ADMIN_PUBLIC_TOO_MUCH"),
            Some(Kind::PublicLinks)
        );
        assert_eq!(
            kind_for_message("USERNAMES_ACTIVE_TOO_MUCH"),
            Some(Kind::PublicLinks)
        );
        assert_eq!(
            kind_for_message("PINNED_DIALOGS_TOO_MUCH"),
            Some(Kind::Pins)
        );
        assert_eq!(
            kind_for_message("Maximum number of pinned chats exceeded"),
            Some(Kind::Pins)
        );
        assert_eq!(
            kind_for_message("MEDIA_CAPTION_TOO_LONG"),
            Some(Kind::Caption)
        );
        assert_eq!(
            kind_for_message("Message caption is too long"),
            Some(Kind::Caption)
        );
        // Folder limits and unrelated errors stay out of this box.
        assert_eq!(kind_for_message("FILTER_INCLUDE_TOO_MUCH"), None);
        assert_eq!(kind_for_message("CHAT_WRITE_FORBIDDEN"), None);
        assert_eq!(kind_for_message("MESSAGE_TOO_LONG"), None);
        assert_eq!(
            kind_for_message("BOT_CREATE_LIMIT_EXCEEDED"),
            Some(Kind::OwnedBots)
        );
    }

    #[test]
    fn free_accounts_get_the_upsell_sentence() {
        let free = limit_box_text(Kind::Pins, 5, 10, false, true).expect("a box");
        assert_eq!(free.title, "Limit Reached");
        assert!(
            free.message
                .starts_with("Sorry, you can't pin more than 5 chats")
        );
        assert!(free.message.contains("double the limit to 10 chats"));
        assert!(free.offer_upgrade);
    }

    #[test]
    fn premium_accounts_and_missing_premium_get_the_plain_sentence() {
        let premium = limit_box_text(Kind::Caption, 4096, 4096, true, true).expect("a box");
        assert!(!premium.offer_upgrade);
        assert!(!premium.message.contains("Premium"));
        let none = limit_box_text(Kind::Channels, 500, 1000, false, false).expect("a box");
        assert!(!none.offer_upgrade);
        assert!(none.message.ends_with("before joining a new one."));
        assert!(!none.message.contains("Premium"));
        let links = limit_box_text(Kind::PublicLinks, 10, 20, true, true).expect("a box");
        assert!(links.message.ends_with("an older group or channel."));
    }

    #[test]
    fn counts_use_the_singular() {
        let text = limit_box_text(Kind::Pins, 1, 2, false, true).expect("a box");
        assert!(text.message.contains("1 chat to the top"));
        assert_eq!(
            limit_box_text(Kind::FileSize, 2, 4, false, true)
                .expect("a box")
                .message,
            "The document can't be sent, because it is larger than 2 GB. You can double this \
             limit to 4 GB per document by subscribing to Telegram Premium."
        );
    }

    #[test]
    fn line_position_and_limit_in_force() {
        let pair = Kind::Caption.fallback();
        assert_eq!(shown_limit(pair, false), 1024);
        assert_eq!(shown_limit(pair, true), 4096);
        assert_eq!(line_ratio(Kind::Pins, Kind::Pins.fallback(), 5), 0.5);
        let ratio = line_ratio(Kind::Caption, pair, 1024);
        assert!((ratio - 0.25).abs() < 1e-6);
        assert_eq!(line_ratio(Kind::Caption, pair, 100_000), 1.0);
        assert_eq!(
            pair_text(
                Kind::Pins,
                LimitPair {
                    default_value: 5,
                    premium_value: 10
                }
            ),
            "5 \u{2192} 10 chats"
        );
    }

    #[test]
    fn upload_sizes_follow_the_two_limits() {
        use super::{FileSizeCheck as C, UPLOAD_MAX_FREE, UPLOAD_MAX_PREMIUM, check_file_size};
        assert_eq!(check_file_size(UPLOAD_MAX_FREE, false), C::Fits);
        assert_eq!(check_file_size(UPLOAD_MAX_FREE + 1, false), C::NeedsPremium);
        assert_eq!(check_file_size(UPLOAD_MAX_FREE + 1, true), C::Fits);
        assert_eq!(check_file_size(UPLOAD_MAX_PREMIUM + 1, true), C::TooLarge);
        assert_eq!(check_file_size(UPLOAD_MAX_PREMIUM + 1, false), C::TooLarge);
    }

    #[test]
    fn full_saved_lists_are_detected() {
        assert!(saved_list_full(200, 200));
        assert!(!saved_list_full(199, 200));
        assert!(!saved_list_full(5, 0));
    }

    #[test]
    fn saved_pins_use_the_pins_sentence_and_toast_kinds_have_no_box() {
        let free = limit_box_text(Kind::SavedPins, 5, 100, false, true).expect("a box");
        assert_eq!(free.title, "Limit Reached");
        assert!(free.message.contains("more than 5 chats"));
        assert!(free.message.contains("double the limit to 100 chats"));
        let premium = limit_box_text(Kind::SavedPins, 100, 100, true, true).expect("a box");
        assert_eq!(
            premium.message,
            "Sorry, you can't pin more than 100 chats to the top."
        );
        assert!(!premium.offer_upgrade);
        for kind in Kind::ALL {
            assert_eq!(
                limit_box_text(kind, 1, 2, false, true).is_none(),
                kind.shows_toast(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn new_kinds_send_tdesktops_premium_refs() {
        assert_eq!(
            Kind::SavedPins.premium_ref(),
            "double_limits__saved_dialog_pinned"
        );
        assert_eq!(
            Kind::FavoriteStickers.premium_ref(),
            "double_limits__stickers_faved"
        );
        assert_eq!(Kind::SimilarChats.premium_ref(), "similar_channels");
        assert_eq!(Kind::OwnedBots.premium_ref(), "managed_bots");
    }

    #[test]
    fn story_boxes_name_the_period_and_the_premium_value() {
        let weekly = limit_box_text(Kind::WeeklyStories, 7, 700, false, true).expect("a box");
        assert_eq!(
            weekly.message,
            "Sorry, you can't post more than 7 stories a week. Subscribe to Telegram Premium \
             to raise the limit to 700 stories a week."
        );
        assert!(weekly.offer_upgrade);
        let monthly = limit_box_text(Kind::MonthlyStories, 3000, 3000, true, true).expect("a box");
        assert_eq!(
            monthly.message,
            "Sorry, you can't post more than 3000 stories a month."
        );
        let active = limit_box_text(Kind::ActiveStories, 1, 100, false, false).expect("a box");
        assert_eq!(
            active.message,
            "Sorry, you can't have more than 1 active story at once. Delete one or wait for \
             the oldest to expire."
        );
        assert!(!active.offer_upgrade);
        assert!(!Kind::WeeklyStories.on_doubled_page());
        assert_eq!(
            Kind::from_td_limit_type("premiumLimitTypeMonthlyPostedStoryCount"),
            Some(Kind::MonthlyStories)
        );
    }

    #[test]
    fn the_store_folds_answers_options_and_asks_once() {
        let mut store = LimitStore::default();
        assert!(store.should_ask(Kind::SavedPins));
        assert!(!store.should_ask(Kind::FileSize));
        store.note_asked(Kind::SavedPins);
        assert!(!store.should_ask(Kind::SavedPins));
        store.forget_asked(Kind::SavedPins);
        assert!(store.should_ask(Kind::SavedPins));
        store.apply_premium_limit("premiumLimitTypePinnedSavedMessagesTopicCount", 6, 120);
        store.apply_premium_limit("premiumLimitTypeBioLength", 70, 140);
        assert_eq!(
            store.answered(Kind::SavedPins),
            Some(LimitPair {
                default_value: 6,
                premium_value: 120
            })
        );
        assert!(!store.should_ask(Kind::SavedPins));
        store.apply_option("favorite_stickers_limit", 7);
        store.apply_option("pinned_chat_count_max", 9);
        assert_eq!(store.option(Kind::FavoriteStickers), Some(7));
        assert_eq!(store.option(Kind::Pins), None);
        store.apply_option("favorite_stickers_limit", 0);
        assert_eq!(store.option(Kind::FavoriteStickers), None);
    }

    #[test]
    fn the_bots_toast_offers_premium_only_where_it_helps() {
        assert_eq!(
            bots_limit_notice(20, 40, false, true),
            BotsLimitNotice::Promo {
                count: 20,
                premium_count: 40
            }
        );
        assert_eq!(
            bots_limit_notice(40, 40, true, true),
            BotsLimitNotice::Final { count: 40 }
        );
        assert_eq!(
            bots_limit_notice(20, 40, false, false),
            BotsLimitNotice::Final { count: 20 }
        );
    }
}

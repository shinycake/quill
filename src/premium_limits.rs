//! Premium limit boxes outside folders: pinned chats, captions, saved GIFs,
//! joined communities, public links and file size. Pure logic, no GPUI and
//! no TDLib I/O.
//!
//! tdesktop reference: `boxes/premium_limits_box.cpp` (`SimpleLimitBox`,
//! `SimplePinsLimitBox`, `ChannelsLimitBox`, `PublicLinksLimitBox`,
//! `CaptionLimitBox`, `CaptionLimitReachedBox`, `FileSizeLimitBox`),
//! `data/data_premium_limits.cpp` (the fallbacks used before the server
//! answers) and `Resources/langs/lang.strings` (`lng_*_limit*`). Folder
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
}

/// The free and the Premium value of one limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitPair {
    pub default_value: i64,
    pub premium_value: i64,
}

impl PremiumLimitKind {
    /// The `/start` ref of the box's Increase Limit (tdesktop
    /// `LimitsPremiumRef`).
    pub fn premium_ref(self) -> String {
        let addition = match self {
            Self::Pins => "dialog_pinned",
            Self::PinsArchive => "dialogs_folder_pinned",
            Self::Caption => "caption_length",
            Self::SavedGifs => "saved_gifs",
            Self::Channels => "channels",
            Self::PublicLinks => "channels_public",
            Self::FileSize => "upload_max_fileparts",
        };
        format!("double_limits__{addition}")
    }

    /// Every kind, in the order of the "Doubled limits" page.
    pub const ALL: [Self; 7] = [
        Self::Channels,
        Self::Pins,
        Self::PinsArchive,
        Self::PublicLinks,
        Self::SavedGifs,
        Self::Caption,
        Self::FileSize,
    ];

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
            Self::FileSize => None,
        }
    }

    /// The reverse of [`Self::td_limit_type`].
    pub fn from_td_limit_type(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.td_limit_type() == Some(name))
    }

    /// Telegram's published free and Premium values, used until
    /// `getPremiumFeatures` answers (tdesktop `data_premium_limits.cpp`).
    pub fn fallback(self) -> LimitPair {
        let (default_value, premium_value) = match self {
            Self::Pins => (5, 10),
            Self::PinsArchive => (100, 200),
            Self::Caption => (1024, 4096),
            Self::SavedGifs => (200, 400),
            Self::Channels => (500, 1000),
            Self::PublicLinks => (10, 20),
            Self::FileSize => (2, 4),
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
/// gets the first sentence only and an OK button.
pub fn limit_box_text(
    kind: PremiumLimitKind,
    shown: i64,
    premium: i64,
    is_premium: bool,
    premium_possible: bool,
) -> LimitBoxText {
    let upsell = !is_premium && premium_possible;
    let join = |first: String, second: String| {
        if upsell {
            format!("{first} {second}")
        } else {
            first
        }
    };
    let (title, message) = match kind {
        PremiumLimitKind::Pins | PremiumLimitKind::PinsArchive => (
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
        PremiumLimitKind::SavedGifs => (
            format!("Limit of {} Reached", plural(shown, "GIF", "GIFs")),
            join(
                "An older GIF was replaced with this one.".to_string(),
                format!(
                    "You can increase the limit to {}.",
                    plural(premium, "GIF", "GIFs")
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
    };
    LimitBoxText {
        title,
        message,
        offer_upgrade: upsell,
    }
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

/// The saved GIF list was already at its limit when a GIF was added (TDLib
/// replaces the oldest one silently, so the box is the only notice).
pub fn gif_limit_reached(saved_count: usize, limit: i64) -> bool {
    limit > 0 && saved_count as i64 >= limit
}

#[cfg(test)]
mod tests {
    use super::{
        LimitPair, PremiumLimitKind as Kind, gif_limit_reached, kind_for_message, limit_box_text,
        line_ratio, pair_text, shown_limit,
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
    }

    #[test]
    fn free_accounts_get_the_upsell_sentence() {
        let free = limit_box_text(Kind::Pins, 5, 10, false, true);
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
        let premium = limit_box_text(Kind::Caption, 4096, 4096, true, true);
        assert!(!premium.offer_upgrade);
        assert!(!premium.message.contains("Premium"));
        let none = limit_box_text(Kind::Channels, 500, 1000, false, false);
        assert!(!none.offer_upgrade);
        assert!(none.message.ends_with("before joining a new one."));
        assert!(!none.message.contains("Premium"));
        let links = limit_box_text(Kind::PublicLinks, 10, 20, true, true);
        assert!(links.message.ends_with("an older group or channel."));
    }

    #[test]
    fn counts_use_the_singular() {
        let text = limit_box_text(Kind::Pins, 1, 2, false, true);
        assert!(text.message.contains("1 chat to the top"));
        assert_eq!(
            limit_box_text(Kind::SavedGifs, 200, 400, false, true).title,
            "Limit of 200 GIFs Reached"
        );
        assert_eq!(
            limit_box_text(Kind::FileSize, 2, 4, false, true).message,
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
    fn full_gif_list_is_detected() {
        assert!(gif_limit_reached(200, 200));
        assert!(!gif_limit_reached(199, 200));
        assert!(!gif_limit_reached(5, 0));
    }
}

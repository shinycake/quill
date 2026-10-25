//! Premium and Business features, keyed by TDLib's `premiumFeature*` and
//! `businessFeature*` constructors.
//!
//! tdesktop names them `PremiumFeature` (`boxes/premium_preview_box.h`) and
//! opens the preview of one with `ShowPremiumPreviewBox(feature)`. Quill
//! keys every upsell and every preview page by these types instead of by
//! a row title.

use crate::lang::{Key, keys};

/// One `premiumFeature*` constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PremiumFeature {
    IncreasedLimits,
    IncreasedUploadFileSize,
    ImprovedDownloadSpeed,
    VoiceRecognition,
    DisabledAds,
    UniqueReactions,
    UniqueStickers,
    CustomEmoji,
    AdvancedChatManagement,
    ProfileBadge,
    EmojiStatus,
    AnimatedProfilePhoto,
    ForumTopicIcon,
    AppIcons,
    RealTimeChatTranslation,
    UpgradedStories,
    ChatBoost,
    AccentColor,
    BackgroundForBoth,
    SavedMessagesTags,
    MessagePrivacy,
    LastSeenTimes,
    Business,
    MessageEffects,
    Checklists,
    PaidMessages,
    ProtectPrivateChatContent,
    TextComposition,
    RichMessages,
}

/// What Quill knows about one Premium feature.
struct PremiumEntry {
    td_type: &'static str,
    title: &'static str,
    about: &'static str,
    /// tdesktop `EntryMap` key (`LookupPremiumRef`), the `/start` ref of
    /// the subscribe button.
    start_ref: Option<&'static str>,
}

const fn entry(
    td_type: &'static str,
    title: &'static str,
    about: &'static str,
    start_ref: Option<&'static str>,
) -> PremiumEntry {
    PremiumEntry {
        td_type,
        title,
        about,
        start_ref,
    }
}

impl PremiumFeature {
    /// Every feature, in tdesktop's explainer order.
    pub const ALL: [Self; 29] = [
        Self::IncreasedLimits,
        Self::IncreasedUploadFileSize,
        Self::ImprovedDownloadSpeed,
        Self::VoiceRecognition,
        Self::DisabledAds,
        Self::UniqueReactions,
        Self::UniqueStickers,
        Self::CustomEmoji,
        Self::AdvancedChatManagement,
        Self::ProfileBadge,
        Self::EmojiStatus,
        Self::AnimatedProfilePhoto,
        Self::ForumTopicIcon,
        Self::AppIcons,
        Self::RealTimeChatTranslation,
        Self::UpgradedStories,
        Self::ChatBoost,
        Self::AccentColor,
        Self::BackgroundForBoth,
        Self::SavedMessagesTags,
        Self::MessagePrivacy,
        Self::LastSeenTimes,
        Self::Business,
        Self::MessageEffects,
        Self::Checklists,
        Self::PaidMessages,
        Self::ProtectPrivateChatContent,
        Self::TextComposition,
        Self::RichMessages,
    ];

    fn entry(self) -> PremiumEntry {
        match self {
            Self::IncreasedLimits => entry(
                "premiumFeatureIncreasedLimits",
                "Doubled limits",
                "Up to 1000 channels, 20 folders, 10 pins, 20 public links and more.",
                Some("double_limits"),
            ),
            Self::IncreasedUploadFileSize => entry(
                "premiumFeatureIncreasedUploadFileSize",
                "Files up to 4 GB",
                "Upload documents and media of up to 4 GB each.",
                Some("more_upload"),
            ),
            Self::ImprovedDownloadSpeed => entry(
                "premiumFeatureImprovedDownloadSpeed",
                "Faster download speed",
                "No speed limits when downloading media and documents.",
                Some("faster_download"),
            ),
            Self::VoiceRecognition => entry(
                "premiumFeatureVoiceRecognition",
                "Voice-to-text conversion",
                "Read the transcript of any incoming voice message.",
                Some("voice_to_text"),
            ),
            Self::DisabledAds => entry(
                "premiumFeatureDisabledAds",
                "No ads",
                "Sponsored messages in public channels are no longer shown.",
                Some("no_ads"),
            ),
            Self::UniqueReactions => entry(
                "premiumFeatureUniqueReactions",
                "Unique reactions",
                "Add more reactions to messages, including exclusive ones.",
                Some("infinite_reactions"),
            ),
            Self::UniqueStickers => entry(
                "premiumFeatureUniqueStickers",
                "Premium stickers",
                "Exclusive stickers with fullscreen effects.",
                Some("premium_stickers"),
            ),
            Self::CustomEmoji => entry(
                "premiumFeatureCustomEmoji",
                "Animated emoji",
                "Use custom emoji from any pack in your messages.",
                Some("animated_emoji"),
            ),
            Self::AdvancedChatManagement => entry(
                "premiumFeatureAdvancedChatManagement",
                "Advanced chat management",
                "Tools to organize your chats: auto-archive and more.",
                Some("advanced_chat_management"),
            ),
            Self::ProfileBadge => entry(
                "premiumFeatureProfileBadge",
                "Profile badge",
                "A badge next to your name shows you are subscribed.",
                Some("profile_badge"),
            ),
            Self::EmojiStatus => entry(
                "premiumFeatureEmojiStatus",
                "Emoji status",
                "Choose an emoji status to show next to your name.",
                Some("emoji_status"),
            ),
            Self::AnimatedProfilePhoto => entry(
                "premiumFeatureAnimatedProfilePhoto",
                "Animated profile pictures",
                "Video avatars that play in chat lists and profiles.",
                Some("animated_userpics"),
            ),
            Self::ForumTopicIcon => entry(
                "premiumFeatureForumTopicIcon",
                "Topic icons",
                "Create topics with any custom emoji as the icon.",
                None,
            ),
            Self::AppIcons => entry(
                "premiumFeatureAppIcons",
                "Special app icons",
                "Choose from a selection of Premium app icons.",
                None,
            ),
            Self::RealTimeChatTranslation => entry(
                "premiumFeatureRealTimeChatTranslation",
                "Real-time translation",
                "Translate entire chats in real time.",
                Some("translations"),
            ),
            Self::UpgradedStories => entry(
                "premiumFeatureUpgradedStories",
                "Upgraded stories",
                "Longer stories, more captions, link and permanent options.",
                Some("stories"),
            ),
            Self::ChatBoost => entry(
                "premiumFeatureChatBoost",
                "Boosts for channels",
                "Boost channels to unlock perks for everyone.",
                None,
            ),
            Self::AccentColor => entry(
                "premiumFeatureAccentColor",
                "Name and profile colors",
                "Pick a color and logo for your name, replies and profile.",
                Some("peer_colors"),
            ),
            Self::BackgroundForBoth => entry(
                "premiumFeatureBackgroundForBoth",
                "Wallpapers for both sides",
                "Set a wallpaper for both you and your chat partner.",
                Some("wallpapers"),
            ),
            Self::SavedMessagesTags => entry(
                "premiumFeatureSavedMessagesTags",
                "Tags for Saved Messages",
                "Organize saved messages with tags.",
                Some("saved_tags"),
            ),
            Self::MessagePrivacy => entry(
                "premiumFeatureMessagePrivacy",
                "Message privacy",
                "Choose who can send you messages.",
                Some("message_privacy"),
            ),
            Self::LastSeenTimes => entry(
                "premiumFeatureLastSeenTimes",
                "Last seen times",
                "See the last seen time of people who hid it.",
                Some("last_seen"),
            ),
            Self::Business => entry(
                "premiumFeatureBusiness",
                "Telegram Business",
                "Business hours, quick replies, chatbots and more.",
                Some("business"),
            ),
            Self::MessageEffects => entry(
                "premiumFeatureMessageEffects",
                "Message effects",
                "Add animated effects to private messages.",
                Some("effects"),
            ),
            Self::Checklists => entry(
                "premiumFeatureChecklists",
                "Checklists",
                "Create checklists in chats.",
                Some("todo"),
            ),
            Self::PaidMessages => entry(
                "premiumFeaturePaidMessages",
                "Paid messages",
                "Charge Stars for messages from people outside your contacts.",
                None,
            ),
            Self::ProtectPrivateChatContent => entry(
                "premiumFeatureProtectPrivateChatContent",
                "Protect private chat content",
                "Restrict forwarding and saving of your private chats.",
                Some("no_forwards"),
            ),
            Self::TextComposition => entry(
                "premiumFeatureTextComposition",
                "AI text tools",
                "Rewrite, translate and fix your messages while typing.",
                Some("ai_compose"),
            ),
            Self::RichMessages => entry(
                "premiumFeatureRichMessages",
                "Rich messages",
                "Send messages with extended formatting.",
                Some("rich_formatting"),
            ),
        }
    }

    /// The feature behind a `premiumFeature*` constructor; `None` for one
    /// newer than Quill.
    pub fn from_td(constructor: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|feature| feature.td_type() == constructor)
    }

    pub fn td_type(self) -> &'static str {
        self.entry().td_type
    }

    /// Row and preview title.
    pub fn title(self) -> &'static str {
        self.entry().title
    }

    /// Row and preview description.
    pub fn about(self) -> &'static str {
        self.entry().about
    }

    /// tdesktop `LookupPremiumRef`: the `/start` ref the subscribe button
    /// sends from this feature's preview.
    pub fn start_ref(self) -> Option<&'static str> {
        self.entry().start_ref
    }
}

/// One `businessFeature*` constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BusinessFeature {
    Location,
    OpeningHours,
    QuickReplies,
    GreetingMessage,
    AwayMessage,
    AccountLinks,
    StartPage,
    Bots,
    EmojiStatus,
    ChatFolderTags,
    UpgradedStories,
}

impl BusinessFeature {
    pub const ALL: [Self; 11] = [
        Self::Location,
        Self::OpeningHours,
        Self::QuickReplies,
        Self::GreetingMessage,
        Self::AwayMessage,
        Self::AccountLinks,
        Self::StartPage,
        Self::Bots,
        Self::EmojiStatus,
        Self::ChatFolderTags,
        Self::UpgradedStories,
    ];

    /// tdesktop `settings_business.cpp` `FallbackOrder`: the Business
    /// list before `getBusinessFeatures` answers.
    pub const FALLBACK_ORDER: [Self; 9] = [
        Self::GreetingMessage,
        Self::AwayMessage,
        Self::QuickReplies,
        Self::OpeningHours,
        Self::Location,
        Self::AccountLinks,
        Self::StartPage,
        Self::Bots,
        Self::ChatFolderTags,
    ];

    pub fn td_type(self) -> &'static str {
        match self {
            Self::Location => "businessFeatureLocation",
            Self::OpeningHours => "businessFeatureOpeningHours",
            Self::QuickReplies => "businessFeatureQuickReplies",
            Self::GreetingMessage => "businessFeatureGreetingMessage",
            Self::AwayMessage => "businessFeatureAwayMessage",
            Self::AccountLinks => "businessFeatureAccountLinks",
            Self::StartPage => "businessFeatureStartPage",
            Self::Bots => "businessFeatureBots",
            Self::EmojiStatus => "businessFeatureEmojiStatus",
            Self::ChatFolderTags => "businessFeatureChatFolderTags",
            Self::UpgradedStories => "businessFeatureUpgradedStories",
        }
    }

    pub fn from_td(constructor: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|feature| feature.td_type() == constructor)
    }

    /// Whether tdesktop lists the feature on the Business screen and in
    /// its preview (`BusinessFeaturesOrder` knows nine; the emoji status
    /// and stories entries are Premium rows there).
    pub fn listed(self) -> bool {
        Self::FALLBACK_ORDER.contains(&self)
    }

    /// Title and description (`lng_business_subtitle_*` /
    /// `lng_business_about_*`; folder tags use the Premium summary keys).
    pub fn text_keys(self) -> (Key, Key) {
        match self {
            Self::Location => (
                keys::BUSINESS_SUBTITLE_LOCATION,
                keys::BUSINESS_ABOUT_LOCATION,
            ),
            Self::OpeningHours => (
                keys::BUSINESS_SUBTITLE_OPENING_HOURS,
                keys::BUSINESS_ABOUT_OPENING_HOURS,
            ),
            Self::QuickReplies => (
                keys::BUSINESS_SUBTITLE_QUICK_REPLIES,
                keys::BUSINESS_ABOUT_QUICK_REPLIES,
            ),
            Self::GreetingMessage => (
                keys::BUSINESS_SUBTITLE_GREETING_MESSAGES,
                keys::BUSINESS_ABOUT_GREETING_MESSAGES,
            ),
            Self::AwayMessage => (
                keys::BUSINESS_SUBTITLE_AWAY_MESSAGES,
                keys::BUSINESS_ABOUT_AWAY_MESSAGES,
            ),
            Self::AccountLinks => (
                keys::BUSINESS_SUBTITLE_CHAT_LINKS,
                keys::BUSINESS_ABOUT_CHAT_LINKS,
            ),
            Self::StartPage => (
                keys::BUSINESS_SUBTITLE_CHAT_INTRO,
                keys::BUSINESS_ABOUT_CHAT_INTRO,
            ),
            Self::Bots => (
                keys::BUSINESS_SUBTITLE_CHATBOTS,
                keys::BUSINESS_ABOUT_CHATBOTS,
            ),
            Self::ChatFolderTags | Self::EmojiStatus | Self::UpgradedStories => (
                keys::PREMIUM_SUMMARY_SUBTITLE_FILTER_TAGS,
                keys::PREMIUM_SUMMARY_ABOUT_FILTER_TAGS,
            ),
        }
    }
}

/// `businessFeatures`: the features tdesktop lists, in the server's order.
pub fn parse_business_features(value: &serde_json::Value) -> Vec<BusinessFeature> {
    value
        .get("features")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| BusinessFeature::from_td(row.get("@type")?.as_str()?))
        .filter(|feature| feature.listed())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{BusinessFeature, PremiumFeature, parse_business_features};
    use serde_json::json;

    #[test]
    fn premium_features_round_trip_their_constructor() {
        for feature in PremiumFeature::ALL {
            assert_eq!(PremiumFeature::from_td(feature.td_type()), Some(feature));
        }
        for feature in BusinessFeature::ALL {
            assert_eq!(BusinessFeature::from_td(feature.td_type()), Some(feature));
        }
        assert_eq!(PremiumFeature::from_td("premiumFeatureNewerThanThis"), None);
    }

    #[test]
    fn start_refs_follow_tdesktop() {
        assert_eq!(
            PremiumFeature::IncreasedLimits.start_ref(),
            Some("double_limits")
        );
        assert_eq!(PremiumFeature::MessageEffects.start_ref(), Some("effects"));
        assert_eq!(PremiumFeature::ForumTopicIcon.start_ref(), None);
    }

    #[test]
    fn business_list_keeps_server_order_and_drops_unlisted_rows() {
        let list = parse_business_features(&json!({"features": [
            {"@type": "businessFeatureChatFolderTags"},
            {"@type": "businessFeatureEmojiStatus"},
            {"@type": "businessFeatureQuickReplies"},
            {"@type": "businessFeatureUpgradedStories"},
            {"@type": "businessFeatureNewerThanThis"},
            {"@type": "businessFeatureBots"}
        ]}));
        assert_eq!(
            list,
            [
                BusinessFeature::ChatFolderTags,
                BusinessFeature::QuickReplies,
                BusinessFeature::Bots
            ]
        );
    }
}

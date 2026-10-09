//! Premium, Stars and received gifts — the read-only parity slice.
//!
//! Mirrors Telegram Desktop's `settings_credits*`, `settings_premium*`,
//! `info/peer_gifts/*` and `boxes/gift_premium_box*`: the Stars balance and
//! transaction history, the gifts shown on a profile, and the Premium
//! features explainer. Nothing here buys, sends or transfers anything: the
//! only mutations are `toggleGiftIsSaved` ("show on profile") and
//! `sellGift` ("convert to Stars"), both for the user's own gifts and both
//! behind an explicit confirmation in the UI.

use crate::telegram::envelope::{MessageSender, StickerContent, parse_sticker_value};
use serde_json::Value;

fn int(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn text_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn formatted(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|t| t.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn sender_of(value: Option<&Value>) -> Option<MessageSender> {
    let value = value?;
    match value.get("@type")?.as_str()? {
        "messageSenderUser" => Some(MessageSender::User {
            user_id: int(value.get("user_id")),
        }),
        "messageSenderChat" => Some(MessageSender::Chat {
            chat_id: int(value.get("chat_id")),
        }),
        _ => None,
    }
}

/// Group digits in threes: `1234567` -> `1,234,567`.
pub fn group_digits(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// `starAmount`: whole Stars plus nanostars (1e-9 Star).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StarAmount {
    pub stars: i64,
    pub nanos: i32,
}

impl StarAmount {
    pub fn parse(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        Self {
            stars: int(value.get("star_count")),
            nanos: int(value.get("nanostar_count")) as i32,
        }
    }

    /// Negative when Stars left the account.
    pub fn is_negative(&self) -> bool {
        self.stars < 0 || (self.stars == 0 && self.nanos < 0)
    }

    /// `1,234` or `12.5`; fractional part only when there are nanostars.
    pub fn label(&self) -> String {
        let whole = group_digits(self.stars.abs());
        let sign = if self.is_negative() { "-" } else { "" };
        if self.nanos == 0 {
            return format!("{sign}{whole}");
        }
        let frac = format!("{:09}", self.nanos.unsigned_abs());
        let frac = frac.trim_end_matches('0');
        format!("{sign}{whole}.{frac}")
    }

    /// Signed label for a history row: `+100` / `-50`.
    pub fn signed_label(&self) -> String {
        let label = self.label();
        if self.is_negative() {
            label
        } else {
            format!("+{label}")
        }
    }
}

/// Which side of the history the list shows (tdesktop's All / Incoming /
/// Outgoing tabs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TxFilter {
    #[default]
    All,
    Incoming,
    Outgoing,
}

impl TxFilter {
    pub const ALL: [TxFilter; 3] = [TxFilter::All, TxFilter::Incoming, TxFilter::Outgoing];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All transactions",
            Self::Incoming => "Incoming",
            Self::Outgoing => "Outgoing",
        }
    }

    /// `transactionDirection` argument of `getStarTransactions`; `None` =
    /// both directions (the argument is omitted).
    pub fn direction_type(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Incoming => Some("transactionDirectionIncoming"),
            Self::Outgoing => Some("transactionDirectionOutgoing"),
        }
    }
}

/// One `starTransaction`, reduced to what a history row and the details box
/// show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarTx {
    pub id: String,
    pub amount: StarAmount,
    pub is_refund: bool,
    pub date: i32,
    /// Row headline ("Gift", "Telegram Premium", the product name…).
    pub title: String,
    /// Secondary line ("From Dana", "Via Premium Bot"…); may be empty.
    pub detail: String,
    /// The user or chat on the other side, resolved to a name by the UI.
    pub peer: Option<MessageSender>,
    /// `starTransactionType*` constructor name, for diagnostics and tests.
    pub kind: String,
}

impl StarTx {
    pub fn is_incoming(&self) -> bool {
        !self.amount.is_negative()
    }
}

fn user(id: i64) -> Option<MessageSender> {
    (id != 0).then_some(MessageSender::User { user_id: id })
}

fn chat(id: i64) -> Option<MessageSender> {
    (id != 0).then_some(MessageSender::Chat { chat_id: id })
}

fn months(count: i64) -> String {
    match count {
        1 => "1 month".to_string(),
        n if n > 0 && n % 12 == 0 => {
            let years = n / 12;
            if years == 1 {
                "1 year".to_string()
            } else {
                format!("{years} years")
            }
        }
        n => format!("{n} months"),
    }
}

fn period_label(seconds: i64) -> String {
    match seconds {
        2_592_000 => "monthly".to_string(),
        s if s > 0 && s % 86_400 == 0 => format!("every {} days", s / 86_400),
        _ => String::new(),
    }
}

fn product_title(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(|info| text_of(info, "title"))
        .unwrap_or_default()
}

fn gift_name(value: &Value) -> String {
    let Some(gift) = value.get("gift") else {
        return String::new();
    };
    if gift.get("@type").and_then(Value::as_str) == Some("upgradedGift") {
        let title = text_of(gift, "title");
        let number = int(gift.get("number"));
        return if number > 0 {
            format!("{title} #{}", group_digits(number))
        } else {
            title
        };
    }
    let stars = int(gift.get("star_count"));
    if stars > 0 {
        format!("{} Stars", group_digits(stars))
    } else {
        String::new()
    }
}

/// Map a `StarTransactionType` to (title, detail, peer), following the
/// wording of Telegram Desktop's credits history rows.
pub fn describe_tx_type(ty: &Value) -> (String, String, Option<MessageSender>) {
    let kind = ty.get("@type").and_then(Value::as_str).unwrap_or("");
    let s = |t: &str, d: &str, p: Option<MessageSender>| (t.to_string(), d.to_string(), p);
    match kind {
        "starTransactionTypePremiumBotDeposit" => s("Stars top-up", "Via Premium Bot", None),
        "starTransactionTypeAppStoreDeposit" => s("Stars top-up", "Via App Store", None),
        "starTransactionTypeGooglePlayDeposit" => s("Stars top-up", "Via Google Play", None),
        "starTransactionTypeFragmentDeposit" => s("Stars top-up", "Via Fragment", None),
        "starTransactionTypeUserDeposit" => s("Gifted Stars", "", user(int(ty.get("user_id")))),
        "starTransactionTypeGiveawayDeposit" => {
            s("Giveaway prize", "", chat(int(ty.get("chat_id"))))
        }
        "starTransactionTypeFragmentWithdrawal" => s("Withdrawal", "Via Fragment", None),
        "starTransactionTypeTelegramAdsWithdrawal" => s("Withdrawal", "To Telegram Ads", None),
        "starTransactionTypeTelegramApiUsage" => (
            "Paid API usage".to_string(),
            format!("{} requests", group_digits(int(ty.get("request_count")))),
            None,
        ),
        "starTransactionTypeBotPaidMediaPurchase" => {
            s("Paid media", "", user(int(ty.get("user_id"))))
        }
        "starTransactionTypeBotPaidMediaSale" => {
            s("Paid media sale", "", user(int(ty.get("user_id"))))
        }
        "starTransactionTypeChannelPaidMediaPurchase" => {
            s("Paid media", "", chat(int(ty.get("chat_id"))))
        }
        "starTransactionTypeChannelPaidMediaSale" => {
            s("Paid media sale", "", user(int(ty.get("user_id"))))
        }
        "starTransactionTypeBotInvoicePurchase" => {
            let title = product_title(ty, "product_info");
            (
                if title.is_empty() {
                    "Purchase".into()
                } else {
                    title
                },
                String::new(),
                user(int(ty.get("user_id"))),
            )
        }
        "starTransactionTypeBotInvoiceSale" => {
            let title = product_title(ty, "product_info");
            (
                if title.is_empty() {
                    "Sale".into()
                } else {
                    title
                },
                "Sale".into(),
                user(int(ty.get("user_id"))),
            )
        }
        "starTransactionTypeBotSubscriptionPurchase" => (
            {
                let title = product_title(ty, "product_info");
                if title.is_empty() {
                    "Subscription".into()
                } else {
                    title
                }
            },
            period_label(int(ty.get("subscription_period"))),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeBotSubscriptionSale" => (
            "Subscription sale".into(),
            period_label(int(ty.get("subscription_period"))),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeChannelSubscriptionPurchase" => (
            "Channel subscription".into(),
            period_label(int(ty.get("subscription_period"))),
            chat(int(ty.get("chat_id"))),
        ),
        "starTransactionTypeChannelSubscriptionSale" => (
            "Subscription sale".into(),
            period_label(int(ty.get("subscription_period"))),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeGiftAuctionBid" => (
            "Gift auction bid".into(),
            gift_name(ty),
            sender_of(ty.get("owner_id")),
        ),
        "starTransactionTypeGiftPurchase" => {
            ("Gift".into(), gift_name(ty), sender_of(ty.get("owner_id")))
        }
        "starTransactionTypeGiftPurchaseOffer" => ("Gift offer".into(), gift_name(ty), None),
        "starTransactionTypeGiftTransfer" => (
            "Gift transfer".into(),
            gift_name(ty),
            sender_of(ty.get("owner_id")),
        ),
        "starTransactionTypeGiftOriginalDetailsDrop" => (
            "Original details removed".into(),
            gift_name(ty),
            sender_of(ty.get("owner_id")),
        ),
        "starTransactionTypeGiftSale" => (
            "Gift converted".into(),
            gift_name(ty),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeGiftUpgrade" => (
            "Gift upgrade".into(),
            gift_name(ty),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeGiftUpgradePurchase" => (
            "Gift upgrade".into(),
            gift_name(ty),
            sender_of(ty.get("owner_id")),
        ),
        "starTransactionTypeUpgradedGiftPurchase" => (
            "Collectible gift".into(),
            gift_name(ty),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeUpgradedGiftSale" => (
            "Collectible gift sale".into(),
            gift_name(ty),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeChannelPaidReactionSend" => {
            s("Reaction", "", chat(int(ty.get("chat_id"))))
        }
        "starTransactionTypeChannelPaidReactionReceive" => {
            s("Reaction received", "", user(int(ty.get("user_id"))))
        }
        "starTransactionTypeAffiliateProgramCommission" => (
            "Affiliate commission".into(),
            String::new(),
            chat(int(ty.get("chat_id"))),
        ),
        "starTransactionTypePaidMessageSend" => (
            "Paid message".into(),
            format!("{} messages", int(ty.get("message_count")).max(1)),
            chat(int(ty.get("chat_id"))),
        ),
        "starTransactionTypePaidMessageReceive" => (
            "Paid messages received".into(),
            format!("{} messages", int(ty.get("message_count")).max(1)),
            sender_of(ty.get("sender_id")),
        ),
        "starTransactionTypePaidGroupCallMessageSend"
        | "starTransactionTypePaidGroupCallReactionSend" => (
            "Live stream message".into(),
            String::new(),
            chat(int(ty.get("chat_id"))),
        ),
        "starTransactionTypePaidGroupCallMessageReceive"
        | "starTransactionTypePaidGroupCallReactionReceive" => (
            "Live stream message received".into(),
            String::new(),
            sender_of(ty.get("sender_id")),
        ),
        "starTransactionTypeSuggestedPostPaymentSend" => {
            s("Suggested post", "", chat(int(ty.get("chat_id"))))
        }
        "starTransactionTypeSuggestedPostPaymentReceive" => {
            s("Suggested post", "", user(int(ty.get("user_id"))))
        }
        "starTransactionTypePremiumPurchase" => (
            "Telegram Premium".into(),
            months(int(ty.get("month_count"))),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypeBusinessBotTransferSend" => {
            s("Business bot transfer", "", user(int(ty.get("user_id"))))
        }
        "starTransactionTypeBusinessBotTransferReceive" => (
            "Business bot transfer received".into(),
            String::new(),
            user(int(ty.get("user_id"))),
        ),
        "starTransactionTypePublicPostSearch" => s("Public post search", "", None),
        _ => s("Transaction", "", None),
    }
}

pub fn parse_star_tx(value: &Value) -> Option<StarTx> {
    let ty = value.get("type")?;
    let (title, detail, peer) = describe_tx_type(ty);
    Some(StarTx {
        id: text_of(value, "id"),
        amount: StarAmount::parse(value.get("star_amount")),
        is_refund: flag(value, "is_refund"),
        date: int(value.get("date")) as i32,
        title,
        detail,
        peer,
        kind: text_of(ty, "@type"),
    })
}

/// `starTransactions` — one page of the history.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StarTxPage {
    pub balance: StarAmount,
    pub transactions: Vec<StarTx>,
    pub next_offset: String,
}

pub fn parse_star_transactions(value: &Value) -> StarTxPage {
    StarTxPage {
        balance: StarAmount::parse(value.get("star_amount")),
        transactions: value
            .get("transactions")
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter_map(parse_star_tx).collect())
            .unwrap_or_default(),
        next_offset: text_of(value, "next_offset"),
    }
}

/// What a received gift looks like in the grid and the details box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftInfo {
    /// "Plush Pepe #123" for a collectible, empty for a regular gift.
    pub title: String,
    pub upgraded: bool,
    /// Star price of a regular gift (0 for collectibles).
    pub star_count: i64,
    /// Collectible number, 0 for a regular gift.
    pub number: i32,
    /// Backdrop of a collectible as 0xRRGGBB (center, edge).
    pub backdrop: Option<(u32, u32)>,
    pub sticker: Option<StickerContent>,
    /// Files of the sticker, for the downloader.
    pub files: Vec<crate::telegram::envelope::ParsedFile>,
}

fn sticker_content(
    value: Option<&Value>,
) -> (
    Option<StickerContent>,
    Vec<crate::telegram::envelope::ParsedFile>,
) {
    let (item, mut files) = parse_sticker_value(value);
    files.retain(|file| file.id.0 != 0);
    let sticker = item.map(|item| StickerContent {
        emoji: item.emoji,
        width: item.width,
        height: item.height,
        format: item.format,
        file_id: item.file_id,
        thumb_file_id: item.thumb_file_id,
        thumb_width: item.thumb_width,
        thumb_height: item.thumb_height,
        is_premium: false,
        requires_premium: false,
        set_id: item.set_id,
    });
    (sticker, files)
}

/// Parse the sticker JSON of a gift (public so chat cards reuse it).
pub fn parse_gift_sticker(
    value: Option<&Value>,
) -> (
    Option<StickerContent>,
    Vec<crate::telegram::envelope::ParsedFile>,
) {
    sticker_content(value)
}

fn rgb(value: &Value, key: &str) -> u32 {
    (int(value.get(key)) as u32) & 0x00FF_FFFF
}

/// Parse a `gift` / `upgradedGift` JSON (the `SentGift` payload).
pub fn parse_gift_info(gift: &Value) -> GiftInfo {
    if gift.get("@type").and_then(Value::as_str) == Some("upgradedGift") {
        let number = int(gift.get("number")) as i32;
        let title = text_of(gift, "title");
        let (sticker, files) = sticker_content(gift.get("model").and_then(|m| m.get("sticker")));
        let backdrop = gift
            .get("backdrop")
            .and_then(|b| b.get("colors"))
            .map(|c| (rgb(c, "center_color"), rgb(c, "edge_color")));
        return GiftInfo {
            title: if number > 0 {
                format!("{title} #{}", group_digits(number as i64))
            } else {
                title
            },
            upgraded: true,
            star_count: 0,
            number,
            backdrop,
            sticker,
            files,
        };
    }
    let (sticker, files) = sticker_content(gift.get("sticker"));
    GiftInfo {
        title: String::new(),
        upgraded: false,
        star_count: int(gift.get("star_count")),
        number: 0,
        backdrop: None,
        sticker,
        files,
    }
}

/// One `receivedGift`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedGift {
    pub id: String,
    pub sender: Option<MessageSender>,
    pub text: String,
    pub is_private: bool,
    pub is_saved: bool,
    pub is_pinned: bool,
    pub can_be_upgraded: bool,
    pub was_refunded: bool,
    pub date: i32,
    pub gift: GiftInfo,
    /// Stars the gift converts to (`sellGift`); 0 = not convertible.
    pub sell_star_count: i64,
}

impl ReceivedGift {
    /// "Convert to Stars" is offered for a regular gift with a conversion
    /// price that has not been refunded.
    pub fn can_convert(&self) -> bool {
        !self.gift.upgraded && self.sell_star_count > 0 && !self.was_refunded
    }

    /// Row/card heading: collectible name, or the star price.
    pub fn headline(&self) -> String {
        if self.gift.upgraded {
            self.gift.title.clone()
        } else {
            format!("{} Stars", group_digits(self.gift.star_count))
        }
    }
}

pub fn parse_received_gift(value: &Value) -> Option<ReceivedGift> {
    let id = text_of(value, "received_gift_id");
    if id.is_empty() {
        return None;
    }
    Some(ReceivedGift {
        id,
        sender: sender_of(value.get("sender_id")),
        text: formatted(value, "text"),
        is_private: flag(value, "is_private"),
        is_saved: flag(value, "is_saved"),
        is_pinned: flag(value, "is_pinned"),
        can_be_upgraded: flag(value, "can_be_upgraded"),
        was_refunded: flag(value, "was_refunded"),
        date: int(value.get("date")) as i32,
        gift: parse_gift_info(value.get("gift")?.get("gift")?),
        sell_star_count: int(value.get("sell_star_count")),
    })
}

/// `receivedGifts` — one page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GiftsPage {
    pub total_count: i32,
    pub gifts: Vec<ReceivedGift>,
    pub next_offset: String,
}

pub fn parse_received_gifts(value: &Value) -> GiftsPage {
    GiftsPage {
        total_count: int(value.get("total_count")) as i32,
        gifts: value
            .get("gifts")
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter_map(parse_received_gift).collect())
            .unwrap_or_default(),
        next_offset: text_of(value, "next_offset"),
    }
}

/// One Premium feature row (tdesktop `settings_premium.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PremiumFeatureRow {
    pub title: &'static str,
    pub about: &'static str,
}

/// Map a `premiumFeature*` constructor to its explainer row, in the order
/// tdesktop lists them. Unknown (newer) features are skipped.
pub fn feature_row(constructor: &str) -> Option<PremiumFeatureRow> {
    let (title, about) = match constructor {
        "premiumFeatureIncreasedLimits" => (
            "Doubled limits",
            "Up to 1000 channels, 20 folders, 10 pins, 20 public links and more.",
        ),
        "premiumFeatureIncreasedUploadFileSize" => (
            "Files up to 4 GB",
            "Upload documents and media of up to 4 GB each.",
        ),
        "premiumFeatureImprovedDownloadSpeed" => (
            "Faster download speed",
            "No speed limits when downloading media and documents.",
        ),
        "premiumFeatureVoiceRecognition" => (
            "Voice-to-text conversion",
            "Read the transcript of any incoming voice message.",
        ),
        "premiumFeatureDisabledAds" => (
            "No ads",
            "Sponsored messages in public channels are no longer shown.",
        ),
        "premiumFeatureUniqueReactions" => (
            "Unique reactions",
            "Add more reactions to messages, including exclusive ones.",
        ),
        "premiumFeatureUniqueStickers" => (
            "Premium stickers",
            "Exclusive stickers with fullscreen effects.",
        ),
        "premiumFeatureCustomEmoji" => (
            "Animated emoji",
            "Use custom emoji from any pack in your messages.",
        ),
        "premiumFeatureAdvancedChatManagement" => (
            "Advanced chat management",
            "Tools to organize your chats: auto-archive and more.",
        ),
        "premiumFeatureProfileBadge" => (
            "Profile badge",
            "A badge next to your name shows you are subscribed.",
        ),
        "premiumFeatureEmojiStatus" => (
            "Emoji status",
            "Choose an emoji status to show next to your name.",
        ),
        "premiumFeatureAnimatedProfilePhoto" => (
            "Animated profile pictures",
            "Video avatars that play in chat lists and profiles.",
        ),
        "premiumFeatureForumTopicIcon" => (
            "Topic icons",
            "Create topics with any custom emoji as the icon.",
        ),
        "premiumFeatureAppIcons" => (
            "Special app icons",
            "Choose from a selection of Premium app icons.",
        ),
        "premiumFeatureRealTimeChatTranslation" => (
            "Real-time translation",
            "Translate entire chats in real time.",
        ),
        "premiumFeatureUpgradedStories" => (
            "Upgraded stories",
            "Longer stories, more captions, link and permanent options.",
        ),
        "premiumFeatureChatBoost" => (
            "Boosts for channels",
            "Boost channels to unlock perks for everyone.",
        ),
        "premiumFeatureAccentColor" => (
            "Name and profile colors",
            "Pick a color and logo for your name, replies and profile.",
        ),
        "premiumFeatureBackgroundForBoth" => (
            "Wallpapers for both sides",
            "Set a wallpaper for both you and your chat partner.",
        ),
        "premiumFeatureSavedMessagesTags" => (
            "Tags for Saved Messages",
            "Organize saved messages with tags.",
        ),
        "premiumFeatureMessagePrivacy" => ("Message privacy", "Choose who can send you messages."),
        "premiumFeatureLastSeenTimes" => (
            "Last seen times",
            "See the last seen time of people who hid it.",
        ),
        "premiumFeatureBusiness" => (
            "Telegram Business",
            "Business hours, quick replies, chatbots and more.",
        ),
        "premiumFeatureMessageEffects" => (
            "Message effects",
            "Add animated effects to private messages.",
        ),
        "premiumFeatureChecklists" => ("Checklists", "Create checklists in chats."),
        "premiumFeaturePaidMessages" => (
            "Paid messages",
            "Charge Stars for messages from people outside your contacts.",
        ),
        "premiumFeatureProtectPrivateChatContent" => (
            "Protect private chat content",
            "Restrict forwarding and saving of your private chats.",
        ),
        "premiumFeatureTextComposition" => (
            "AI text tools",
            "Rewrite, translate and fix your messages while typing.",
        ),
        "premiumFeatureRichMessages" => {
            ("Rich messages", "Send messages with extended formatting.")
        }
        _ => return None,
    };
    Some(PremiumFeatureRow { title, about })
}

/// One `premiumLimit`, shown as "default → Premium".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumLimitRow {
    pub title: &'static str,
    pub default_value: i64,
    pub premium_value: i64,
}

fn limit_title(constructor: &str) -> Option<&'static str> {
    Some(match constructor {
        "premiumLimitTypeSupergroupCount" => "Groups and channels",
        "premiumLimitTypePinnedChatCount" => "Pinned chats",
        "premiumLimitTypeCreatedPublicChatCount" => "Public links",
        "premiumLimitTypeSavedAnimationCount" => "Saved GIFs",
        "premiumLimitTypeFavoriteStickerCount" => "Favorite stickers",
        "premiumLimitTypeChatFolderCount" => "Folders",
        "premiumLimitTypeChatFolderChosenChatCount" => "Chats per folder",
        "premiumLimitTypePinnedArchivedChatCount" => "Pins in the archive",
        "premiumLimitTypeMessageTextLength" => "Message length",
        "premiumLimitTypeCaptionLength" => "Caption length",
        "premiumLimitTypeBioLength" => "Bio length",
        "premiumLimitTypeChatFolderInviteLinkCount" => "Folder invite links",
        "premiumLimitTypeShareableChatFolderCount" => "Shareable folders",
        "premiumLimitTypeActiveStoryCount" => "Active stories",
        _ => return None,
    })
}

/// `premiumFeatures`: the feature rows and limits Telegram reports, plus the
/// Premium Bot link the "Get Premium" button opens.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PremiumInfo {
    pub features: Vec<PremiumFeatureRow>,
    pub limits: Vec<PremiumLimitRow>,
}

pub fn parse_premium_features(value: &Value) -> PremiumInfo {
    let features = value
        .get("features")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|f| feature_row(f.get("@type")?.as_str()?))
                .collect()
        })
        .unwrap_or_default();
    let limits = value
        .get("limits")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|l| {
                    Some(PremiumLimitRow {
                        title: limit_title(l.get("type")?.get("@type")?.as_str()?)?,
                        default_value: int(l.get("default_value")),
                        premium_value: int(l.get("premium_value")),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    PremiumInfo { features, limits }
}

/// `premiumState`: the localized state text and whether a subscription is
/// current (any payment option flagged `is_current`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PremiumStateInfo {
    pub text: String,
    pub is_subscribed: bool,
}

pub fn parse_premium_state(value: &Value) -> PremiumStateInfo {
    PremiumStateInfo {
        text: formatted(value, "state"),
        is_subscribed: value
            .get("payment_options")
            .and_then(Value::as_array)
            .is_some_and(|rows| rows.iter().any(|o| flag(o, "is_current"))),
    }
}

/// Premium Bot link the "Get Premium" button opens (as the folder-limit
/// box does). Subscribing happens outside Quill.
pub const PREMIUM_BOT_URL: &str = "https://t.me/PremiumBot";

/// Per-surface state of the hub dialogs. Lives in `Session` so the poll loop
/// and reducer own every transition (the Subscriptions-dialog pattern).
#[derive(Debug, Clone, Default)]
pub struct PremiumHub {
    // Stars.
    pub stars_open: bool,
    pub balance: Option<StarAmount>,
    pub filter: TxFilter,
    pub transactions: Vec<StarTx>,
    pub tx_offset: String,
    pub tx_loaded: bool,
    pub tx_loading: bool,
    pub tx_error: Option<String>,
    /// `@extra` of the newest transactions request; older answers (a
    /// filter switched mid-flight) are dropped.
    pub tx_request: u64,
    /// Transaction whose details box is showing.
    pub tx_selected: Option<String>,
    // Gifts.
    pub gifts_open: bool,
    pub gifts_owner: Option<MessageSender>,
    pub gifts: Vec<ReceivedGift>,
    pub gifts_total: i32,
    pub gifts_offset: String,
    pub gifts_loaded: bool,
    pub gifts_loading: bool,
    pub gifts_error: Option<String>,
    /// `@extra` of the newest gifts request (stale answers are dropped).
    pub gifts_request: u64,
    pub gift_selected: Option<String>,
    /// `sellGift` awaiting confirmation for this gift id.
    pub gift_convert_confirm: Option<String>,
    pub gift_mutating: bool,
    pub gifts_stale: bool,
    // Premium.
    pub premium_open: bool,
    pub premium: Option<PremiumInfo>,
    pub premium_state: Option<PremiumStateInfo>,
    pub premium_loading: bool,
    pub premium_error: Option<String>,
}

impl PremiumHub {
    pub fn apply_transactions(&mut self, page: StarTxPage, append: bool) {
        self.tx_loading = false;
        self.tx_error = None;
        self.tx_loaded = true;
        self.balance = Some(page.balance);
        self.tx_offset = page.next_offset;
        if append {
            self.transactions.extend(page.transactions);
        } else {
            self.transactions = page.transactions;
        }
    }

    pub fn apply_gifts(&mut self, page: GiftsPage, append: bool) {
        self.gifts_loading = false;
        self.gifts_error = None;
        self.gifts_loaded = true;
        self.gifts_stale = false;
        self.gifts_total = page.total_count;
        self.gifts_offset = page.next_offset;
        if append {
            self.gifts.extend(page.gifts);
        } else {
            self.gifts = page.gifts;
        }
    }

    pub fn selected_tx(&self) -> Option<&StarTx> {
        let id = self.tx_selected.as_deref()?;
        self.transactions.iter().find(|tx| tx.id == id)
    }

    pub fn selected_gift(&self) -> Option<&ReceivedGift> {
        let id = self.gift_selected.as_deref()?;
        self.gifts.iter().find(|g| g.id == id)
    }

    /// Whether the open gifts list belongs to the signed-in user (the only
    /// case where visibility toggles and conversion are offered).
    pub fn gifts_are_mine(&self, my_user_id: Option<i64>) -> bool {
        matches!(
            (self.gifts_owner, my_user_id),
            (Some(MessageSender::User { user_id }), Some(me)) if user_id == me
        )
    }

    /// Flip `is_saved` after the server confirmed `toggleGiftIsSaved`.
    pub fn set_gift_saved(&mut self, id: &str, saved: bool) {
        if let Some(gift) = self.gifts.iter_mut().find(|g| g.id == id) {
            gift.is_saved = saved;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn star_amount_formats_whole_and_fraction() {
        let a = StarAmount {
            stars: 1234567,
            nanos: 0,
        };
        assert_eq!(a.label(), "1,234,567");
        let b = StarAmount {
            stars: 12,
            nanos: 500_000_000,
        };
        assert_eq!(b.label(), "12.5");
        let out = StarAmount {
            stars: -50,
            nanos: 0,
        };
        assert_eq!(out.signed_label(), "-50");
        assert_eq!(
            StarAmount {
                stars: 100,
                nanos: 0
            }
            .signed_label(),
            "+100"
        );
        assert!(
            StarAmount {
                stars: 0,
                nanos: -1
            }
            .is_negative()
        );
        assert_eq!(
            StarAmount {
                stars: 0,
                nanos: -250_000_000
            }
            .label(),
            "-0.25"
        );
    }

    #[test]
    fn transaction_types_map_to_titles() {
        let page = parse_star_transactions(&json!({
            "@type": "starTransactions",
            "star_amount": {"star_count": 640, "nanostar_count": 0},
            "next_offset": "o2",
            "transactions": [
                {"id": "a", "star_amount": {"star_count": 500}, "is_refund": false, "date": 1700000000,
                 "type": {"@type": "starTransactionTypePremiumBotDeposit"}},
                {"id": "b", "star_amount": {"star_count": -100}, "is_refund": false, "date": 1700000100,
                 "type": {"@type": "starTransactionTypeGiftPurchase",
                          "owner_id": {"@type": "messageSenderUser", "user_id": 7},
                          "gift": {"@type": "gift", "star_count": 100}}},
                {"id": "c", "star_amount": {"star_count": 25}, "is_refund": true, "date": 1700000200,
                 "type": {"@type": "starTransactionTypeBotInvoicePurchase", "user_id": 9,
                          "product_info": {"title": "Pro plan"}}},
                {"id": "d", "star_amount": {"star_count": 1}, "date": 1,
                 "type": {"@type": "starTransactionTypeBrandNew"}}
            ]
        }));
        assert_eq!(page.balance.stars, 640);
        assert_eq!(page.next_offset, "o2");
        assert_eq!(page.transactions.len(), 4);
        assert_eq!(page.transactions[0].title, "Stars top-up");
        assert_eq!(page.transactions[0].detail, "Via Premium Bot");
        assert!(page.transactions[0].is_incoming());
        assert_eq!(page.transactions[1].title, "Gift");
        assert_eq!(page.transactions[1].detail, "100 Stars");
        assert_eq!(
            page.transactions[1].peer,
            Some(MessageSender::User { user_id: 7 })
        );
        assert!(!page.transactions[1].is_incoming());
        assert_eq!(page.transactions[2].title, "Pro plan");
        assert!(page.transactions[2].is_refund);
        assert_eq!(page.transactions[3].title, "Transaction");
    }

    #[test]
    fn received_gifts_parse_regular_and_collectible() {
        let page = parse_received_gifts(&json!({
            "@type": "receivedGifts", "total_count": 2, "next_offset": "",
            "gifts": [
              {"received_gift_id": "g1", "sender_id": {"@type": "messageSenderUser", "user_id": 5},
               "text": {"text": "Happy birthday!", "entities": []}, "is_private": false,
               "is_saved": true, "is_pinned": false, "can_be_upgraded": true, "date": 1700000000,
               "sell_star_count": 85,
               "gift": {"@type": "sentGiftRegular", "gift": {"@type": "gift", "id": "1", "star_count": 100,
                        "sticker": {"@type": "sticker", "id": 1, "emoji": "x", "width": 512, "height": 512,
                                    "format": {"@type": "stickerFormatWebp"},
                                    "sticker": {"@type": "file", "id": 11, "size": 10, "expected_size": 10,
                                                "local": {"path": "", "is_downloading_completed": false, "can_be_downloaded": true},
                                                "remote": {"id": "r", "unique_id": "u"}}}}}},
              {"received_gift_id": "g2", "is_saved": false, "date": 1700000500, "sell_star_count": 0,
               "gift": {"@type": "sentGiftUpgraded", "gift": {"@type": "upgradedGift", "title": "Plush Pepe",
                        "number": 123, "backdrop": {"colors": {"center_color": 16711680, "edge_color": 255}}}}}
            ]
        }));
        assert_eq!(page.total_count, 2);
        assert_eq!(page.gifts.len(), 2);
        let first = &page.gifts[0];
        assert_eq!(first.text, "Happy birthday!");
        assert!(first.is_saved && first.can_convert());
        assert_eq!(first.headline(), "100 Stars");
        assert!(first.gift.sticker.is_some());
        assert_eq!(first.gift.files.len(), 1);
        let second = &page.gifts[1];
        assert!(second.gift.upgraded);
        assert_eq!(second.headline(), "Plush Pepe #123");
        assert_eq!(second.gift.backdrop, Some((0xFF0000, 0x0000FF)));
        assert!(!second.can_convert());
    }

    #[test]
    fn gifts_without_an_id_are_dropped() {
        let page = parse_received_gifts(&json!({"gifts": [{"gift": {"gift": {}}}]}));
        assert!(page.gifts.is_empty());
    }

    #[test]
    fn premium_features_keep_known_rows_and_limits() {
        let info = parse_premium_features(&json!({
            "features": [
                {"@type": "premiumFeatureIncreasedLimits"},
                {"@type": "premiumFeatureFromTheFuture"},
                {"@type": "premiumFeatureVoiceRecognition"}
            ],
            "limits": [
                {"type": {"@type": "premiumLimitTypeChatFolderCount"}, "default_value": 10, "premium_value": 20},
                {"type": {"@type": "premiumLimitTypeUnknown"}, "default_value": 1, "premium_value": 2}
            ]
        }));
        assert_eq!(info.features.len(), 2);
        assert_eq!(info.features[0].title, "Doubled limits");
        assert_eq!(info.limits.len(), 1);
        assert_eq!(info.limits[0].title, "Folders");
        assert_eq!(info.limits[0].premium_value, 20);
    }

    #[test]
    fn premium_state_reads_current_plan() {
        let s = parse_premium_state(&json!({
            "state": {"text": "Thanks for subscribing", "entities": []},
            "payment_options": [{"is_current": false}, {"is_current": true}]
        }));
        assert!(s.is_subscribed);
        assert_eq!(s.text, "Thanks for subscribing");
        assert!(!parse_premium_state(&json!({})).is_subscribed);
    }

    #[test]
    fn hub_pages_append_and_replace() {
        let mut hub = PremiumHub::default();
        let tx = |id: &str| StarTx {
            id: id.into(),
            amount: StarAmount { stars: 1, nanos: 0 },
            is_refund: false,
            date: 0,
            title: "t".into(),
            detail: String::new(),
            peer: None,
            kind: String::new(),
        };
        hub.apply_transactions(
            StarTxPage {
                balance: StarAmount { stars: 9, nanos: 0 },
                transactions: vec![tx("a")],
                next_offset: "n".into(),
            },
            false,
        );
        hub.apply_transactions(
            StarTxPage {
                balance: StarAmount { stars: 9, nanos: 0 },
                transactions: vec![tx("b")],
                next_offset: String::new(),
            },
            true,
        );
        assert_eq!(hub.transactions.len(), 2);
        assert!(hub.tx_offset.is_empty());
        hub.tx_selected = Some("b".into());
        assert_eq!(hub.selected_tx().map(|t| t.id.as_str()), Some("b"));
        hub.apply_transactions(StarTxPage::default(), false);
        assert!(hub.transactions.is_empty());
    }

    #[test]
    fn only_own_gifts_are_mine() {
        let mut hub = PremiumHub {
            gifts_owner: Some(MessageSender::User { user_id: 4 }),
            ..Default::default()
        };
        assert!(hub.gifts_are_mine(Some(4)));
        assert!(!hub.gifts_are_mine(Some(5)));
        assert!(!hub.gifts_are_mine(None));
        hub.gifts_owner = Some(MessageSender::Chat { chat_id: 4 });
        assert!(!hub.gifts_are_mine(Some(4)));
    }

    #[test]
    fn filters_map_to_td_directions() {
        assert_eq!(TxFilter::All.direction_type(), None);
        assert_eq!(
            TxFilter::Incoming.direction_type(),
            Some("transactionDirectionIncoming")
        );
        assert_eq!(
            TxFilter::Outgoing.direction_type(),
            Some("transactionDirectionOutgoing")
        );
    }
}

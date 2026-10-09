//! Service actions: every TDLib `MessageContent` constructor that is a
//! chat event rather than user content ("Dana added Omar", "pinned a
//! message", gifts, giveaways, forum topics…). The parsed action keeps
//! only structured facts (ids, titles, counts); the wording lives in
//! `crate::service_text`, which follows Telegram Desktop's
//! `HistoryItem::setServiceMessageByAction` strings.

use super::*;
use serde_json::Value;

/// Which expired media a `messageExpired*` stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiredKind {
    Photo,
    Video,
    VideoNote,
    VoiceNote,
}

/// `BotWriteAccessAllowReason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotAccessReason {
    ConnectedWebsite { domain: String },
    AttachMenu,
    WebApp { title: String },
    AcceptedRequest,
}

/// `PassportElementType`, grouped the way Telegram Desktop words them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassportKind {
    PersonalDetails,
    ProofOfIdentity,
    Address,
    ProofOfAddress,
    Phone,
    Email,
}

/// `UpgradedGiftOrigin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftOrigin {
    Upgrade,
    Transfer,
    Resale,
    Blockchain,
    PrepaidUpgrade,
    Offer,
    Craft,
}

/// `SuggestedPostPrice` / amounts: currency string ("XTR" for stars) and
/// the amount in the currency's smallest unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    pub currency: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceAction {
    ChatCreate {
        title: String,
        supergroup: bool,
    },
    ChatTitle {
        title: String,
    },
    /// The new photo (thumbnail shown under the row).
    ChatPhoto {
        photo: Option<PhotoContent>,
    },
    ChatPhotoDeleted,
    OwnerLeft {
        new_owner: i64,
    },
    OwnerChanged {
        new_owner: i64,
    },
    ProtectedToggled {
        enabled: bool,
    },
    ProtectedDisableRequested {
        expired: bool,
    },
    AddMembers {
        users: Vec<i64>,
    },
    DeleteMember {
        user: i64,
    },
    JoinByLink,
    JoinByRequest,
    UpgradedTo,
    UpgradedFrom {
        title: String,
    },
    Pin {
        message_id: i64,
    },
    Background {
        only_for_self: bool,
    },
    /// `messageChatSetTheme`: an emoji theme, a gift theme (`gift_name`),
    /// or neither (disabled).
    Theme {
        emoji: String,
        gift_name: Option<String>,
    },
    Boost {
        count: i32,
    },
    TopicCreated {
        name: String,
    },
    TopicEdited {
        name: String,
        icon_edited: bool,
        icon_custom_emoji_id: i64,
    },
    TopicClosed {
        closed: bool,
    },
    TopicHidden {
        hidden: bool,
    },
    SuggestProfilePhoto {
        photo: Option<PhotoContent>,
    },
    SuggestBirthdate {
        day: i32,
        month: i32,
        year: i32,
    },
    Custom {
        text: String,
    },
    GameScore {
        game_message_id: i64,
        score: i32,
    },
    ManagedBotCreated {
        bot_user_id: i64,
    },
    PaymentRefunded {
        owner: Option<MessageSender>,
        money: Money,
    },
    GiftedPremium {
        gifter: i64,
        receiver: i64,
        money: Money,
    },
    PremiumGiftCode {
        creator: Option<MessageSender>,
        from_giveaway: bool,
        unclaimed: bool,
        money: Money,
    },
    GiveawayCreated {
        stars: i64,
    },
    Giveaway {
        winners: i32,
        stars: i64,
        months: i32,
    },
    GiveawayCompleted {
        winners: i32,
        stars: bool,
        unclaimed: i32,
    },
    GiveawayWinners {
        winners: i32,
        unclaimed: i32,
        stars: bool,
    },
    GiftedStars {
        gifter: i64,
        receiver: i64,
        money: Money,
    },
    GiftedGrams {
        gifter: i64,
        receiver: i64,
        grams: i64,
    },
    GiveawayPrizeStars {
        boosted_chat_id: i64,
        unclaimed: bool,
    },
    Gift {
        sender: Option<MessageSender>,
        receiver: Option<MessageSender>,
        /// Star price of the gift (including a bundled upgrade).
        stars: i64,
        prepaid_upgrade: bool,
        from_auction: bool,
    },
    UpgradedGift {
        name: String,
        sender: Option<MessageSender>,
        receiver: Option<MessageSender>,
        origin: GiftOrigin,
        /// Resale / offer price, formatted in `Money` terms.
        price: Option<Money>,
    },
    RefundedUpgradedGift,
    GiftOffer {
        name: String,
        price: Money,
        /// 0 pending, 1 accepted, 2 rejected.
        state: u8,
    },
    GiftOfferRejected {
        name: String,
        price: Money,
        expired: bool,
    },
    PaidMessagesRefunded {
        count: i32,
        stars: i64,
    },
    PaidMessagePriceChanged {
        stars: i64,
    },
    DirectMessagePriceChanged {
        enabled: bool,
        stars: i64,
    },
    ChecklistDone {
        checklist_message_id: i64,
        done: usize,
        not_done: usize,
    },
    ChecklistAdded {
        titles: Vec<String>,
    },
    PollOptionAdded {
        text: String,
    },
    PollOptionDeleted {
        text: String,
    },
    SuggestedPostApprovalFailed,
    SuggestedPostApproved,
    SuggestedPostDeclined,
    SuggestedPostPaid {
        stars: i64,
        grams: i64,
    },
    SuggestedPostRefunded {
        post_deleted: bool,
    },
    ContactRegistered,
    UsersShared {
        users: Vec<(i64, String)>,
    },
    ChatShared {
        chat_id: i64,
        title: String,
    },
    BotWriteAccess {
        reason: BotAccessReason,
    },
    WebAppDataSent {
        button: String,
    },
    WebAppDataReceived {
        button: String,
    },
    PassportSent {
        kinds: Vec<PassportKind>,
    },
    PassportReceived,
    Proximity {
        traveler: MessageSender,
        watcher: MessageSender,
        distance: i32,
    },
    VideoChatScheduled {
        start_date: i32,
    },
    VideoChatStarted,
    VideoChatEnded {
        duration: i32,
    },
    VideoChatInvite {
        users: Vec<i64>,
    },
    Expired(ExpiredKind),
    StakeDice {
        stake: i64,
        prize: i64,
    },
    Story {
        via_mention: bool,
    },
    PaidMedia {
        stars: i64,
    },
    Checklist {
        title: String,
    },
}

fn i(value: &Value, key: &str) -> i64 {
    int64(value.get(key)).unwrap_or(0)
}

fn n(value: &Value, key: &str) -> i32 {
    i(value, key) as i32
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn string(value: &Value, key: &str) -> String {
    json_field_str(value, key)
}

fn ids(value: &Value, key: &str) -> Vec<i64> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| int64(Some(v)))
        .collect()
}

fn sender(value: &Value, key: &str) -> Option<MessageSender> {
    parse_message_sender(value.get(key)).ok()
}

fn money(value: &Value) -> Money {
    Money {
        currency: string(value, "currency"),
        amount: i(value, "amount"),
    }
}

fn formatted(value: &Value, key: &str) -> String {
    parse_formatted_text(value.get(key))
}

/// A `giftResalePrice`: stars, or grams in cents.
fn resale_price(value: Option<&Value>) -> Option<Money> {
    let value = value?;
    match value.get("@type").and_then(Value::as_str)? {
        "giftResalePriceStar" => Some(Money {
            currency: "XTR".into(),
            amount: i(value, "star_count"),
        }),
        "giftResalePriceGram" => Some(Money {
            currency: "TON".into(),
            amount: i(value, "gram_cent_count"),
        }),
        _ => None,
    }
}

fn upgraded_gift_name(gift: Option<&Value>) -> String {
    let Some(gift) = gift else {
        return String::new();
    };
    let title = string(gift, "title");
    match n(gift, "number") {
        0 => title,
        number => format!("{title} #{number}"),
    }
}

fn chat_photo(value: &Value, files: &mut Vec<ParsedFile>) -> Option<PhotoContent> {
    let photo = value.get("photo").filter(|p| !p.is_null())?;
    let (sizes, photo_files) = parse_photo_sizes(photo);
    if sizes.is_empty() {
        return None;
    }
    files.extend(photo_files);
    Some(PhotoContent {
        has_stickers: false,
        caption: String::new(),
        caption_entities: Vec::new(),
        show_caption_above_media: false,
        sizes,
        is_secret: false,
        has_spoiler: false,
        minithumbnail: parse_minithumbnail(photo.get("minithumbnail")),
    })
}

fn passport_kind(value: &Value) -> Option<PassportKind> {
    Some(
        match value
            .get("@type")?
            .as_str()?
            .strip_prefix("passportElementType")?
        {
            "PersonalDetails" => PassportKind::PersonalDetails,
            "Passport" | "DriverLicense" | "IdentityCard" | "InternalPassport" => {
                PassportKind::ProofOfIdentity
            }
            "Address" => PassportKind::Address,
            "UtilityBill"
            | "BankStatement"
            | "RentalAgreement"
            | "PassportRegistration"
            | "TemporaryRegistration" => PassportKind::ProofOfAddress,
            "PhoneNumber" => PassportKind::Phone,
            "EmailAddress" => PassportKind::Email,
            _ => return None,
        },
    )
}

impl ServiceAction {
    /// Parses the service content constructors. Returns `None` for
    /// constructors that are not service actions (or are unknown), so the
    /// caller falls back to `Unsupported`.
    pub(crate) fn from_td(
        kind: &str,
        value: &Value,
        files: &mut Vec<ParsedFile>,
    ) -> Option<ServiceAction> {
        use ServiceAction as A;
        Some(match kind {
            "messageBasicGroupChatCreate" => A::ChatCreate {
                title: string(value, "title"),
                supergroup: false,
            },
            "messageSupergroupChatCreate" => A::ChatCreate {
                title: string(value, "title"),
                supergroup: true,
            },
            "messageChatChangeTitle" => A::ChatTitle {
                title: string(value, "title"),
            },
            "messageChatChangePhoto" => A::ChatPhoto {
                photo: chat_photo(value, files),
            },
            "messageChatDeletePhoto" => A::ChatPhotoDeleted,
            "messageChatOwnerLeft" => A::OwnerLeft {
                new_owner: i(value, "new_owner_user_id"),
            },
            "messageChatOwnerChanged" => A::OwnerChanged {
                new_owner: i(value, "new_owner_user_id"),
            },
            "messageChatHasProtectedContentToggled" => A::ProtectedToggled {
                enabled: flag(value, "new_has_protected_content"),
            },
            "messageChatHasProtectedContentDisableRequested" => A::ProtectedDisableRequested {
                expired: flag(value, "is_expired"),
            },
            "messageChatAddMembers" => A::AddMembers {
                users: ids(value, "member_user_ids"),
            },
            "messageChatDeleteMember" => A::DeleteMember {
                user: i(value, "user_id"),
            },
            "messageChatJoinByLink" => A::JoinByLink,
            "messageChatJoinByRequest" => A::JoinByRequest,
            "messageChatUpgradeTo" => A::UpgradedTo,
            "messageChatUpgradeFrom" => A::UpgradedFrom {
                title: string(value, "title"),
            },
            "messagePinMessage" => A::Pin {
                message_id: i(value, "message_id"),
            },
            "messageChatSetBackground" => A::Background {
                only_for_self: flag(value, "only_for_self"),
            },
            "messageChatSetTheme" => {
                let theme = value.get("theme");
                let theme_type = theme.and_then(|t| t.get("@type")).and_then(Value::as_str);
                A::Theme {
                    emoji: theme
                        .filter(|_| theme_type == Some("chatThemeEmoji"))
                        .map(|t| string(t, "name"))
                        .unwrap_or_default(),
                    gift_name: (theme_type == Some("chatThemeGift")).then(|| {
                        upgraded_gift_name(
                            theme.and_then(|t| t.get("gift_theme").and_then(|g| g.get("gift"))),
                        )
                    }),
                }
            }
            "messageChatBoost" => A::Boost {
                count: n(value, "boost_count"),
            },
            "messageForumTopicCreated" => A::TopicCreated {
                name: string(value, "name"),
            },
            "messageForumTopicEdited" => A::TopicEdited {
                name: string(value, "name"),
                icon_edited: flag(value, "edit_icon_custom_emoji_id"),
                icon_custom_emoji_id: i(value, "icon_custom_emoji_id"),
            },
            "messageForumTopicIsClosedToggled" => A::TopicClosed {
                closed: flag(value, "is_closed"),
            },
            "messageForumTopicIsHiddenToggled" => A::TopicHidden {
                hidden: flag(value, "is_hidden"),
            },
            "messageSuggestProfilePhoto" => A::SuggestProfilePhoto {
                photo: chat_photo(value, files),
            },
            "messageSuggestBirthdate" => {
                let birthdate = value.get("birthdate").unwrap_or(&Value::Null);
                A::SuggestBirthdate {
                    day: n(birthdate, "day"),
                    month: n(birthdate, "month"),
                    year: n(birthdate, "year"),
                }
            }
            "messageCustomServiceAction" => A::Custom {
                text: string(value, "text"),
            },
            "messageGameScore" => A::GameScore {
                game_message_id: i(value, "game_message_id"),
                score: n(value, "score"),
            },
            "messageManagedBotCreated" => A::ManagedBotCreated {
                bot_user_id: i(value, "bot_user_id"),
            },
            "messagePaymentRefunded" => A::PaymentRefunded {
                owner: sender(value, "owner_id"),
                money: Money {
                    currency: string(value, "currency"),
                    amount: i(value, "total_amount"),
                },
            },
            "messageGiftedPremium" => A::GiftedPremium {
                gifter: i(value, "gifter_user_id"),
                receiver: i(value, "receiver_user_id"),
                money: money(value),
            },
            "messagePremiumGiftCode" => A::PremiumGiftCode {
                creator: sender(value, "creator_id"),
                from_giveaway: flag(value, "is_from_giveaway"),
                unclaimed: flag(value, "is_unclaimed"),
                money: money(value),
            },
            "messageGiveawayCreated" => A::GiveawayCreated {
                stars: i(value, "star_count"),
            },
            "messageGiveaway" => {
                let prize = value.get("prize").unwrap_or(&Value::Null);
                A::Giveaway {
                    winners: n(value, "winner_count"),
                    stars: i(prize, "star_count"),
                    months: n(prize, "month_count"),
                }
            }
            "messageGiveawayCompleted" => A::GiveawayCompleted {
                winners: n(value, "winner_count"),
                stars: flag(value, "is_star_giveaway"),
                unclaimed: n(value, "unclaimed_prize_count"),
            },
            "messageGiveawayWinners" => A::GiveawayWinners {
                winners: n(value, "winner_count"),
                unclaimed: n(value, "unclaimed_prize_count"),
                stars: value
                    .get("prize")
                    .and_then(|p| p.get("@type"))
                    .and_then(Value::as_str)
                    == Some("giveawayPrizeStars"),
            },
            "messageGiftedStars" => A::GiftedStars {
                gifter: i(value, "gifter_user_id"),
                receiver: i(value, "receiver_user_id"),
                money: money(value),
            },
            "messageGiftedGrams" => A::GiftedGrams {
                gifter: i(value, "gifter_user_id"),
                receiver: i(value, "receiver_user_id"),
                grams: i(value, "gram_amount"),
            },
            "messageGiveawayPrizeStars" => A::GiveawayPrizeStars {
                boosted_chat_id: i(value, "boosted_chat_id"),
                unclaimed: flag(value, "is_unclaimed"),
            },
            "messageGift" => {
                let gift = value.get("gift").unwrap_or(&Value::Null);
                let prepaid_upgrade = flag(value, "is_prepaid_upgrade");
                let base = i(gift, "star_count");
                let upgrade = i(value, "prepaid_upgrade_star_count");
                A::Gift {
                    sender: sender(value, "sender_id"),
                    receiver: sender(value, "receiver_id"),
                    stars: if prepaid_upgrade {
                        upgrade
                    } else if flag(value, "is_upgrade_separate") {
                        base
                    } else {
                        base + upgrade
                    },
                    prepaid_upgrade,
                    from_auction: flag(value, "is_from_auction"),
                }
            }
            "messageUpgradedGift" => {
                let origin_value = value.get("origin");
                let origin = match origin_value
                    .and_then(|o| o.get("@type"))
                    .and_then(Value::as_str)
                {
                    Some("upgradedGiftOriginTransfer") => GiftOrigin::Transfer,
                    Some("upgradedGiftOriginResale") => GiftOrigin::Resale,
                    Some("upgradedGiftOriginBlockchain") => GiftOrigin::Blockchain,
                    Some("upgradedGiftOriginPrepaidUpgrade") => GiftOrigin::PrepaidUpgrade,
                    Some("upgradedGiftOriginOffer") => GiftOrigin::Offer,
                    Some("upgradedGiftOriginCraft") => GiftOrigin::Craft,
                    _ => GiftOrigin::Upgrade,
                };
                A::UpgradedGift {
                    name: upgraded_gift_name(value.get("gift")),
                    sender: sender(value, "sender_id"),
                    receiver: sender(value, "receiver_id"),
                    origin,
                    price: resale_price(origin_value.and_then(|o| o.get("price"))),
                }
            }
            "messageRefundedUpgradedGift" => A::RefundedUpgradedGift,
            "messageUpgradedGiftPurchaseOffer" => A::GiftOffer {
                name: upgraded_gift_name(value.get("gift")),
                price: resale_price(value.get("price")).unwrap_or(Money {
                    currency: "XTR".into(),
                    amount: 0,
                }),
                state: match value
                    .get("state")
                    .and_then(|s| s.get("@type"))
                    .and_then(Value::as_str)
                {
                    Some("giftPurchaseOfferStateAccepted") => 1,
                    Some("giftPurchaseOfferStateRejected") => 2,
                    _ => 0,
                },
            },
            "messageUpgradedGiftPurchaseOfferRejected" => A::GiftOfferRejected {
                name: upgraded_gift_name(value.get("gift")),
                price: resale_price(value.get("price")).unwrap_or(Money {
                    currency: "XTR".into(),
                    amount: 0,
                }),
                expired: flag(value, "was_expired"),
            },
            "messagePaidMessagesRefunded" => A::PaidMessagesRefunded {
                count: n(value, "message_count"),
                stars: i(value, "star_count"),
            },
            "messagePaidMessagePriceChanged" => A::PaidMessagePriceChanged {
                stars: i(value, "paid_message_star_count"),
            },
            "messageDirectMessagePriceChanged" => A::DirectMessagePriceChanged {
                enabled: flag(value, "is_enabled"),
                stars: i(value, "paid_message_star_count"),
            },
            "messageChecklistTasksDone" => A::ChecklistDone {
                checklist_message_id: i(value, "checklist_message_id"),
                done: ids(value, "marked_as_done_task_ids").len(),
                not_done: ids(value, "marked_as_not_done_task_ids").len(),
            },
            "messageChecklistTasksAdded" => A::ChecklistAdded {
                titles: value
                    .get("tasks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|task| formatted(task, "text"))
                    .collect(),
            },
            "messagePollOptionAdded" => A::PollOptionAdded {
                text: formatted(value, "text"),
            },
            "messagePollOptionDeleted" => A::PollOptionDeleted {
                text: formatted(value, "text"),
            },
            "messageSuggestedPostApprovalFailed" => A::SuggestedPostApprovalFailed,
            "messageSuggestedPostApproved" => A::SuggestedPostApproved,
            "messageSuggestedPostDeclined" => A::SuggestedPostDeclined,
            "messageSuggestedPostPaid" => A::SuggestedPostPaid {
                stars: value
                    .get("star_amount")
                    .map(|a| i(a, "star_count"))
                    .unwrap_or(0),
                grams: i(value, "gram_amount"),
            },
            "messageSuggestedPostRefunded" => A::SuggestedPostRefunded {
                post_deleted: value
                    .get("reason")
                    .and_then(|r| r.get("@type"))
                    .and_then(Value::as_str)
                    == Some("suggestedPostRefundReasonPostDeleted"),
            },
            "messageContactRegistered" => A::ContactRegistered,
            "messageUsersShared" => A::UsersShared {
                users: value
                    .get("users")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|user| {
                        let name = format!(
                            "{} {}",
                            string(user, "first_name"),
                            string(user, "last_name")
                        );
                        (i(user, "user_id"), name.trim().to_string())
                    })
                    .collect(),
            },
            "messageChatShared" => {
                let chat = value.get("chat").unwrap_or(&Value::Null);
                A::ChatShared {
                    chat_id: i(chat, "chat_id"),
                    title: string(chat, "title"),
                }
            }
            "messageBotWriteAccessAllowed" => {
                let reason = value.get("reason");
                let reason_type = reason.and_then(|r| r.get("@type")).and_then(Value::as_str);
                A::BotWriteAccess {
                    reason: match reason_type {
                        Some("botWriteAccessAllowReasonConnectedWebsite") => {
                            BotAccessReason::ConnectedWebsite {
                                domain: reason
                                    .map(|r| string(r, "domain_name"))
                                    .unwrap_or_default(),
                            }
                        }
                        Some("botWriteAccessAllowReasonAddedToAttachmentMenu") => {
                            BotAccessReason::AttachMenu
                        }
                        Some("botWriteAccessAllowReasonLaunchedWebApp") => {
                            BotAccessReason::WebApp {
                                title: reason
                                    .and_then(|r| r.get("web_app"))
                                    .map(|a| string(a, "title"))
                                    .unwrap_or_default(),
                            }
                        }
                        _ => BotAccessReason::AcceptedRequest,
                    },
                }
            }
            "messageWebAppDataSent" => A::WebAppDataSent {
                button: string(value, "button_text"),
            },
            "messageWebAppDataReceived" => A::WebAppDataReceived {
                button: string(value, "button_text"),
            },
            "messagePassportDataSent" => A::PassportSent {
                kinds: value
                    .get("types")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(passport_kind)
                    .collect(),
            },
            "messagePassportDataReceived" => A::PassportReceived,
            "messageProximityAlertTriggered" => A::Proximity {
                traveler: sender(value, "traveler_id")?,
                watcher: sender(value, "watcher_id")?,
                distance: n(value, "distance"),
            },
            "messageVideoChatScheduled" => A::VideoChatScheduled {
                start_date: n(value, "start_date"),
            },
            "messageVideoChatStarted" => A::VideoChatStarted,
            "messageVideoChatEnded" => A::VideoChatEnded {
                duration: n(value, "duration"),
            },
            "messageInviteVideoChatParticipants" => A::VideoChatInvite {
                users: ids(value, "user_ids"),
            },
            "messageExpiredPhoto" => A::Expired(ExpiredKind::Photo),
            "messageExpiredVideo" => A::Expired(ExpiredKind::Video),
            "messageExpiredVideoNote" => A::Expired(ExpiredKind::VideoNote),
            "messageExpiredVoiceNote" => A::Expired(ExpiredKind::VoiceNote),
            "messageStakeDice" => A::StakeDice {
                stake: i(value, "stake_gram_amount"),
                prize: i(value, "prize_gram_amount"),
            },
            "messageStory" => A::Story {
                via_mention: flag(value, "via_mention"),
            },
            "messagePaidMedia" => A::PaidMedia {
                stars: i(value, "star_count"),
            },
            "messageChecklist" => A::Checklist {
                title: value
                    .get("list")
                    .map(|l| formatted(l, "title"))
                    .unwrap_or_default(),
            },
            _ => return None,
        })
    }
}

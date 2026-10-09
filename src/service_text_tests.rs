//! One test per service action: recorded TDLib JSON in, Telegram
//! Desktop's wording (and the links) out.
use crate::service_text::{PinTarget, ServiceCtx, ServiceLink, ServiceNames, ServiceText, render};
use crate::telegram::envelope::{
    MessageContent, MessageSender, ParsedFile, ServiceAction, parse_content,
};
use serde_json::Value;

const ME: i64 = 9;
const DANA: i64 = 1;
const OMAR: i64 = 2;
const LEA: i64 = 3;
const CHAT: i64 = -1001;

struct Names;

impl ServiceNames for Names {
    fn user_name(&self, id: i64) -> Option<String> {
        match id {
            DANA => Some("Dana Cole".into()),
            OMAR => Some("Omar Haddad".into()),
            LEA => Some("Lea Stern".into()),
            ME => Some("Idan Birman".into()),
            _ => None,
        }
    }
    fn user_short_name(&self, id: i64) -> Option<String> {
        self.user_name(id)
            .map(|n| n.split(' ').next().unwrap_or("").to_string())
    }
    fn chat_name(&self, id: i64) -> Option<String> {
        match id {
            CHAT => Some("Design Club".into()),
            -2002 => Some("Launch Channel".into()),
            _ => None,
        }
    }
    fn pinned(&self, id: i64) -> PinTarget {
        match id {
            100 => PinTarget::Text("Lunch at noon, bring snacks!".into()),
            101 => PinTarget::Text("Short note".into()),
            102 => PinTarget::Media("a photo".into()),
            _ => PinTarget::Loading,
        }
    }
    fn game_title(&self, id: i64) -> Option<String> {
        (id == 300).then(|| "Lumberjack".to_string())
    }
}

#[derive(Clone, Copy)]
struct Opts {
    actor: Option<MessageSender>,
    outgoing: bool,
    channel: bool,
}

const BY_DANA: Opts = Opts {
    actor: Some(MessageSender::User { user_id: DANA }),
    outgoing: false,
    channel: false,
};
const BY_ME: Opts = Opts {
    actor: Some(MessageSender::User { user_id: ME }),
    outgoing: true,
    channel: false,
};
const IN_CHANNEL: Opts = Opts {
    actor: None,
    outgoing: false,
    channel: true,
};

fn parse(json: &str) -> (MessageContent, Vec<ParsedFile>) {
    let value: Value = serde_json::from_str(json).expect("valid json");
    parse_content(Some(&value))
}

fn run(json: &str, opts: Opts) -> ServiceText {
    let (content, _) = parse(json);
    let ctx = ServiceCtx {
        actor: opts.actor,
        is_outgoing: opts.outgoing,
        my_id: Some(ME),
        is_channel: opts.channel,
        is_secret: false,
        chat_id: CHAT,
        now: 1_800_000_000,
        names: &Names,
    };
    render(&content, &ctx).unwrap_or_else(|| panic!("not a service message: {json}"))
}

fn say(json: &str, opts: Opts) -> String {
    run(json, opts).plain()
}

macro_rules! wording {
    ($name:ident, $opts:expr, $json:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(say($json, $opts), $expected);
        }
    };
}

wording!(
    basic_group_created,
    BY_DANA,
    r#"{"@type":"messageBasicGroupChatCreate","title":"Weekend","member_user_ids":[2,3]}"#,
    "Dana Cole created the group \u{AB}Weekend\u{BB}"
);
wording!(
    supergroup_created,
    BY_DANA,
    r#"{"@type":"messageSupergroupChatCreate","title":"Weekend"}"#,
    "Dana Cole created the group \u{AB}Weekend\u{BB}"
);
wording!(
    channel_created,
    IN_CHANNEL,
    r#"{"@type":"messageSupergroupChatCreate","title":"Launch"}"#,
    "Channel created"
);
wording!(
    title_changed,
    BY_DANA,
    r#"{"@type":"messageChatChangeTitle","title":"Brunch Club"}"#,
    "Dana Cole changed group name to \u{AB}Brunch Club\u{BB}"
);
wording!(
    title_changed_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatChangeTitle","title":"Launch News"}"#,
    "Channel name was changed to \u{AB}Launch News\u{BB}"
);
wording!(
    photo_changed,
    BY_DANA,
    r#"{"@type":"messageChatChangePhoto","photo":{"@type":"chatPhoto","sizes":[]}}"#,
    "Dana Cole updated group photo"
);
wording!(
    photo_changed_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatChangePhoto","photo":{"@type":"chatPhoto","sizes":[]}}"#,
    "Channel photo updated"
);
wording!(
    photo_removed,
    BY_DANA,
    r#"{"@type":"messageChatDeletePhoto"}"#,
    "Dana Cole removed group photo"
);
wording!(
    photo_removed_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatDeletePhoto"}"#,
    "Channel photo removed"
);
wording!(
    owner_left,
    BY_DANA,
    r#"{"@type":"messageChatOwnerLeft","new_owner_user_id":2}"#,
    "Omar Haddad will become the new main admin in 7 days if Dana Cole does not return."
);
wording!(
    owner_changed,
    BY_DANA,
    r#"{"@type":"messageChatOwnerChanged","new_owner_user_id":2}"#,
    "Dana Cole made Omar Haddad the new main admin of the group."
);
wording!(
    protected_content_enabled,
    BY_DANA,
    r#"{"@type":"messageChatHasProtectedContentToggled","request_message_id":0,"old_has_protected_content":false,"new_has_protected_content":true}"#,
    "Dana Cole disabled sharing in this chat"
);
wording!(
    protected_content_disabled_by_me,
    BY_ME,
    r#"{"@type":"messageChatHasProtectedContentToggled","request_message_id":0,"old_has_protected_content":true,"new_has_protected_content":false}"#,
    "You enabled sharing in this chat"
);
wording!(
    protected_content_request,
    BY_DANA,
    r#"{"@type":"messageChatHasProtectedContentDisableRequested","is_expired":false}"#,
    "Dana Cole would like to enable sharing in this chat, which includes:\nForwarding messages\nSaving photos and videos\nCopying messages"
);
wording!(
    protected_content_request_expired,
    BY_DANA,
    r#"{"@type":"messageChatHasProtectedContentDisableRequested","is_expired":true}"#,
    "Sharing enable request has expired"
);
wording!(
    members_added_one,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[2]}"#,
    "Dana Cole added Omar Haddad"
);
wording!(
    members_added_many,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[2,3,9]}"#,
    "Dana Cole added Omar Haddad, Lea Stern and Idan Birman"
);
wording!(
    member_added_self_is_a_join,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[1]}"#,
    "Dana Cole joined the group"
);
wording!(
    member_added_you,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[9]}"#,
    "Dana Cole added you to this group"
);
wording!(
    member_removed,
    BY_DANA,
    r#"{"@type":"messageChatDeleteMember","user_id":2}"#,
    "Dana Cole removed Omar Haddad"
);
wording!(
    member_left,
    BY_DANA,
    r#"{"@type":"messageChatDeleteMember","user_id":1}"#,
    "Dana Cole left the group"
);
wording!(
    joined_by_link,
    BY_DANA,
    r#"{"@type":"messageChatJoinByLink"}"#,
    "Dana Cole joined the group via invite link"
);
wording!(
    joined_by_request,
    BY_DANA,
    r#"{"@type":"messageChatJoinByRequest"}"#,
    "Dana Cole was accepted to the group"
);
wording!(
    your_join_request_approved,
    BY_ME,
    r#"{"@type":"messageChatJoinByRequest"}"#,
    "Your request to join the group was approved"
);
wording!(
    group_upgraded_to,
    BY_DANA,
    r#"{"@type":"messageChatUpgradeTo","supergroup_id":42}"#,
    "This group was upgraded to a supergroup"
);
wording!(
    group_upgraded_from,
    BY_DANA,
    r#"{"@type":"messageChatUpgradeFrom","title":"Weekend","basic_group_id":7}"#,
    "The group \u{AB}Weekend\u{BB} was upgraded to a supergroup"
);
wording!(
    pinned_text_is_cut_at_sixteen_characters,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":100}"#,
    "Dana Cole pinned \"Lunch at noon, b\u{2026}\""
);
wording!(
    pinned_short_text_is_kept,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":101}"#,
    "Dana Cole pinned \"Short note\""
);
wording!(
    pinned_media,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":102}"#,
    "Dana Cole pinned a photo"
);
wording!(
    pinned_unloaded,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":999}"#,
    "Dana Cole pinned a message"
);
wording!(
    wallpaper_set_by_other,
    BY_DANA,
    r#"{"@type":"messageChatSetBackground","old_background_message_id":0,"background":{"@type":"chatBackground","dark_theme_dimming":0},"only_for_self":false}"#,
    "Dana Cole set a new wallpaper for this chat"
);
wording!(
    wallpaper_set_by_me_for_self,
    BY_ME,
    r#"{"@type":"messageChatSetBackground","old_background_message_id":0,"background":{"@type":"chatBackground"},"only_for_self":true}"#,
    "You set a new wallpaper for this chat"
);
wording!(
    theme_changed,
    BY_DANA,
    r#"{"@type":"messageChatSetTheme","theme":{"@type":"chatThemeEmoji","name":"☃"}}"#,
    "Dana Cole changed the chat theme to \u{2603}"
);
wording!(
    theme_disabled,
    BY_DANA,
    r#"{"@type":"messageChatSetTheme","theme":null}"#,
    "Dana Cole disabled the chat theme"
);
wording!(
    gift_theme_set_by_me,
    BY_ME,
    r#"{"@type":"messageChatSetTheme","theme":{"@type":"chatThemeGift","gift_theme":{"@type":"giftChatTheme","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":12}}}}"#,
    "You set Plush Pepe #12 as a new theme for this chat."
);
wording!(
    boost_applied,
    BY_DANA,
    r#"{"@type":"messageChatBoost","boost_count":3}"#,
    "Dana Cole boosted the group 3 times"
);
wording!(
    boost_applied_once,
    BY_DANA,
    r#"{"@type":"messageChatBoost","boost_count":1}"#,
    "Dana Cole boosted the group"
);
wording!(
    boost_applied_by_me,
    BY_ME,
    r#"{"@type":"messageChatBoost","boost_count":2}"#,
    "You boosted the group"
);
wording!(
    topic_created,
    BY_DANA,
    r#"{"@type":"messageForumTopicCreated","name":"Ideas","is_name_implicit":false,"icon":{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"}}"#,
    "The topic \"Ideas\" was created"
);
wording!(
    topic_renamed,
    BY_DANA,
    r#"{"@type":"messageForumTopicEdited","name":"Plans","edit_icon_custom_emoji_id":false,"icon_custom_emoji_id":"0"}"#,
    "Dana Cole renamed the topic to \"Plans\""
);
wording!(
    topic_icon_changed,
    BY_DANA,
    r#"{"@type":"messageForumTopicEdited","name":"","edit_icon_custom_emoji_id":true,"icon_custom_emoji_id":"5312"}"#,
    "Dana Cole changed the topic icon"
);
wording!(
    topic_closed,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsClosedToggled","is_closed":true}"#,
    "Dana Cole closed the topic"
);
wording!(
    topic_reopened,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsClosedToggled","is_closed":false}"#,
    "Dana Cole reopened the topic"
);
wording!(
    topic_hidden,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsHiddenToggled","is_hidden":true}"#,
    "Topic hidden"
);
wording!(
    topic_unhidden,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsHiddenToggled","is_hidden":false}"#,
    "Topic unhidden"
);
wording!(
    profile_photo_suggested,
    BY_DANA,
    r#"{"@type":"messageSuggestProfilePhoto","photo":{"@type":"chatPhoto","sizes":[]}}"#,
    "Dana Cole suggests this photo for your Telegram profile."
);
wording!(
    birthdate_suggested,
    BY_DANA,
    r#"{"@type":"messageSuggestBirthdate","birthdate":{"@type":"birthdate","day":6,"month":10,"year":1990}}"#,
    "Dana Cole suggests you add your date of birth: 6 October 1990"
);
wording!(
    custom_action,
    BY_DANA,
    r#"{"@type":"messageCustomServiceAction","text":"Welcome aboard"}"#,
    "Welcome aboard"
);
wording!(
    game_score_with_title,
    BY_DANA,
    r#"{"@type":"messageGameScore","game_message_id":300,"game_id":"5","score":120}"#,
    "Dana Cole scored 120 in Lumberjack"
);
wording!(
    game_score_without_title,
    BY_ME,
    r#"{"@type":"messageGameScore","game_message_id":999,"game_id":"5","score":15}"#,
    "You scored 15"
);
wording!(
    managed_bot_created,
    BY_DANA,
    r#"{"@type":"messageManagedBotCreated","bot_user_id":2,"manager_bot_user_id":3}"#,
    "Dana Cole created a bot Omar Haddad."
);
wording!(
    payment_refunded,
    BY_DANA,
    r#"{"@type":"messagePaymentRefunded","owner_id":{"@type":"messageSenderChat","chat_id":-2002},"currency":"USD","total_amount":1999,"invoice_payload":"","telegram_payment_charge_id":"c","provider_payment_charge_id":"p"}"#,
    "Launch Channel refunded USD 19.99"
);
wording!(
    premium_gifted_to_me,
    BY_DANA,
    r#"{"@type":"messageGiftedPremium","gifter_user_id":1,"receiver_user_id":9,"text":{"text":"","entities":[]},"currency":"USD","amount":999,"cryptocurrency":"","cryptocurrency_amount":0,"month_count":3,"day_count":0}"#,
    "Dana sent you a gift for USD 9.99"
);
wording!(
    premium_gifted_by_me,
    BY_ME,
    r#"{"@type":"messageGiftedPremium","gifter_user_id":9,"receiver_user_id":1,"currency":"USD","amount":999,"month_count":3}"#,
    "You sent a gift for USD 9.99"
);
wording!(
    premium_gift_code_from_giveaway,
    BY_DANA,
    r#"{"@type":"messagePremiumGiftCode","creator_id":{"@type":"messageSenderChat","chat_id":-2002},"text":{"text":"","entities":[]},"is_from_giveaway":true,"is_unclaimed":false,"currency":"USD","amount":0,"month_count":3,"code":"abc"}"#,
    "You won a prize in a giveaway organized by Launch Channel."
);
wording!(
    premium_gift_code_unclaimed,
    BY_DANA,
    r#"{"@type":"messagePremiumGiftCode","creator_id":{"@type":"messageSenderChat","chat_id":-2002},"is_from_giveaway":true,"is_unclaimed":true,"currency":"USD","amount":0,"month_count":3,"code":"abc"}"#,
    "You have an unclaimed prize from a giveaway by Launch Channel."
);
wording!(
    premium_gift_code_from_channel,
    BY_DANA,
    r#"{"@type":"messagePremiumGiftCode","creator_id":{"@type":"messageSenderChat","chat_id":-2002},"is_from_giveaway":false,"is_unclaimed":false,"currency":"USD","amount":0,"month_count":3,"code":"abc"}"#,
    "You've received a gift from Launch Channel."
);
wording!(
    giveaway_started_in_channel,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayCreated","star_count":0}"#,
    "Someone just started a giveaway of Telegram Premium subscriptions to its followers."
);
wording!(
    stars_giveaway_started,
    BY_DANA,
    r#"{"@type":"messageGiveawayCreated","star_count":500}"#,
    "Dana Cole just started a giveaway of 500 Stars to its members."
);
wording!(
    premium_giveaway_card,
    IN_CHANNEL,
    r#"{"@type":"messageGiveaway","parameters":{"@type":"giveawayParameters","boosted_chat_id":-2002},"winner_count":5,"prize":{"@type":"giveawayPrizePremium","month_count":3}}"#,
    "Giveaway: 5 Telegram Premium subscriptions for 3 months"
);
wording!(
    giveaway_completed,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayCompleted","giveaway_message_id":5,"winner_count":4,"is_star_giveaway":false,"unclaimed_prize_count":0}"#,
    "4 winners of the giveaway were randomly selected by Telegram and received private messages with giftcodes."
);
wording!(
    giveaway_completed_some_unclaimed,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayCompleted","giveaway_message_id":5,"winner_count":4,"is_star_giveaway":true,"unclaimed_prize_count":1}"#,
    "Some winners of the giveaway were randomly selected by Telegram and received their prize."
);
wording!(
    giveaway_without_winners,
    IN_CHANNEL,
    r#"{"@type":"messageGiveawayWinners","boosted_chat_id":-2002,"giveaway_message_id":5,"winner_count":0,"unclaimed_prize_count":0,"winner_user_ids":[],"prize":{"@type":"giveawayPrizePremium","month_count":3}}"#,
    "No winners of the giveaway could be selected."
);
wording!(
    stars_gifted_to_me,
    BY_DANA,
    r#"{"@type":"messageGiftedStars","gifter_user_id":2,"receiver_user_id":9,"currency":"USD","amount":499,"star_count":250,"transaction_id":"t"}"#,
    "Omar sent you a gift for USD 4.99"
);
wording!(
    grams_gifted_to_me,
    BY_DANA,
    r#"{"@type":"messageGiftedGrams","gifter_user_id":3,"receiver_user_id":9,"gram_amount":250,"transaction_id":"t"}"#,
    "Lea sent you a gift for 2.5 Grams"
);
wording!(
    giveaway_stars_prize,
    BY_DANA,
    r#"{"@type":"messageGiveawayPrizeStars","star_count":100,"transaction_id":"t","boosted_chat_id":-2002,"giveaway_message_id":5,"is_unclaimed":false}"#,
    "You won a prize in a giveaway organized by Launch Channel."
);
wording!(
    gift_received,
    BY_DANA,
    r#"{"@type":"messageGift","gift":{"@type":"gift","id":"1","star_count":100},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"prepaid_upgrade_star_count":0}"#,
    "Dana sent you a gift for 100 Stars"
);
wording!(
    gift_sent,
    BY_ME,
    r#"{"@type":"messageGift","gift":{"@type":"gift","id":"1","star_count":1},"sender_id":{"@type":"messageSenderUser","user_id":9},"receiver_id":{"@type":"messageSenderUser","user_id":1}}"#,
    "You sent a gift for 1 Star"
);
wording!(
    gift_prepaid_upgrade,
    BY_DANA,
    r#"{"@type":"messageGift","gift":{"@type":"gift","id":"1","star_count":100},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"is_prepaid_upgrade":true,"prepaid_upgrade_star_count":300}"#,
    "Dana sent an upgrade worth 300 Stars for your gift."
);
wording!(
    gift_upgraded_by_me,
    BY_ME,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"origin":{"@type":"upgradedGiftOriginUpgrade","gift_message_id":0}}"#,
    "You turned the gift from Dana Cole into a unique collectible"
);
wording!(
    gift_upgraded_by_sender_for_me,
    BY_DANA,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":9},"receiver_id":{"@type":"messageSenderUser","user_id":1},"origin":{"@type":"upgradedGiftOriginUpgrade","gift_message_id":0}}"#,
    "Dana Cole turned the gift from you into a unique collectible"
);
wording!(
    gift_transferred_to_me,
    BY_DANA,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"origin":{"@type":"upgradedGiftOriginTransfer"}}"#,
    "Dana Cole transferred you a gift"
);
wording!(
    gift_resold_to_me,
    BY_DANA,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"origin":{"@type":"upgradedGiftOriginResale","price":{"@type":"giftResalePriceStar","star_count":900}}}"#,
    "Dana Cole sold you a gift for 900 Stars"
);
wording!(
    gift_crafted,
    BY_ME,
    r#"{"@type":"messageUpgradedGift","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"origin":{"@type":"upgradedGiftOriginCraft"}}"#,
    "You crafted a new gift"
);
wording!(
    gift_refunded,
    BY_DANA,
    r#"{"@type":"messageRefundedUpgradedGift","gift":{"@type":"gift","id":"1"},"origin":{"@type":"upgradedGiftOriginUpgrade","gift_message_id":0}}"#,
    "This gift was downgraded because a request to refund the payment related to this gift was made, and the money was returned."
);
wording!(
    gift_offer_pending,
    BY_DANA,
    r#"{"@type":"messageUpgradedGiftPurchaseOffer","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"state":{"@type":"giftPurchaseOfferStatePending"},"price":{"@type":"giftResalePriceStar","star_count":700},"expiration_date":1}"#,
    "An offer to buy this gift for 700 Stars."
);
wording!(
    gift_offer_accepted,
    BY_DANA,
    r#"{"@type":"messageUpgradedGiftPurchaseOffer","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"state":{"@type":"giftPurchaseOfferStateAccepted"},"price":{"@type":"giftResalePriceStar","star_count":700},"expiration_date":1}"#,
    "This offer was accepted."
);
wording!(
    gift_offer_rejected,
    BY_DANA,
    r#"{"@type":"messageUpgradedGiftPurchaseOfferRejected","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"price":{"@type":"giftResalePriceStar","star_count":700},"offer_message_id":5,"was_expired":false}"#,
    "Dana Cole rejected your offer to buy Plush Pepe #4 for 700 Stars."
);
wording!(
    gift_offer_expired,
    BY_ME,
    r#"{"@type":"messageUpgradedGiftPurchaseOfferRejected","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":4},"price":{"@type":"giftResalePriceStar","star_count":700},"offer_message_id":5,"was_expired":true}"#,
    "Your offer to buy Plush Pepe #4 for 700 Stars has expired."
);
wording!(
    paid_messages_refunded,
    BY_DANA,
    r#"{"@type":"messagePaidMessagesRefunded","message_count":2,"star_count":20}"#,
    "Dana Cole refunded 20 Stars to you"
);
wording!(
    paid_message_price_set,
    BY_DANA,
    r#"{"@type":"messagePaidMessagePriceChanged","paid_message_star_count":5}"#,
    "Messages now cost 5 Stars each in this group."
);
wording!(
    paid_message_price_free,
    BY_DANA,
    r#"{"@type":"messagePaidMessagePriceChanged","paid_message_star_count":0}"#,
    "Messages are now free in this group."
);
wording!(
    direct_messages_paid,
    IN_CHANNEL,
    r#"{"@type":"messageDirectMessagePriceChanged","is_enabled":true,"paid_message_star_count":10}"#,
    "Channel allows Direct Messages for 10 Stars each."
);
wording!(
    direct_messages_disabled,
    IN_CHANNEL,
    r#"{"@type":"messageDirectMessagePriceChanged","is_enabled":false,"paid_message_star_count":0}"#,
    "Channel disabled Direct Messages."
);
wording!(
    checklist_tasks_done,
    BY_DANA,
    r#"{"@type":"messageChecklistTasksDone","checklist_message_id":5,"marked_as_done_task_ids":[1,2],"marked_as_not_done_task_ids":[]}"#,
    "Dana Cole marked 2 tasks as done."
);
wording!(
    checklist_task_undone,
    BY_ME,
    r#"{"@type":"messageChecklistTasksDone","checklist_message_id":5,"marked_as_done_task_ids":[],"marked_as_not_done_task_ids":[1]}"#,
    "You marked task as not done."
);
wording!(
    checklist_tasks_added,
    BY_DANA,
    r#"{"@type":"messageChecklistTasksAdded","checklist_message_id":5,"tasks":[{"@type":"checklistTask","id":3,"text":{"text":"Buy milk","entities":[]}},{"@type":"checklistTask","id":4,"text":{"text":"Pack bags","entities":[]}}]}"#,
    "Dana Cole added Buy milk and Pack bags to the list."
);
wording!(
    poll_option_added,
    BY_DANA,
    r#"{"@type":"messagePollOptionAdded","poll_message_id":5,"option_id":"3","text":{"text":"Pizza","entities":[]}}"#,
    "Dana Cole added \"Pizza\" to the poll."
);
wording!(
    poll_option_removed,
    BY_ME,
    r#"{"@type":"messagePollOptionDeleted","poll_message_id":5,"option_id":"3","text":{"text":"Pizza","entities":[]}}"#,
    "You removed \"Pizza\" from the poll."
);
wording!(
    suggested_post_approval_failed,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostApprovalFailed","suggested_post_message_id":5,"price":{"@type":"suggestedPostPriceStar","star_count":10}}"#,
    "Transaction failed."
);
wording!(
    suggested_post_approved,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostApproved","suggested_post_message_id":5,"price":{"@type":"suggestedPostPriceStar","star_count":10},"send_date":1}"#,
    "Agreement reached!"
);
wording!(
    suggested_post_declined,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostDeclined","suggested_post_message_id":5,"comment":""}"#,
    "The post was rejected."
);
wording!(
    suggested_post_paid,
    BY_DANA,
    r#"{"@type":"messageSuggestedPostPaid","suggested_post_message_id":5,"star_amount":{"@type":"starAmount","star_count":40,"nanostar_count":0},"gram_amount":0}"#,
    "Dana Cole has received 40 Stars for publishing post."
);
wording!(
    suggested_post_refunded_user,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostRefunded","suggested_post_message_id":5,"reason":{"@type":"suggestedPostRefundReasonPaymentRefunded"}}"#,
    "User refunded the Stars so that post was deleted."
);
wording!(
    suggested_post_refunded_admin,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostRefunded","suggested_post_message_id":5,"reason":{"@type":"suggestedPostRefundReasonPostDeleted"}}"#,
    "Admin deleted the post early so that the price was refunded to the user."
);
wording!(
    contact_registered,
    BY_DANA,
    r#"{"@type":"messageContactRegistered"}"#,
    "Dana Cole joined Telegram"
);
wording!(
    users_shared,
    BY_ME,
    r#"{"@type":"messageUsersShared","users":[{"@type":"sharedUser","user_id":2,"first_name":"Omar","last_name":"Haddad"},{"@type":"sharedUser","user_id":3,"first_name":"Lea","last_name":"Stern"}],"button_id":1}"#,
    "You shared Omar Haddad and Lea Stern with Design Club"
);
wording!(
    chat_shared,
    BY_ME,
    r#"{"@type":"messageChatShared","chat":{"@type":"sharedChat","chat_id":-2002,"title":"Launch Channel"},"button_id":1}"#,
    "You shared Launch Channel with Design Club"
);
wording!(
    bot_allowed_on_website,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonConnectedWebsite","domain_name":"example.org"}}"#,
    "You allowed this bot to message you when you logged in on example.org."
);
wording!(
    bot_allowed_from_attach_menu,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonAddedToAttachmentMenu"}}"#,
    "You allowed this bot to message you when you added it to your attachment menu."
);
wording!(
    bot_allowed_from_web_app,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonLaunchedWebApp","web_app":{"@type":"webApp","short_name":"shop","title":"Shop"}}}"#,
    "You allowed this bot to message you when you opened Shop."
);
wording!(
    bot_allowed_by_request,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonAcceptedRequest"}}"#,
    "You allowed this bot to message you in its web-app."
);
wording!(
    web_app_data_sent,
    BY_ME,
    r#"{"@type":"messageWebAppDataSent","button_text":"Order"}"#,
    "Data from the \"Order\" button was transferred to the bot."
);
wording!(
    web_app_data_received,
    BY_DANA,
    r#"{"@type":"messageWebAppDataReceived","button_text":"Order","data":"{}"}"#,
    "Data from the \"Order\" button was received."
);
wording!(
    passport_data_sent,
    BY_ME,
    r#"{"@type":"messagePassportDataSent","types":[{"@type":"passportElementTypePersonalDetails"},{"@type":"passportElementTypeIdentityCard"},{"@type":"passportElementTypeBankStatement"},{"@type":"passportElementTypePhoneNumber"}]}"#,
    "Design Club received the following documents: personal details, proof of identity, proof of address, phone number"
);
wording!(
    passport_data_received,
    BY_DANA,
    r#"{"@type":"messagePassportDataReceived","elements":[],"credentials":{}}"#,
    "Telegram Passport data received"
);
wording!(
    proximity_traveler_is_me,
    BY_ME,
    r#"{"@type":"messageProximityAlertTriggered","traveler_id":{"@type":"messageSenderUser","user_id":9},"watcher_id":{"@type":"messageSenderUser","user_id":1},"distance":250}"#,
    "You are now within 250 meters from Dana Cole"
);
wording!(
    proximity_watcher_is_me,
    BY_DANA,
    r#"{"@type":"messageProximityAlertTriggered","traveler_id":{"@type":"messageSenderUser","user_id":1},"watcher_id":{"@type":"messageSenderUser","user_id":9},"distance":1500}"#,
    "Dana Cole is now within 1.5 km from you"
);
wording!(
    proximity_between_others,
    BY_DANA,
    r#"{"@type":"messageProximityAlertTriggered","traveler_id":{"@type":"messageSenderUser","user_id":1},"watcher_id":{"@type":"messageSenderUser","user_id":2},"distance":1}"#,
    "Dana Cole is now within 1 meter from Omar Haddad"
);
wording!(
    video_chat_started,
    BY_DANA,
    r#"{"@type":"messageVideoChatStarted","group_call_id":5}"#,
    "Dana Cole started a video chat"
);
wording!(
    live_stream_started,
    IN_CHANNEL,
    r#"{"@type":"messageVideoChatStarted","group_call_id":5}"#,
    "Live stream started"
);
wording!(
    video_chat_ended,
    BY_DANA,
    r#"{"@type":"messageVideoChatEnded","duration":3725}"#,
    "Dana Cole ended the video chat (62 minutes)"
);
wording!(
    video_chat_ended_short,
    BY_DANA,
    r#"{"@type":"messageVideoChatEnded","duration":45}"#,
    "Dana Cole ended the video chat (45 seconds)"
);
wording!(
    live_stream_finished,
    IN_CHANNEL,
    r#"{"@type":"messageVideoChatEnded","duration":7300}"#,
    "Live stream finished (2 hours)"
);
wording!(
    video_chat_invite,
    BY_DANA,
    r#"{"@type":"messageInviteVideoChatParticipants","group_call_id":5,"user_ids":[2,3]}"#,
    "Dana Cole invited Omar Haddad and Lea Stern to the video chat"
);
wording!(
    expired_photo,
    BY_DANA,
    r#"{"@type":"messageExpiredPhoto"}"#,
    "Expired photo"
);
wording!(
    expired_video,
    BY_DANA,
    r#"{"@type":"messageExpiredVideo"}"#,
    "Expired video"
);
wording!(
    expired_round_video,
    BY_DANA,
    r#"{"@type":"messageExpiredVideoNote"}"#,
    "Round message expired"
);
wording!(
    expired_voice_message,
    BY_DANA,
    r#"{"@type":"messageExpiredVoiceNote"}"#,
    "Voice message expired"
);
wording!(
    stake_dice_won,
    BY_DANA,
    r#"{"@type":"messageStakeDice","value":6,"stake_gram_amount":100,"prize_gram_amount":300}"#,
    "Dana Cole won 2 Grams"
);
wording!(
    stake_dice_lost,
    BY_ME,
    r#"{"@type":"messageStakeDice","value":1,"stake_gram_amount":100,"prize_gram_amount":0}"#,
    "You lost 1 Gram"
);
wording!(
    story_mention,
    BY_DANA,
    r#"{"@type":"messageStory","story_poster_chat_id":1,"story_id":4,"via_mention":true}"#,
    "Mentioned in a story"
);
wording!(
    paid_media,
    BY_DANA,
    r#"{"@type":"messagePaidMedia","star_count":25,"media":[],"caption":{"text":"","entities":[]}}"#,
    "Paid media \u{B7} 25 Stars"
);
wording!(
    auto_delete_set_by_other,
    BY_DANA,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":86400,"from_user_id":1}"#,
    "Dana Cole set messages to auto-delete in 1 day"
);
wording!(
    auto_delete_set_by_me,
    BY_ME,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":604800,"from_user_id":9}"#,
    "You set messages to auto-delete in 1 week"
);
wording!(
    auto_delete_off,
    BY_DANA,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":0,"from_user_id":1}"#,
    "Dana Cole disabled the auto-delete timer"
);
wording!(
    auto_delete_in_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":3600,"from_user_id":0}"#,
    "Messages in this channel will be automatically deleted after 1 hour"
);
wording!(
    screenshot_by_other,
    BY_DANA,
    r#"{"@type":"messageScreenshotTaken"}"#,
    "Dana Cole took a screenshot!"
);
wording!(
    screenshot_by_me,
    BY_ME,
    r#"{"@type":"messageScreenshotTaken"}"#,
    "You took a screenshot!"
);

#[test]
fn video_chat_scheduled_says_today_tomorrow_or_the_date() {
    let at = |offset: i64| {
        let start = 1_800_000_000 + offset;
        say(
            &format!(
                r#"{{"@type":"messageVideoChatScheduled","group_call_id":5,"start_date":{start}}}"#
            ),
            BY_DANA,
        )
    };
    assert!(at(60).starts_with("Dana Cole scheduled a video chat for today at "));
    assert!(at(86_400 + 60).starts_with("Dana Cole scheduled a video chat for tomorrow at "));
    let later = at(9 * 86_400);
    assert!(later.starts_with("Dana Cole scheduled a video chat for "));
    assert!(!later.contains("today") && !later.contains("tomorrow"));
    assert!(later.contains(" at "));
    let channel = say(
        r#"{"@type":"messageVideoChatScheduled","group_call_id":5,"start_date":1800000060}"#,
        IN_CHANNEL,
    );
    assert!(channel.starts_with("Live stream scheduled for today at "));
}

#[test]
fn actor_and_target_names_are_links_to_their_profiles() {
    let text = run(
        r#"{"@type":"messageChatAddMembers","member_user_ids":[2,3]}"#,
        BY_DANA,
    );
    let (plain, links) = text.spans();
    assert_eq!(plain, "Dana Cole added Omar Haddad and Lea Stern");
    let linked: Vec<(&str, &ServiceLink)> = links
        .iter()
        .map(|(range, link)| (&plain[range.clone()], link))
        .collect();
    assert_eq!(
        linked,
        vec![
            (
                "Dana Cole",
                &ServiceLink::Sender(MessageSender::User { user_id: DANA })
            ),
            (
                "Omar Haddad",
                &ServiceLink::Sender(MessageSender::User { user_id: OMAR })
            ),
            (
                "Lea Stern",
                &ServiceLink::Sender(MessageSender::User { user_id: LEA })
            ),
        ]
    );
}

#[test]
fn pinned_excerpt_links_to_the_pinned_message() {
    let text = run(r#"{"@type":"messagePinMessage","message_id":100}"#, BY_DANA);
    let (plain, links) = text.spans();
    let (range, link) = links.last().expect("a link for the excerpt");
    assert_eq!(&plain[range.clone()], "Lunch at noon, b\u{2026}");
    assert_eq!(link, &ServiceLink::Message(100));
}

#[test]
fn game_score_title_links_to_the_game_message() {
    let text = run(
        r#"{"@type":"messageGameScore","game_message_id":300,"game_id":"5","score":9}"#,
        BY_DANA,
    );
    let (_, links) = text.spans();
    assert_eq!(links.last().unwrap().1, ServiceLink::Message(300));
}

#[test]
fn unknown_users_read_as_someone_instead_of_a_blank() {
    assert_eq!(
        say(
            r#"{"@type":"messageChatDeleteMember","user_id":77}"#,
            BY_DANA
        ),
        "Dana Cole removed Someone"
    );
}

#[test]
fn bot_website_domain_is_an_external_link() {
    let text = run(
        r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonConnectedWebsite","domain_name":"example.org"}}"#,
        BY_ME,
    );
    let (_, links) = text.spans();
    assert_eq!(links[0].1, ServiceLink::Url("http://example.org".into()));
}

#[test]
fn chat_photo_change_keeps_the_new_photo_for_the_thumbnail() {
    let json = r#"{"@type":"messageChatChangePhoto","photo":{"@type":"chatPhoto","id":"5","added_date":1,"minithumbnail":{"@type":"minithumbnail","width":8,"height":8,"data":"AQID"},"sizes":[{"@type":"photoSize","type":"a","photo":{"@type":"file","id":77,"size":900,"expected_size":900,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"y","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":900}},"width":160,"height":160}]}}"#;
    let (content, files) = parse(json);
    let MessageContent::Action(action) = content else {
        panic!("a service action");
    };
    let ServiceAction::ChatPhoto { photo: Some(photo) } = *action else {
        panic!("a chat photo with sizes");
    };
    assert_eq!(photo.sizes.len(), 1);
    assert_eq!(photo.sizes[0].file_id.0, 77);
    assert!(photo.minithumbnail.is_some());
    assert_eq!(files.len(), 1);
}

#[test]
fn ordinary_content_is_not_a_service_message() {
    let (content, _) = parse(r#"{"@type":"messageText","text":{"text":"hi","entities":[]}}"#);
    let ctx = ServiceCtx {
        actor: None,
        is_outgoing: false,
        my_id: None,
        is_channel: false,
        is_secret: false,
        chat_id: CHAT,
        now: 0,
        names: &Names,
    };
    assert!(render(&content, &ctx).is_none());
}

#[test]
fn unknown_constructors_stay_unsupported() {
    let (content, _) = parse(r#"{"@type":"messageFromTheFuture"}"#);
    assert!(matches!(content, MessageContent::Unsupported { .. }));
    let (content, _) = parse(r#"{"@type":"messageUnsupported"}"#);
    assert!(matches!(content, MessageContent::Unsupported { .. }));
}

#[test]
fn nameless_preview_never_says_loading() {
    let (content, _) = parse(r#"{"@type":"messagePinMessage","message_id":5}"#);
    assert_eq!(content.preview(), "Someone pinned a message");
}

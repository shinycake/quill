//! Sweep fixes (r4-02, fix round 3): a send that never left ends a stage in
//! the translated line that names the cause, not in English literals.
use super::sweep_s3_payments_1::Flaky;
use crate::connect::ConnectSendError;

fn no_connection() -> String {
    ConnectSendError::Native.stage_line()
}

#[test]
fn a_refused_contacts_fetch_shows_the_send_line() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getContacts"]);
    assert!(t.driver.fetch_contacts().is_err());
    assert_eq!(
        t.driver.session.users_state.contacts.error(),
        Some(no_connection().as_str())
    );
}

#[test]
fn a_refused_mask_list_shows_the_send_line() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getInstalledStickerSets"]);
    assert!(t.driver.open_mask_sets().is_err());
    assert_eq!(
        t.driver.session.stickers.masks.installed.error(),
        Some(no_connection().as_str())
    );
}

#[test]
fn a_refused_forum_topic_icons_send_shows_the_send_line() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getForumTopicDefaultIcons"]);
    assert!(t.driver.load_forum_topic_icons().is_err());
    assert_eq!(
        t.driver.session.threads.forum_topic_icons.error(),
        Some(no_connection().as_str())
    );
}

#[test]
fn a_refused_chat_links_and_chatbot_send_show_the_send_line() {
    let mut t = Flaky::new();
    t.sender
        .refuse(&["getBusinessChatLinks", "getBusinessConnectedBot"]);
    assert!(t.driver.ensure_chat_links().is_err());
    assert!(t.driver.ensure_connected_bot().is_err());
    let b = &t.driver.session.settings.business_extra;
    assert_eq!(b.chat_links.error(), Some(no_connection().as_str()));
    assert_eq!(b.connected_bot.error(), Some(no_connection().as_str()));
}

#[test]
fn a_refused_premium_options_send_shows_the_send_line() {
    use crate::gift_manage::GiftListFetch;
    let mut t = Flaky::new();
    t.feed(serde_json::json!({"@type":"updateNewChat","chat":{"id":21,"title":"Ann","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}));
    t.sender.refuse(&["getPremiumGiftPaymentOptions"]);
    let _ = t.driver.open_send_gift(crate::ids::ChatId(21));
    assert_eq!(
        t.driver.session.payments.gifts_manage.send.premium_fetch,
        GiftListFetch::Failed(no_connection())
    );
}

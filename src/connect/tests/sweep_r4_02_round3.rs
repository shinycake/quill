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

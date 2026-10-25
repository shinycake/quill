//! Driver tests: gifts for a user picked by id (the birthday suggestion and
//! the gift recipient list). Nothing is bought.
use super::paid_messages::{Fixture, cleanup, fixture, ingest, requests};
use crate::telegram::envelope::MessageSender;

fn user(f: &mut Fixture, id: i64, bot: bool) {
    let kind = if bot {
        "userTypeBot"
    } else {
        "userTypeRegular"
    };
    ingest(
        f,
        &format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"Ada","last_name":"Lovelace","is_contact":true,"type":{{"@type":"{kind}"}}}}}}"#
        ),
    );
}

#[test]
fn a_contact_without_a_chat_gets_the_gift_picker() {
    let mut f = fixture();
    user(&mut f, 901, false);
    f.1.open_send_gift_to_user(901).unwrap();
    let send = &f.1.session.payments.gifts_manage.send;
    assert!(send.open);
    let recipient = send.recipient.as_ref().expect("a recipient");
    assert_eq!(recipient.sender, MessageSender::User { user_id: 901 });
    assert_eq!(recipient.name, "Ada Lovelace");
    assert_eq!(requests(&f, "getAvailableGifts").len(), 1);
    assert_eq!(requests(&f, "getPremiumGiftPaymentOptions").len(), 1);
    assert!(requests(&f, "createPrivateChat").is_empty());
    cleanup(f);
}

#[test]
fn bots_and_unknown_users_get_no_gift_picker() {
    let mut f = fixture();
    user(&mut f, 902, true);
    assert!(f.1.open_send_gift_to_user(902).is_err());
    assert!(f.1.open_send_gift_to_user(903).is_err());
    assert!(!f.1.session.payments.gifts_manage.send.open);
    assert!(requests(&f, "getAvailableGifts").is_empty());
    cleanup(f);
}

#[test]
fn the_recipient_list_loads_contacts_and_frequent_contacts() {
    let mut f = fixture();
    f.1.load_gift_recipients().unwrap();
    assert_eq!(requests(&f, "getContacts").len(), 1);
    assert_eq!(requests(&f, "getTopChats").len(), 1);
    cleanup(f);
}

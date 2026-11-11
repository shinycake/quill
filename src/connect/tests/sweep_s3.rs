//! Sweep fixes (s3-links-tdlib): connect-driver behaviour that needs a
//! sender which can refuse requests.
use super::support::toggle_driver;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::UsernameCheckResult;

/// Sweep 02-037: a send error in a later step of an ingest does not lose
/// the answer captured from the same envelope.
#[test]
fn a_send_error_later_in_the_ingest_keeps_the_captured_answer() {
    let (_dir, mut driver, sender, sink, seq) = toggle_driver();
    driver.session.my_user_id = Some(7);
    let extra = driver.check_username("quill").expect("sent");
    // A step that runs in every ingest and must send (it fails below):
    // the open supergroup's membership probe.
    let chat = r#"{"@type":"updateNewChat","chat":{"id":-1001,"title":"G","type":{"@type":"chatTypeSupergroup","supergroup_id":1,"is_channel":false}}}"#;
    driver
        .ingest(copy_and_parse(chat, &seq, &sink).expect("parse"))
        .expect("chat");
    driver.session.open_chat = Some(crate::ids::ChatId(-1001));
    sender.refuse(true);
    let json = format!(
        r#"{{"@type":"checkChatUsernameResultOk","@extra":"{}"}}"#,
        extra.as_extra()
    );
    let result = driver.ingest(copy_and_parse(&json, &seq, &sink).expect("parse"));
    assert!(result.is_err(), "the later send really failed");
    assert_eq!(
        driver.session.users_state.username_check,
        Some(("quill".to_string(), UsernameCheckResult::Available)),
        "the verdict is stashed before the failing step"
    );
}

/// Sweep 04-041: a name whose request never left is not marked asked, so
/// the row does not keep its placeholder for the session.
#[test]
fn a_grams_peer_request_that_never_left_is_asked_again() {
    use crate::premium_hub::ton::{TonTx, TonTxKind};
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.payments.hub.grams.entries.push(TonTx {
        id: "t1".into(),
        key: "t1".into(),
        gram_nano: 1,
        is_refund: false,
        date: 1,
        kind: TonTxKind::SuggestedPostPayment { chat_id: 13 },
    });
    driver.session.payments.hub.grams.peers_dirty = true;
    sender.refuse(true);
    assert!(!driver.grams_tick(false), "nothing left");
    assert!(driver.session.payments.hub.grams.asked_chats.is_empty());
    sender.refuse(false);
    assert!(driver.grams_tick(false), "asked on the next tick");
    assert!(
        sender
            .snapshot()
            .iter()
            .any(|json| json.contains(r#""@type":"getChat""#))
    );
    assert!(!driver.grams_tick(false), "and only once");
}

/// Sweep 02-212: one failed `getSecretChat` send does not drop the ids
/// queued behind it.
#[test]
fn a_failed_secret_chat_send_keeps_the_rest_of_the_queue() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.users_state.secret_chat_fetch_queue = vec![1, 2, 3];
    sender.refuse(true);
    assert!(driver.maybe_fetch_secret_chat_states().is_err());
    assert_eq!(
        driver.session.users_state.secret_chat_fetch_queue,
        vec![1, 2, 3]
    );
    sender.refuse(false);
    assert!(driver.maybe_fetch_secret_chat_states().is_ok());
    assert_eq!(
        sender
            .snapshot()
            .iter()
            .filter(|json| json.contains(r#""@type":"getSecretChat""#))
            .count(),
        3
    );
}

/// Sweep 09-010: a flow whose account left the screen (its farewell ends
/// it) drops the answer that lands in the background, so switching back
/// opens nothing by itself.
#[test]
fn a_deep_link_answer_after_the_flow_ended_is_dropped() {
    use crate::state::DeepLinkAction;
    let (_dir, mut driver, _sender, sink, seq) = toggle_driver();
    let extra = driver
        .resolve_deep_link(DeepLinkAction::OpenUsername {
            domain: "durov".into(),
            start_param: None,
            post: None,
            story_id: None,
        })
        .expect("sent")
        .expect("a request");
    driver.session.chats_state.end_deep_link_flow();
    let json = format!(
        r#"{{"@type":"chat","@extra":"{}","id":42,"title":"Durov","type":{{"@type":"chatTypePrivate","user_id":42}},"unread_count":0}}"#,
        extra.as_extra()
    );
    driver
        .ingest(copy_and_parse(&json, &seq, &sink).expect("parse"))
        .expect("ingest");
    assert_eq!(driver.session.chats_state.deep_link, None);
}

/// Sweep 05-231: a calendar's generation never repeats after the box
/// closes, so an earlier calendar's late page cannot match the next one.
#[test]
fn a_reopened_calendar_has_a_new_generation() {
    let (_dir, mut driver, _sender, _sink, _seq) = toggle_driver();
    driver.session.open_chat = Some(crate::ids::ChatId(5));
    let first = driver
        .session
        .open_history_calendar(0)
        .map(|calendar| calendar.generation);
    driver.session.close_history_calendar();
    let second = driver
        .session
        .open_history_calendar(0)
        .map(|calendar| calendar.generation);
    assert!(first.is_some() && second.is_some());
    assert_ne!(first, second);
}

/// Sweep 06-005: a deletion in another chat does not make the search
/// jump's message missing (ids repeat across channels).
#[test]
fn a_deletion_in_another_chat_leaves_the_search_jump_alone() {
    use crate::ids::{ChatId, MessageId};
    use crate::state::ChatSearchJump;
    let (_dir, mut driver, _sender, _sink, _seq) = toggle_driver();
    let search = &mut driver.session.search.chat_search;
    search.chat_id = Some(ChatId(1));
    search.jump = ChatSearchJump::Ready {
        message_id: MessageId(77),
    };
    driver.session.miss_search_jump(ChatId(2), &[MessageId(77)]);
    assert!(matches!(
        driver.session.search.chat_search.jump,
        ChatSearchJump::Ready { .. }
    ));
    driver.session.miss_search_jump(ChatId(1), &[MessageId(77)]);
    assert!(matches!(
        driver.session.search.chat_search.jump,
        ChatSearchJump::Missing { .. }
    ));
}

const NIGHT: &str = r#"{"@type":"updateDefaultBackground","for_dark_theme":true,"background":{"@type":"background","id":6,"is_default":true,"is_dark":true,"name":"night","type":{"@type":"backgroundTypeFill","fill":{"@type":"backgroundFillSolid","color":1}}}}"#;
const NIGHT_GONE: &str =
    r#"{"@type":"updateDefaultBackground","for_dark_theme":true,"background":null}"#;

/// Sweep 06-032: a default wallpaper removed on another device (a null
/// background) is removed here too, instead of the update failing to parse.
#[test]
fn a_removed_default_wallpaper_is_removed_here_too() {
    let (_dir, mut driver, _sender, sink, seq) = toggle_driver();
    let feed = |driver: &mut crate::connect::ConnectDriver<_>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).expect("parses"))
            .expect("ingest");
    };
    feed(&mut driver, NIGHT);
    assert!(
        driver
            .session
            .chats_state
            .default_backgrounds
            .contains_key(&true)
    );
    feed(&mut driver, NIGHT_GONE);
    assert!(driver.session.chats_state.default_backgrounds.is_empty());
}

/// Sweep 06-032 / 02-087: the default wallpaper leaves only when TDLib
/// confirms its delete, so a refusal keeps it.
#[test]
fn a_refused_default_wallpaper_delete_keeps_it() {
    let (_dir, mut driver, _sender, sink, seq) = toggle_driver();
    let feed = |driver: &mut crate::connect::ConnectDriver<_>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).expect("parses"))
            .expect("ingest");
    };
    feed(&mut driver, NIGHT);
    let delete = driver.delete_default_background(true).expect("sent");
    assert!(
        driver
            .session
            .chats_state
            .default_backgrounds
            .contains_key(&true),
        "still shown while the delete is on its way"
    );
    feed(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BACKGROUND_INVALID"}}"#,
            delete.as_extra()
        ),
    );
    assert!(
        driver
            .session
            .chats_state
            .default_backgrounds
            .contains_key(&true)
    );
}

//! Connect-driver tests: work that follows an answer runs for the chat
//! window that asked (jump to date, the corner buttons, reply strips),
//! not for the main window's chat.
use super::chat_window::{ALICE, BOB, Harness, message_json};
use crate::ids::{MessageId, RequestId};
use crate::state::{ChatSearchJump, UnreadJumpKind};

/// Bob in the main window with messages 1..=10, Alice in her own window
/// with 50..=60.
fn bob_main_alice_window() -> Harness {
    let mut h = Harness::two_chats();
    let main_page = h.driver.select_chat(BOB).unwrap().unwrap();
    h.answer(BOB, main_page, &(1..=10).collect::<Vec<_>>());
    let window_page = h.driver.open_chat_window(ALICE).unwrap().unwrap();
    h.answer(ALICE, window_page, &(50..=60).collect::<Vec<_>>());
    h
}

/// The `@extra` of the last `kind` request sent for `chat`.
fn last_extra(h: &Harness, kind: &str, chat: crate::ids::ChatId) -> RequestId {
    let sent = h.sent(kind, chat);
    let extra = sent.last().expect("sent")["@extra"]
        .as_str()
        .expect("extra")
        .parse()
        .expect("number");
    RequestId(extra)
}

/// A `message` answer to `extra`.
fn message_answer(extra: RequestId, chat: i64, id: i64) -> String {
    message_json(chat, id, false).replacen(
        r#""@type":"message","#,
        &format!(r#""@type":"message","@extra":"{}","#, extra.0),
        1,
    )
}

fn pick_date_in_alice_window(h: &mut Harness) -> RequestId {
    h.driver.with_window_chat(ALICE, |driver| {
        driver.session.open_history_calendar(1_700_000_000);
        driver
            .jump_to_date(19_000)
            .unwrap()
            .expect("getChatMessageByDate")
    })
}

#[test]
fn a_date_picked_in_a_chat_window_jumps_that_window() {
    let mut h = bob_main_alice_window();
    let bob_pages = h.sent("getChatHistory", BOB).len();
    let asked = pick_date_in_alice_window(&mut h);
    // The answer lands while the main window's view is in place.
    h.ingest(&message_answer(asked, ALICE.0, 20));

    // Alice's window loads around the day; Bob is not touched.
    let around = h.sent("getChatHistory", ALICE);
    assert_eq!(around.last().unwrap()["from_message_id"], 20);
    assert_eq!(h.sent("getChatHistory", BOB).len(), bob_pages);
    assert!(h.history(BOB).contains(MessageId(10)), "Bob kept his page");

    // The page lands in Alice's view and the jump settles there, on the
    // day's first message (the one after the answer).
    let page = last_extra(&h, "getChatHistory", ALICE);
    h.answer(ALICE, page, &(15..=25).collect::<Vec<_>>());
    let jump = h
        .driver
        .session
        .with_view(Some(ALICE), |session| session.search.chat_search.jump);
    assert_eq!(
        jump,
        Some(ChatSearchJump::Ready {
            message_id: MessageId(21)
        })
    );
    assert!(
        !matches!(
            h.driver.session.search.chat_search.jump,
            ChatSearchJump::Ready { .. }
        ),
        "the main window's view does not jump"
    );
}

#[test]
fn a_date_answer_for_a_chat_the_main_window_left_is_dropped() {
    let mut h = Harness::two_chats();
    let page = h.driver.select_chat(ALICE).unwrap().unwrap();
    h.answer(ALICE, page, &(50..=60).collect::<Vec<_>>());
    h.driver.session.open_history_calendar(1_700_000_000);
    let asked = h.driver.jump_to_date(19_000).unwrap().unwrap();
    let page = h.driver.select_chat(BOB).unwrap().unwrap();
    h.answer(BOB, page, &(1..=10).collect::<Vec<_>>());
    let pages = (
        h.sent("getChatHistory", ALICE).len(),
        h.sent("getChatHistory", BOB).len(),
    );

    h.ingest(&message_answer(asked, ALICE.0, 20));
    assert_eq!(
        (
            h.sent("getChatHistory", ALICE).len(),
            h.sent("getChatHistory", BOB).len(),
        ),
        pages,
        "Bob is not reset around Alice's message id"
    );
    assert!(h.history(BOB).contains(MessageId(10)));
}

#[test]
fn a_date_answer_for_a_closed_window_is_dropped() {
    let mut h = bob_main_alice_window();
    let asked = pick_date_in_alice_window(&mut h);
    h.driver.close_chat_window(ALICE).unwrap();
    let bob_pages = h.sent("getChatHistory", BOB).len();
    h.ingest(&message_answer(asked, ALICE.0, 20));
    assert_eq!(h.sent("getChatHistory", BOB).len(), bob_pages);
    assert!(h.history(BOB).contains(MessageId(10)));
}

#[test]
fn the_mention_button_in_a_chat_window_jumps_that_window() {
    let mut h = bob_main_alice_window();
    let bob_pages = h.sent("getChatHistory", BOB).len();
    h.driver
        .with_window_chat(ALICE, |driver| {
            driver.jump_to_unread_marker(UnreadJumpKind::Mention)
        })
        .unwrap();
    let asked = last_extra(&h, "searchChatMessages", ALICE);
    h.ingest(&format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{},{}]}}"#,
        asked.0,
        message_json(ALICE.0, 30, false),
        message_json(ALICE.0, 20, false)
    ));
    // The oldest unread mention, 20, is outside Alice's page: her window
    // loads around it, and Bob's is left alone.
    let around = h.sent("getChatHistory", ALICE);
    assert_eq!(around.last().unwrap()["from_message_id"], 20);
    assert_eq!(h.sent("getChatHistory", BOB).len(), bob_pages);
}

#[test]
fn a_reply_in_a_chat_window_fetches_its_original() {
    let mut h = bob_main_alice_window();
    h.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":61,"chat_id":{},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"late answer","entities":[]}}}},"reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":3,"quote":null,"checklist_task_id":0,"poll_option_id":"","origin":null,"origin_send_date":0,"content":null}}}}}}"#,
        ALICE.0, ALICE.0
    ));
    let asked = h.sent("getRepliedMessage", ALICE);
    assert_eq!(asked.len(), 1, "the strip asks for the original");
    assert_eq!(asked[0]["message_id"], 61);
}

#[test]
fn the_calendar_goes_when_its_chat_is_left_and_a_stale_one_jumps_nowhere() {
    let mut h = Harness::two_chats();
    let page = h.driver.select_chat(ALICE).unwrap().unwrap();
    h.answer(ALICE, page, &(50..=60).collect::<Vec<_>>());
    h.driver.session.open_history_calendar(1_700_000_000);
    assert!(h.driver.session.search.history_calendar.is_some());
    h.driver.select_chat(BOB).unwrap();
    assert!(
        h.driver.session.search.history_calendar.is_none(),
        "leaving the chat closes its Jump to date box"
    );

    // A box that somehow outlives its chat jumps nowhere.
    h.driver.session.open_history_calendar(1_700_000_000);
    h.driver.session.search.history_calendar =
        h.driver
            .session
            .search
            .history_calendar
            .take()
            .map(|mut calendar| {
                calendar.chat_id = ALICE;
                calendar
            });
    let sent = h.sent("getChatMessageByDate", ALICE).len();
    assert_eq!(h.driver.jump_to_date(19_000).unwrap(), None);
    assert_eq!(h.sent("getChatMessageByDate", ALICE).len(), sent);
}

//! State tests: story messages (`messageStory`) and the stories they need.
use super::common::*;
use super::*;
use crate::telegram::envelope::ServiceAction;

fn story_content(via_mention: bool) -> MessageContent {
    MessageContent::Action(Box::new(ServiceAction::Story {
        via_mention,
        poster_chat_id: 77,
        story_id: 5,
    }))
}

#[test]
fn story_messages_name_their_story() {
    assert_eq!(message_story_key(&story_content(true)), Some((77, 5, true)));
    assert_eq!(
        message_story_key(&story_content(false)),
        Some((77, 5, false))
    );
    assert_eq!(message_story_key(&MessageContent::ScreenshotTaken), None);
    let unnamed = MessageContent::Action(Box::new(ServiceAction::Story {
        via_mention: false,
        poster_chat_id: 0,
        story_id: 0,
    }));
    assert_eq!(message_story_key(&unnamed), None);
}

#[test]
fn card_state_is_loading_until_the_story_lands_or_fails() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert_eq!(session.story_card_state(77, 5), StoryCardState::Loading);

    // `getStory` fails: the story is gone.
    let extra = session.request_for_story(RequestPurpose::GetStory, ChatId(77), 5);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"Story not found"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.story_card_state(77, 5), StoryCardState::Expired);

    // Another story arrives and is ready.
    let extra = session.request_for_story(RequestPurpose::GetStory, ChatId(77), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"story","@extra":"{}","id":6,"poster_chat_id":77,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
            extra.0
        ),
    );
    assert_eq!(session.story_card_state(77, 6), StoryCardState::Ready);
}

#[test]
fn a_story_is_unread_only_while_active_and_newer_than_the_read_mark() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(!session.story_is_unread(77, 5));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatActiveStories","active_stories":{"@type":"chatActiveStories","chat_id":77,"list":{"@type":"storyListMain"},"order":1,"max_read_story_id":4,"stories":[{"@type":"storyInfo","story_id":5,"date":1,"is_for_close_friends":false,"is_live":false},{"@type":"storyInfo","story_id":4,"date":1,"is_for_close_friends":false,"is_live":false}]}}"#,
    );
    assert!(session.story_is_unread(77, 5));
    assert!(!session.story_is_unread(77, 4));
    assert!(!session.story_is_unread(77, 9));
}

fn pay_for(session: &mut Session, sink: &Arc<MemorySink>, content: &str, star_count: i64) {
    let seq = AtomicU64::new(session.last_seq);
    apply_json(
        session,
        &seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":402,"chat_id":21,"is_outgoing":false,"date":1,"content":{content}}}}}"#
        ),
    );
    session.payments.request = Some(PaymentRequest {
        invoice: crate::payment_invoice::InputInvoice::Message {
            chat_id: ChatId(21),
            message_id: MessageId(402),
        },
    });
    session.payments.form = Some(crate::telegram::envelope::PaymentFormData {
        id: 1,
        form_type: crate::telegram::envelope::PaymentFormTypeData::Stars { star_count },
        seller_bot_user_id: 0,
        product_title: String::new(),
        product_description: String::new(),
    });
    let extra = session.request(RequestPurpose::SendPaymentForm, Some(ChatId(21)));
    apply_json(
        session,
        &seq,
        sink,
        &format!(
            r#"{{"@type":"paymentResult","@extra":"{}","success":true,"verification_url":""}}"#,
            extra.0
        ),
    );
}

#[test]
fn paying_for_paid_media_leaves_a_media_unlocked_note() {
    let (mut session, sink) = session();
    pay_for(
        &mut session,
        &sink,
        r#"{"@type":"messagePaidMedia","star_count":25,"media":[{"@type":"paidMediaPreview","width":4,"height":3,"duration":0,"minithumbnail":null}],"caption":{"@type":"formattedText","text":"","entities":[]}}"#,
        25,
    );
    let note = session.payments.unlocked_notice.take().expect("a note");
    assert!(note.starts_with("Media Unlocked."), "{note}");
    assert!(note.contains("25 Stars"), "{note}");
}

#[test]
fn paying_for_an_invoice_message_leaves_no_note() {
    let (mut session, sink) = session();
    pay_for(
        &mut session,
        &sink,
        r#"{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}"#,
        25,
    );
    assert!(session.payments.unlocked_notice.is_none());
}

#[test]
fn a_bought_album_pages_through_every_item_in_the_viewer() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let photo = |id: i32| {
        format!(
            r#"{{"@type":"paidMediaPhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"x","width":800,"height":600,"photo":{{"@type":"file","id":{id},"size":9,"expected_size":9,"local":{{"path":"","can_be_downloaded":true}}}}}}]}}}}"#
        )
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":402,"chat_id":21,"is_outgoing":false,"date":1,"content":{{"@type":"messagePaidMedia","star_count":5,"media":[{},{}],"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#,
            photo(31),
            photo(32)
        ),
    );
    let messages: Vec<HistoryMessage> = session.histories[&21]
        .ordered()
        .into_iter()
        .cloned()
        .collect();
    let items = crate::media_viewer::collect_media_items(&messages);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.message_id == MessageId(402)));
    assert_ne!(items[0].download_file_id, items[1].download_file_id);
}

#[test]
fn the_note_counts_one_star_in_the_singular() {
    assert_eq!(
        media_unlocked_note(1, "Dana"),
        "Media Unlocked. 1 Star transferred to Dana."
    );
}

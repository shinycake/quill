use super::{MessageA11yContext, format_duration_text, message_accessibility_name};
use crate::ids::{ChatId, FileId, MessageId};
use crate::state::{HistoryMessage, OutboxReceipt};
use crate::telegram::envelope::ParsedFile;
use crate::telegram::envelope::{
    DocumentContent, LocalFileState, MessageContent, MessageInteractionInfo, MessageReaction,
    MessageReactions, ReactionType, TextContent,
};

fn message(content: MessageContent, outgoing: bool) -> HistoryMessage {
    HistoryMessage {
        sender: None,
        id: MessageId(10),
        chat_id: ChatId(1),
        is_outgoing: outgoing,
        date: 0,
        content,
        pending: false,
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        is_pinned: false,
        media_album_id: 0,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
        author_signature: None,
        failed: false,
        can_retry: false,
        ephemeral: None,
    }
}

fn no_files(_: FileId) -> Option<ParsedFile> {
    None
}

#[test]
fn an_incoming_text_reads_sender_text_and_status() {
    let mut msg = message(
        MessageContent::Text(TextContent::plain("Hello there")),
        false,
    );
    msg.extras.edit_date = 5;
    msg.is_pinned = true;
    msg.interaction_info = Some(MessageInteractionInfo {
        reactions: Some(MessageReactions {
            reactions: vec![
                MessageReaction {
                    reaction_type: ReactionType::emoji("👍"),
                    total_count: 2,
                    is_chosen: false,
                    recent_senders: Vec::new(),
                },
                MessageReaction {
                    reaction_type: ReactionType::emoji("❤"),
                    total_count: 1,
                    is_chosen: false,
                    recent_senders: Vec::new(),
                },
                MessageReaction {
                    reaction_type: ReactionType::Paid,
                    total_count: 9,
                    is_chosen: false,
                    recent_senders: Vec::new(),
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    });
    let context = MessageA11yContext {
        sender_name: Some("Ada".into()),
        reply: Some((Some("Grace".into()), "Lunch?".into())),
        forwarded_from: Some("News".into()),
        via_bot: Some("gif".into()),
        time: Some("14:05".into()),
        ..Default::default()
    };
    assert_eq!(
        message_accessibility_name(&msg, &context, &no_files),
        "Ada\nvia @gif\nIn reply to Grace: Lunch?\nForwarded from News\nHello there\nPinned\n\
         Received edited at 14:05\nReactions: 👍 2, ❤"
    );
}

#[test]
fn an_outgoing_message_reads_seen_me_and_delivery() {
    let mut msg = message(MessageContent::Text(TextContent::plain("Hi")), true);
    let mut context = MessageA11yContext {
        receipt: Some(OutboxReceipt::Read),
        reply: Some((None, "Earlier".into())),
        ..Default::default()
    };
    assert_eq!(
        message_accessibility_name(&msg, &context, &no_files),
        "Seen\nMe\nIn reply to Me: Earlier\nHi\nSent"
    );
    context.receipt = Some(OutboxReceipt::Sent);
    msg.pending = true;
    assert_eq!(
        message_accessibility_name(&msg, &context, &no_files),
        "Not seen\nMe\nIn reply to Me: Earlier\nHi\nSending"
    );
    msg.pending = false;
    msg.failed = true;
    assert!(message_accessibility_name(&msg, &context, &no_files).ends_with("\nFailed"));
}

#[test]
fn a_file_reads_its_kind_state_name_and_size() {
    let doc = DocumentContent {
        thumbnail: None,
        minithumbnail: None,
        size: 0,
        file_name: "report.pdf".into(),
        mime_type: "application/pdf".into(),
        caption: "Q3".into(),
        caption_entities: Vec::new(),
        file_id: FileId(7),
    };
    let msg = message(MessageContent::Document(doc), false);
    let downloaded = |id: FileId| {
        (id == FileId(7)).then(|| ParsedFile {
            id,
            size: 3 * 1024 * 1024 / 2,
            expected_size: 0,
            local: LocalFileState {
                path: "/tmp/report.pdf".into(),
                can_be_downloaded: true,
                is_downloading_active: false,
                is_downloading_completed: true,
                downloaded_size: 0,
            },
        })
    };
    assert_eq!(
        message_accessibility_name(&msg, &MessageA11yContext::default(), &downloaded),
        "File, Downloaded, report.pdf, 1.5 MB\nQ3\nReceived"
    );
    assert_eq!(
        message_accessibility_name(&msg, &MessageA11yContext::default(), &no_files),
        "File, Not downloaded, report.pdf\nQ3\nReceived"
    );
}

#[test]
fn durations_follow_format_duration_text() {
    assert_eq!(format_duration_text(7), "00:07");
    assert_eq!(format_duration_text(754), "12:34");
    assert_eq!(format_duration_text(3723), "1:02:03");
}

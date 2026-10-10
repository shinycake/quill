//! Screenshot fixtures of the chat-list rows work: the video chat badge and
//! emoji status on rows, the Archive hint box, the folder editor's chat
//! sections and picker, and the folder toast (injected through the
//! reducer, no live Telegram).

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::dialogs::FolderEditorDialog;
use super::folder_demo::apply_ready_folders_share;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::{Context, Window};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::folder_picker::PickerMode;
use quill::ids::{ChatId, FileId};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The custom emoji the demo Premium user shows after the name.
const STATUS_EMOJI: i64 = 4242;

/// Two more chats over the `ReadyChats` seed: Maya Chen (Premium, with an
/// emoji status whose image is already downloaded) and the "Design team"
/// group with a running video chat.
fn apply_ready_chat_badges(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":51,"first_name":"Maya","last_name":"Chen","is_premium":true,"emoji_status":{{"@type":"emojiStatus","type":{{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"{STATUS_EMOJI}"}},"expiration_date":0}},"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":51,"title":"Maya Chen","type":{"@type":"chatTypePrivate","user_id":51},"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":52,"title":"Design team","type":{"@type":"chatTypeSupergroup","supergroup_id":52,"is_channel":false},"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":51,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"45","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":52,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"44","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":51,"last_message":{"id":151,"chat_id":51,"is_outgoing":false,"date":1790632000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"The new mockups are ready","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"45","is_pinned":false}]}"#.to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":52,"last_message":{"id":141,"chat_id":52,"is_outgoing":false,"date":1790631900,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Joining the review call now","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"44","is_pinned":false}]}"#.to_string(),
        r#"{"@type":"updateChatVideoChat","chat_id":52,"video_chat":{"@type":"videoChat","group_call_id":555,"has_participants":true}}"#.to_string(),
        demo_file_json(61, &demo_thumb_png_path(), true),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.emoji.custom_emoji_stickers.push(StickerItem {
        custom_emoji_id: Some(STATUS_EMOJI),
        id: STATUS_EMOJI,
        set_id: 0,
        emoji: "\u{1F600}".to_string(),
        width: 512,
        height: 512,
        format: StickerFormat::Webp,
        file_id: FileId(61),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
    });
}

register_demos![
    // Chat-list rows: the Archive's "How does it work?" box.
    DemoSpec::chats("ready-archive-hint", "screenshot demo — chat-list rows, archive hint and folder pickers (injected, no live Telegram)").setup(|app, window, cx| app.demo_setup_chatlist_rows(RowsDemo::ArchiveHint, window, cx)),
    // Chat-list rows: video chat badge and emoji status on rows.
    DemoSpec::chats("ready-chat-badges", "screenshot demo — chat-list rows, archive hint and folder pickers (injected, no live Telegram)").setup(|app, window, cx| app.demo_setup_chatlist_rows(RowsDemo::ChatBadges, window, cx)),
    // Chat-list rows: the folder editor's chat picker with search.
    DemoSpec::chats("ready-folders-chat-picker", "screenshot demo — chat-list rows, archive hint and folder pickers (injected, no live Telegram)").setup(|app, window, cx| app.demo_setup_chatlist_rows(RowsDemo::FoldersChatPicker, window, cx)),
    // Chat-list rows: the folder editor's chat sections.
    DemoSpec::chats("ready-folders-chats", "screenshot demo — chat-list rows, archive hint and folder pickers (injected, no live Telegram)").setup(|app, window, cx| app.demo_setup_chatlist_rows(RowsDemo::FoldersChats, window, cx)),
    // Chat-list rows: toast after adding a chat to a folder.
    DemoSpec::chats("ready-folders-toast", "screenshot demo — chat-list rows, archive hint and folder pickers (injected, no live Telegram)").setup(|app, window, cx| app.demo_setup_chatlist_rows(RowsDemo::FoldersToast, window, cx)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowsDemo {
    ArchiveHint,
    ChatBadges,
    FoldersChatPicker,
    FoldersChats,
    FoldersToast,
}

impl QuillApp {
    /// Fixtures of the chat-list rows captures.
    fn demo_setup_chatlist_rows(
        &mut self,
        demo: RowsDemo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_badges(session, &self.demo_sink, &self.demo_seq);
            if !matches!(demo, RowsDemo::ArchiveHint | RowsDemo::ChatBadges) {
                apply_ready_folders_share(session, &self.demo_sink, &self.demo_seq);
            }
        }
        match demo {
            RowsDemo::ArchiveHint => {
                self.archive_hint_open = true;
                self.status_note = "screenshot demo — the Archive's How does it work? box".into();
            }
            RowsDemo::ChatBadges => {
                self.status_note = "screenshot demo — video chat badge and emoji status".into();
            }
            RowsDemo::FoldersChats | RowsDemo::FoldersChatPicker => {
                let mut dialog = FolderEditorDialog::new(window, cx, None);
                dialog.editor.name = "Design".into();
                dialog.editor.included = [52, 51].into_iter().collect();
                dialog.editor.excluded = [13].into_iter().collect();
                dialog
                    .name_input
                    .update(cx, |input, cx| input.set_value("Design", window, cx));
                if matches!(demo, RowsDemo::FoldersChatPicker) {
                    dialog.picker = Some(PickerMode::Include);
                    dialog
                        .picker_search
                        .update(cx, |input, cx| input.set_value("e", window, cx));
                }
                self.folder_editor = Some(dialog);
            }
            RowsDemo::FoldersToast => {
                // The real code path: pick "Work" for Maya Chen's chat.
                self.add_open_chat_to_folder(ChatId(51), 1, cx);
            }
        }
    }
}

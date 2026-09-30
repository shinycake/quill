use quill::ids::{ChatId, MessageId};
/// Slice G2: channel-post comment-thread viewer (message menu →
/// "View comments"). The dialog shows the
/// `Session::comment_thread` fetch (`Loading` / `Failed` / loaded
/// `MessageThreadHistory`) for the tapped post.
pub struct CommentThreadDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
}

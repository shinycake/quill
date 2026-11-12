//! Composer state: menus, dialogs, reply and edit drafts. The next send's
//! attachments and options are per chat, in `composer_sends.rs`.
#![deny(clippy::indexing_slicing)]

use quill::composer::DraftTopic;
use quill::ids::{ChatId, MessageId, RequestId};
use quill::telegram::envelope::ChatDraft;

/// A draft to clear once a send is acknowledged. `sent` is the composer
/// markup that send took: when set, the stored draft goes only while it
/// still equals it, so text kept or typed since survives a late ack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftClear {
    pub(super) chat_id: ChatId,
    pub(super) sent: Option<String>,
    stage: ClearStage,
}

/// Where the clear is. The messages of the send ride in the stage: an ack
/// names its message by the temporary id TDLib gave it (the topic it names
/// can differ from the composer's slot, as for a reply from a supergroup's
/// main composer), so that id is how it finds the clear of the send it ends.
/// A send of several messages (an album) keeps its entry until its last
/// message has acked or failed, so a later ack of the same send finds it by
/// id and never another send's.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ClearStage {
    /// The send named no request (a menu command): only a slot match finds it.
    Unasked,
    /// The send's messages wait for their answers and acks; the first ack
    /// clears the draft.
    Armed(Vec<SentMessage>),
    /// One message acked (the draft was cleared then) or failed (the draft
    /// stays): the rest of the send's acks clear nothing. Done once the
    /// list is empty.
    Spent(Vec<SentMessage>),
}

/// One message of a send: asked for under a request, then made by it.
/// A request that sends an album is answered once per message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SentMessage {
    Asked(RequestId),
    Made {
        request: RequestId,
        message: MessageId,
    },
}

impl ClearStage {
    fn messages_mut(&mut self) -> Option<&mut Vec<SentMessage>> {
        match self {
            Self::Unasked => None,
            Self::Armed(messages) | Self::Spent(messages) => Some(messages),
        }
    }

    fn messages(&self) -> &[SentMessage] {
        match self {
            Self::Unasked => &[],
            Self::Armed(messages) | Self::Spent(messages) => messages,
        }
    }

    /// `message` left the send, acked or failed; the stage is `Spent`.
    fn spend(&mut self, message: MessageId) {
        let mut messages = match std::mem::replace(self, Self::Unasked) {
            Self::Unasked => Vec::new(),
            Self::Armed(messages) | Self::Spent(messages) => messages,
        };
        messages.retain(|m| !matches!(m, SentMessage::Made { message: id, .. } if *id == message));
        *self = Self::Spent(messages);
    }
}

impl DraftClear {
    pub(super) fn whole_composer(chat_id: ChatId) -> Self {
        Self::new(chat_id, None)
    }

    pub(super) fn of_text(chat_id: ChatId, sent: &str) -> Self {
        Self::new(chat_id, Some(sent.to_string()))
    }

    fn new(chat_id: ChatId, sent: Option<String>) -> Self {
        Self {
            chat_id,
            sent,
            stage: ClearStage::Unasked,
        }
    }

    /// The send went out as `requests`.
    pub(super) fn asked(mut self, requests: impl IntoIterator<Item = RequestId>) -> Self {
        let asked: Vec<SentMessage> = requests.into_iter().map(SentMessage::Asked).collect();
        if !asked.is_empty() {
            self.stage = ClearStage::Armed(asked);
        }
        self
    }

    /// `request` was answered with the temporary message `made`: its first
    /// answer replaces the ask, a later one (the next message of an album)
    /// adds a message beside the ones made.
    pub(super) fn answered(&mut self, request: RequestId, made: MessageId) {
        let Some(messages) = self.stage.messages_mut() else {
            return;
        };
        let new = SentMessage::Made {
            request,
            message: made,
        };
        if let Some(asked) = messages
            .iter_mut()
            .find(|m| **m == SentMessage::Asked(request))
        {
            *asked = new;
        } else if messages
            .iter()
            .any(|m| matches!(m, SentMessage::Made { request: r, .. } if *r == request))
            && !messages.contains(&new)
        {
            messages.push(new);
        }
    }

    /// `message` is one the send made.
    pub(super) fn made(&self, message: MessageId) -> bool {
        self.stage
            .messages()
            .iter()
            .any(|m| matches!(m, SentMessage::Made { message: id, .. } if *id == message))
    }

    /// Whether the send named no request: only a slot match can find it.
    pub(super) fn is_unasked(&self) -> bool {
        matches!(self.stage, ClearStage::Unasked)
    }

    /// `message` acked. True for the ack that clears the draft: the first
    /// of the send. The message leaves the send either way.
    pub(super) fn ack(&mut self, message: MessageId) -> bool {
        let clears = matches!(self.stage, ClearStage::Armed(_));
        self.stage.spend(message);
        clears
    }

    /// `message` failed: the draft stays, and no later ack of the send
    /// clears it.
    pub(super) fn fail(&mut self, message: MessageId) {
        self.stage.spend(message);
    }

    /// A send that has acked or failed one message stops waiting for those
    /// that can no longer answer: a made message that left the history
    /// (deleted while it uploaded), an asked one whose request is gone.
    /// Settled when none is left.
    pub(super) fn forget_lost(
        &mut self,
        message_exists: impl Fn(ChatId, MessageId) -> bool,
        request_waits: impl Fn(RequestId) -> bool,
    ) -> bool {
        let chat = self.chat_id;
        if let ClearStage::Spent(messages) = &mut self.stage {
            messages.retain(|m| match *m {
                SentMessage::Made { message, .. } => message_exists(chat, message),
                SentMessage::Asked(request) => request_waits(request),
            });
        }
        self.is_settled()
    }

    /// Every message of the send has acked or failed.
    pub(super) fn is_settled(&self) -> bool {
        matches!(&self.stage, ClearStage::Spent(messages) if messages.is_empty())
    }

    /// Whether the ack for `chat_id` may clear its draft, given the
    /// draft text stored now.
    pub(super) fn matches(&self, chat_id: ChatId, stored: Option<&str>) -> bool {
        self.chat_id == chat_id
            && self
                .sent
                .as_deref()
                .is_none_or(|sent| stored.is_none_or(|stored| stored == sent))
    }
}

/// The slot a composer last showed and its stored draft then.
pub(super) type ShownDraft = (ChatId, DraftTopic, Option<ChatDraft>);

/// What the composer knows of the draft it shows. Only a change to the
/// shown draft replaces the field: text a link put in the composer survives
/// drafts arriving for other chats. A topic or thread switch asks for the
/// new slot's draft from code that has no window (`Restoring`); the next
/// frame runs it, and `restore_open_draft` then records what it showed. The
/// request names the slot it asked for and is dropped when another is open
/// by then. It carries what was shown, so a remote draft arriving meanwhile
/// still compares against it, and a restore that does nothing (editing, a
/// dirty draft) leaves the old value as it was.
#[derive(Clone, Debug, Default)]
pub(super) enum DraftLoad {
    #[default]
    Unshown,
    Shown(ShownDraft),
    Restoring {
        before: Option<ShownDraft>,
        /// The slot that asked: the restore runs only while it is open.
        slot: Option<(ChatId, DraftTopic)>,
    },
}

impl DraftLoad {
    /// Record what the field now shows, returning what it showed before.
    /// A pending restore stays pending.
    pub(super) fn replace_shown(&mut self, now: ShownDraft) -> Option<ShownDraft> {
        match self {
            Self::Restoring { before, .. } => before.replace(now),
            Self::Unshown => {
                *self = Self::Shown(now);
                None
            }
            Self::Shown(shown) => Some(std::mem::replace(shown, now)),
        }
    }

    /// The field was filled from this slot: the request is spent.
    pub(super) fn set_shown(&mut self, now: Option<ShownDraft>) {
        *self = now.map_or(Self::Unshown, Self::Shown);
    }

    /// Ask for the open slot's draft on the next frame.
    pub(super) fn request_restore(&mut self, slot: Option<(ChatId, DraftTopic)>) {
        let before = match std::mem::take(self) {
            Self::Shown(shown) => Some(shown),
            Self::Restoring { before, .. } => before,
            Self::Unshown => None,
        };
        *self = Self::Restoring { before, slot };
    }

    /// Run a requested restore once: the request is spent, what was shown
    /// before stays until the restore replaces it. False when another slot
    /// than the one that asked is open (its restore is for a view that is
    /// gone).
    pub(super) fn take_restore(&mut self, open: Option<(ChatId, DraftTopic)>) -> bool {
        match std::mem::take(self) {
            Self::Restoring { before, slot } => {
                self.set_shown(before);
                slot == open
            }
            other => {
                *self = other;
                false
            }
        }
    }
}

pub(crate) struct ComposerUi {
    /// Highlighted row of the composer's `@` suggestions.
    pub(super) mention_selected: usize,
    /// When this window's AI button last appeared (its sparkle blinks from
    /// there); `None` while it is not drawn. Set and cleared as the button
    /// draws, so each window follows its own composer.
    pub(super) ai_button_since: std::cell::Cell<Option<std::time::Instant>>,
    /// Composer `#hashtag` / `:emoji` autocomplete popup.
    pub(super) suggest: super::composer_suggest::SuggestUi,
    pub(super) emoji_words: super::emoji_words_ui::EmojiWordsUi,
    /// Scroll position of the inline results panel (`inline_grid.rs`).
    pub(super) inline_scroll: gpui_kit::ScrollHandle,
    /// The chat an inline results button left, to come back to
    /// (`inline_return.rs`).
    pub(super) inline_return: Option<super::inline_return::InlineReturn>,
    /// MED4b: the `getLinkPreview` prefetch waiting for its URL to settle:
    /// the URL and its 500 ms timer. Armed once per new URL; a frame with
    /// the same URL leaves it running (round 18 B #7: re-arming every frame
    /// put the request off for as long as the app redrew). Schema: "Do not
    /// call this function too often"; TGX rate-limits 400ms.
    pub(super) preview_timer: Option<(String, gpui_kit::Task<()>)>,
    /// The composer text before the last keystroke, for "Replace emoji
    /// automatically" (it only reacts to one typed character).
    pub(super) echo: super::composer_field::FieldEcho,
    /// The send button's options menu and its effect picker
    /// (`send_effects`).
    pub(super) send_menu: super::send_effects::SendMenuUi,
    /// A rich draft's discard and Premium boxes and the rich paste notice
    /// (`rich_draft`).
    pub(super) rich_draft: super::rich_draft::RichDraftUi,
    /// The session's remote draft revision this composer last checked
    /// (`QuillApp::adopt_remote_draft`).
    pub(super) remote_draft_seen: u64,
    /// The draft the field was last filled from, or asked to be filled
    /// from.
    pub(super) draft_load: DraftLoad,
    /// What the compose field's placeholder was last built from
    /// (`sync_composer_placeholder`): the facts and the language.
    pub(super) placeholder_shown: Option<(quill::composer::PlaceholderFacts, usize)>,
    /// Files being dragged over the conversation, and what they hold
    /// (`drop_zones`), or a demo's fixed drop-zone state (no real drag in
    /// a screenshot).
    pub(super) file_drag: super::drop_zones::FileDrag,
    /// The folder being zipped for "Send as archive" (`drop_zones`).
    pub(super) archive: super::drop_zones::ArchiveJob,
    /// The attachment tray's file checks, price box and video editor.
    pub(super) send_box: super::send_box_state::SendBoxUi,
    pub(super) text_limit: super::composer_ui::TextLimitMemo,
    /// This composer's spell check and underlines (`spellcheck_ui`).
    pub(super) spelling: super::spell_state::ComposerSpelling,
}

impl ComposerUi {
    pub(super) fn new() -> Self {
        Self {
            mention_selected: 0,
            ai_button_since: std::cell::Cell::new(None),
            suggest: super::composer_suggest::SuggestUi::default(),
            emoji_words: Default::default(),
            inline_scroll: gpui_kit::ScrollHandle::new(),
            inline_return: None,
            preview_timer: None,
            echo: Default::default(),
            send_menu: Default::default(),
            rich_draft: Default::default(),
            remote_draft_seen: 0,
            draft_load: DraftLoad::Unshown,
            placeholder_shown: None,
            file_drag: super::drop_zones::FileDrag::Idle,
            archive: Default::default(),
            send_box: super::send_box_state::SendBoxUi::new(),
            text_limit: Default::default(),
            spelling: Default::default(),
        }
    }
}

/// The video editor's preview is a file on disk: it goes with the editor
/// (`close_video_editor`); the rest of the composer's state is the
/// account's only app-side.
impl super::on_screen::Farewell for ComposerUi {
    fn farewell(
        app: &mut super::app::QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<super::app::QuillApp>,
    ) {
        app.close_video_editor(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::DraftClear;
    use quill::ids::{ChatId, MessageId, RequestId};

    /// An album whose second part is deleted while it uploads never acks it:
    /// once the first has acked and the second left the history, the clear
    /// is settled instead of living until a newer send replaces it.
    #[test]
    fn a_clear_stops_waiting_for_an_album_part_that_left_the_history() {
        let chat = ChatId(5);
        let mut clear = DraftClear::whole_composer(chat).asked([RequestId(1)]);
        clear.answered(RequestId(1), MessageId(-10));
        clear.answered(RequestId(1), MessageId(-11));
        assert!(clear.ack(MessageId(-10)), "the first ack clears the draft");
        let there = |_: ChatId, message: MessageId| message == MessageId(-11);
        let waits = |_: RequestId| false;
        assert!(!clear.forget_lost(there, waits), "the part is still there");
        let gone = |_: ChatId, _: MessageId| false;
        assert!(clear.forget_lost(gone, waits), "the part left the history");
    }
}

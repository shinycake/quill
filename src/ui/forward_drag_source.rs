//! The source side of dragging messages onto a chat row (Telegram Desktop
//! `ListWidget::prepareDrag`): which presses may start the drag, what it
//! carries, and the picture that follows the pointer. The rules are in
//! [`quill::forward_drag`]; the chat list accepts the drop in
//! `chat_list::drop_on_row`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::forward_drag::ForwardDrag;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, trn};
use quill::state::{HistoryMessage, Session};

/// Whether the message can be forwarded as far as the session knows:
/// sent, not self-destructing, and not refused by its properties.
fn forwardable(session: &Session, chat: ChatId, id: MessageId) -> bool {
    let Some(message) = session
        .histories
        .get(&chat.0)
        .and_then(|history| history.messages.get(&id.0))
    else {
        return false;
    };
    !message.pending
        && message.ephemeral.is_none()
        && session
            .messages
            .message_menu_actions
            .filter(|(menu_chat, menu_id, _)| *menu_chat == chat && *menu_id == id)
            .is_none_or(|(_, _, actions)| actions.can_be_forwarded)
}

/// The drag a press on `pressed` in `chat` starts, or `None` when the chat
/// is protected or a carried message cannot be forwarded.
pub(super) fn drag_for(
    session: &Session,
    chat: ChatId,
    pressed: MessageId,
    selected: &[MessageId],
) -> Option<ForwardDrag> {
    ForwardDrag::begin(
        chat,
        pressed,
        selected,
        session.chat_has_protected_content(chat),
        |id| forwardable(session, chat, id),
    )
    .ok()
}

/// The drag of a lone message (its time), for a press outside selection
/// mode (`_mouseCursorState == CursorState::Date`).
pub(super) fn drag_for_message(
    session: Option<&Session>,
    message: &HistoryMessage,
) -> Option<ForwardDrag> {
    drag_for(session?, message.chat_id, message.id, &[])
}

impl QuillApp {
    /// The drag a press on a row of the standing selection starts
    /// (`uponSelected`): the whole selection, when all of it can go.
    pub(super) fn selection_forward_drag(
        &self,
        chat: ChatId,
        pressed: MessageId,
    ) -> Option<ForwardDrag> {
        let draft = self.forward_draft().filter(|d| d.from_chat_id == chat)?;
        if !draft.contains(pressed) {
            return None;
        }
        drag_for(self.session()?, chat, pressed, &draft.message_ids)
    }
}

/// What follows the pointer: the count of messages being carried.
pub(super) struct ForwardGhost {
    count: usize,
}

impl ForwardGhost {
    pub(super) fn new(drag: &ForwardDrag) -> Self {
        Self {
            count: drag.count(),
        }
    }
}

impl Render for ForwardGhost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.count == 1 {
            tr(keys::Q_CHAT_FORWARD).to_string()
        } else {
            trn(keys::Q_MISC_MESSAGES_SELECTED_2, self.count)
        };
        let theme = cx.theme();
        h_flex()
            .gap_1p5()
            .px_3()
            .py_1p5()
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .text_sm()
            .shadow_md()
            .child(Icon::new(gpui_kit::assets::IconName::Forward).size(px(14.)))
            .child(label)
    }
}

/// `element` made a source of `drag`, when there is one: pressing and
/// moving picks the messages up.
pub(super) fn draggable<E>(element: E, drag: Option<ForwardDrag>) -> E
where
    E: StatefulInteractiveElement,
{
    match drag {
        Some(drag) => element.on_drag(drag, |drag: &ForwardDrag, _, _, cx| {
            cx.new(|_| ForwardGhost::new(drag))
        }),
        None => element,
    }
}

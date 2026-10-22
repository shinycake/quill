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
use quill::selection_drag::START_DRAG_DISTANCE;
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

/// The drag of a message's time, for a press outside selection mode
/// (`_mouseCursorState == CursorState::Date`): the message, or its whole
/// album (`itemOrItsGroup`).
pub(super) fn drag_for_message(
    session: Option<&Session>,
    message: &HistoryMessage,
) -> Option<ForwardDrag> {
    let session = session?;
    let group: Vec<MessageId> = if message.media_album_id == 0 {
        Vec::new()
    } else {
        session
            .histories
            .get(&message.chat_id.0)
            .map(|history| {
                history
                    .messages
                    .values()
                    .filter(|m| m.media_album_id == message.media_album_id)
                    .map(|m| m.id)
                    .collect()
            })
            .unwrap_or_default()
    };
    drag_for(session, message.chat_id, message.id, &group)
}

/// The drag of a press on a message's media (`dragItemByHandler`, or an
/// album part): that one message, never the album and never a selection.
pub(super) fn drag_for_media(
    session: Option<&Session>,
    message: &HistoryMessage,
) -> Option<ForwardDrag> {
    let session = session?;
    if !quill::forward_drag::media_press_drags(&message.content) {
        return None;
    }
    drag_for(session, message.chat_id, message.id, &[])
}

/// `media` made a source of the message's drag: a press on the picture that
/// moves picks the message up, while a press that stays a click still opens
/// it. The click lives on the media, inside; the drag on this wrapper, so a
/// lift does not take the click with it.
pub(super) fn media_draggable(
    id: impl Into<ElementId>,
    media: AnyElement,
    drag: Option<ForwardDrag>,
) -> AnyElement {
    match drag {
        None => media,
        Some(drag) => draggable(div().id(id).child(media), Some(drag)).into_any_element(),
    }
}

/// The message's picture, video, sticker or file as the bubble shows it:
/// pressing it and moving picks the message up for a chat row
/// (`dragItemByHandler`), and media that does not lead the bubble sits below
/// a gap.
pub(super) fn media_block(
    session: Option<&Session>,
    message: &HistoryMessage,
    media: AnyElement,
    media_led: bool,
) -> AnyElement {
    let drag = drag_for_media(session, message);
    let media = media_draggable(("media-drag", message.id.0 as u64), media, drag);
    if media_led {
        media
    } else {
        div().mt_2().child(media).into_any_element()
    }
}

/// Set while a lifted message drag is in flight, so the history can tell it
/// from any other drag. Cleared when the button comes up.
#[derive(Default)]
struct ForwardLift(bool);

impl Global for ForwardLift {}

/// A message drag was lifted and has not ended.
pub(super) fn forward_lift_live(cx: &App) -> bool {
    cx.try_global::<ForwardLift>().is_some_and(|lift| lift.0) && cx.has_active_drag()
}

/// The button came up: the lifted drag, if any, is over.
pub(super) fn end_forward_lift(cx: &mut App) {
    if cx.has_global::<ForwardLift>() {
        cx.set_global(ForwardLift(false));
    }
}

impl QuillApp {
    /// The drag a press on a row of the standing selection starts
    /// (`uponSelected`): the whole selection, when all of it can go.
    pub(super) fn selection_forward_drag(
        &self,
        chat: ChatId,
        pressed: MessageId,
    ) -> Option<ForwardDrag> {
        // Most rows are not selected: leave before anything is cloned.
        let draft = self
            .share
            .pending_forward
            .as_ref()
            .filter(|d| d.from_chat_id == chat && d.contains(pressed))?;
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
        Some(drag) => element
            .on_drag(drag, |drag: &ForwardDrag, _, _, cx| {
                cx.set_global(ForwardLift(true));
                cx.new(|_| ForwardGhost::new(drag))
            })
            // tdesktop starts a drag after `QApplication::startDragDistance`;
            // a press that moves less is still the click it was.
            .drag_start_distance(px(START_DRAG_DISTANCE)),
        None => element,
    }
}

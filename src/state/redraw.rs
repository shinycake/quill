//! How urgently a TDLib envelope needs the window redrawn.
//!
//! With the window in front, a signed-in account receives a steady stream
//! of updates that change nothing the user looks at, or only a row of the
//! chat list: contacts going online and offline, people typing in other
//! chats, unread totals (the tray reads those), messages in chats that
//! are not open. A full Quill redraw costs a few milliseconds (the whole
//! window is laid out and painted again), so redrawing for each of them
//! kept an idle window at 6–8% CPU.
//!
//! [`redraw_need`] grades an envelope before it is applied; the poll loop
//! keeps the most urgent grade of a batch and redraws accordingly (see
//! `ui::notifications`). Anything not graded here is [`RedrawNeed::Now`],
//! so an update type added later redraws at once until someone decides
//! otherwise.

use super::Session;
use crate::ids::{ChatId, UserId};
use crate::telegram::envelope::{ChatKind, Envelope, EnvelopePayload, ParsedFile};

/// What an envelope's effect on screen asks for, least urgent first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum RedrawNeed {
    /// Nothing drawn depends on it (unread totals, unknown updates).
    #[default]
    Nothing,
    /// Only chat-list rows (and the app chrome around the panes) can
    /// change: typing, online dots, last messages and badges of chats
    /// that are not open. Redrawn together, a few times a second, without
    /// the conversation.
    ChatList,
    /// Anything may change but nothing is urgent (automatic downloads,
    /// user and group records): a coalesced full redraw.
    Later,
    /// The open chat or something the user is waiting for: redraw now.
    Now,
}

/// Grade `envelope` against `session` as it is before the envelope is
/// applied.
pub fn redraw_need(session: &Session, envelope: &Envelope) -> RedrawNeed {
    use EnvelopePayload as P;
    // An answer to a request of ours: the UI may be waiting on it (a
    // spinner, a panel, an error). Only a few fire-and-forget calls
    // return nothing anyone watches.
    if let Some(id) = envelope.extra {
        return match session.requests.purpose(id) {
            Some(super::RequestPurpose::ViewMessages | super::RequestPurpose::SetOnline)
                if matches!(envelope.payload, P::Ok) =>
            {
                RedrawNeed::Nothing
            }
            _ => RedrawNeed::Now,
        };
    }
    match &envelope.payload {
        // The reducer ignores these (`unknown-variant` diagnostic only).
        P::Unknown(_) => RedrawNeed::Nothing,
        // Unread totals feed the tray and the dock badge, which sync on
        // their own timer; no pane draws them.
        P::UpdateUnreadMessageCount { .. } | P::UpdateUnreadChatCount { .. } => RedrawNeed::Nothing,
        // Our own id (which chat is Saved Messages) and Premium change
        // what is drawn; other options are limits and flags read when the
        // user acts.
        P::UpdateOption { name, .. } if matches!(name.as_str(), "my_id" | "is_premium") => {
            RedrawNeed::Now
        }
        P::UpdateOption { .. } => RedrawNeed::Later,
        P::UpdateUserStatus { user_id, status } => {
            let Some(user) = session.user(user_id.0) else {
                // The reducer only updates users it knows.
                return RedrawNeed::Nothing;
            };
            if user.status == *status {
                RedrawNeed::Nothing
            } else if shows_user(session, *user_id) {
                RedrawNeed::Now
            } else if user.status.is_online() != status.is_online() {
                // The chat list's online dot.
                RedrawNeed::ChatList
            } else {
                // "last seen …" text in contact and member lists.
                RedrawNeed::Later
            }
        }
        P::UpdateChatAction { chat_id, .. } => list_unless_shown(session, *chat_id),
        P::UpdateNewMessage(message) => list_unless_shown(session, message.chat_id),
        P::UpdateChatLastMessage { chat_id, .. }
        | P::UpdateChatReadInbox { chat_id, .. }
        | P::UpdateChatReadOutbox { chat_id, .. }
        | P::UpdateChatUnreadMentionCount { chat_id, .. }
        | P::UpdateChatUnreadReactionCount { chat_id, .. }
        | P::UpdateChatIsMarkedAsUnread { chat_id, .. }
        | P::UpdateChatDraftMessage { chat_id, .. }
        | P::UpdateChatNotificationSettings { chat_id, .. }
        | P::UpdateChatAddedToList { chat_id, .. }
        | P::UpdateChatRemovedFromList { chat_id, .. } => list_unless_shown(session, *chat_id),
        P::UpdateChatPosition(position) => list_unless_shown(session, position.chat_id),
        P::UpdateChatOnlineMemberCount { chat_id, .. } => {
            if shows_chat(session, ChatId(*chat_id)) {
                RedrawNeed::Now
            } else {
                RedrawNeed::Later
            }
        }
        P::UpdateFile(file) => file_need(session, file),
        P::UpdateFileDownload { file_id, .. } => {
            if session.user_downloads.contains(file_id) {
                RedrawNeed::Now
            } else {
                RedrawNeed::Later
            }
        }
        P::UpdateUser { user_id, .. } => {
            // Our own record answers the user's profile edits (name,
            // photo, username), which arrive as `updateUser`.
            if shows_user(session, *user_id) || session.my_user_id == Some(user_id.0) {
                RedrawNeed::Now
            } else {
                RedrawNeed::Later
            }
        }
        // The open group's header (member count) and composer (our
        // rights) read its record.
        P::UpdateSupergroup { supergroup_id, .. } => group_need(
            session,
            |kind| matches!(kind, ChatKind::Supergroup { supergroup_id: id, .. } if id == supergroup_id),
        ),
        P::UpdateBasicGroup { basic_group_id, .. } => group_need(
            session,
            |kind| matches!(kind, ChatKind::BasicGroup { basic_group_id: id } if id == basic_group_id),
        ),
        _ => RedrawNeed::Now,
    }
}

/// Whether the conversation pane draws `chat_id`: the open chat, or the
/// comment thread shown in its place.
fn shows_chat(session: &Session, chat_id: ChatId) -> bool {
    session.open_chat == Some(chat_id)
        || session
            .thread
            .as_ref()
            .is_some_and(|thread| thread.chat_id == chat_id || thread.origin_chat_id == chat_id)
}

/// Whether the conversation header shows `user_id`'s status: the peer of
/// the open private or secret chat.
fn shows_user(session: &Session, user_id: UserId) -> bool {
    session
        .open_chat
        .and_then(|chat_id| session.chats.get(&chat_id.0))
        .is_some_and(|chat| match chat.kind {
            ChatKind::Private { user_id: peer } | ChatKind::Secret { user_id: peer, .. } => {
                peer == user_id
            }
            _ => false,
        })
}

/// `Now` when the open chat is the group `is_group` matches, else `Later`.
fn group_need(session: &Session, is_group: impl Fn(&ChatKind) -> bool) -> RedrawNeed {
    let open = session
        .open_chat
        .and_then(|chat_id| session.chats.get(&chat_id.0))
        .is_some_and(|chat| is_group(&chat.kind));
    if open {
        RedrawNeed::Now
    } else {
        RedrawNeed::Later
    }
}

fn list_unless_shown(session: &Session, chat_id: ChatId) -> RedrawNeed {
    if shows_chat(session, chat_id) {
        RedrawNeed::Now
    } else {
        RedrawNeed::ChatList
    }
}

/// Downloads the user started, and the open chat's automatic media
/// downloads, show live progress. Other automatic downloads (avatars,
/// thumbnails, stickers) draw nothing while in flight, so their progress
/// redraws in batches; the update that completes one redraws at once, as
/// the picture appears wherever it is shown. A file Quill is not
/// downloading at all (an upload, a generated file) redraws at once.
fn file_need(session: &Session, file: &ParsedFile) -> RedrawNeed {
    let id = file.id.0;
    let completes = file.local.is_downloading_completed
        && !session
            .files
            .get(&id)
            .is_some_and(|known| known.local.is_downloading_completed);
    if completes
        || session.user_downloads.contains(&id)
        || session.open_chat_media_downloads.contains(&id)
    {
        RedrawNeed::Now
    } else if session.downloading.contains(&id) || file.local.is_downloading_completed {
        RedrawNeed::Later
    } else {
        RedrawNeed::Now
    }
}

//! The Add-bot and Share-game boxes send one request and close on its
//! answer: a box that closed when the request was only queued showed a
//! success the server could still refuse. While the request is on its way
//! the box stays (its button disabled); an answer closes it and opens the
//! chat, an error leaves it open with the reason.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::ProfileDialog;
use super::shell::DialogKind;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr};

/// Which box sent, for the words of its failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoxKind {
    AddBot,
    ShareGame,
}

/// What the box waits for. Every stage has a way out: the answer, an
/// error, the box closing, the account going away (`DialogUi` is replaced
/// whole).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) enum BoxSend {
    #[default]
    Idle,
    Sending {
        kind: BoxKind,
        request: RequestId,
        chat: ChatId,
        /// A bot added as an administrator gets the link's start payload
        /// once the promotion is answered (tdesktop sends it then).
        then_start: Option<(i64, String)>,
    },
}

impl BoxSend {
    pub(super) fn is_sending(&self) -> bool {
        matches!(self, Self::Sending { .. })
    }
}

impl QuillApp {
    /// The box's request went out: it waits for the answer.
    pub(super) fn begin_box_send(
        &mut self,
        kind: BoxKind,
        request: RequestId,
        chat: ChatId,
        then_start: Option<(i64, String)>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.bots.sends.watch(request);
        }
        self.dialogs.box_send = BoxSend::Sending {
            kind,
            request,
            chat,
            then_start,
        };
    }

    /// Each window's frame: the answer of the box's request closes the box
    /// and opens the chat; an error keeps the box with the reason.
    pub(super) fn pump_box_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let BoxSend::Sending {
            kind,
            request,
            chat,
            then_start,
        } = self.dialogs.box_send.clone()
        else {
            return;
        };
        let box_open = matches!(
            self.dialogs.profile_dialog,
            Some(ProfileDialog::AddBot { .. } | ProfileDialog::ShareGame { .. })
        );
        let outcome = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.bots.sends.take_outcome(request));
        if !box_open {
            // Closed meanwhile: nothing waits for it any more.
            if outcome.is_some() {
                self.dialogs.box_send = BoxSend::Idle;
            }
            return;
        }
        let Some(done) = outcome else {
            return;
        };
        self.dialogs.box_send = BoxSend::Idle;
        if !done {
            self.set_note(StatusNote::error(tr(match kind {
                BoxKind::AddBot => keys::Q_MEDIA_YOU_CANT_ADD_THE_BOT,
                BoxKind::ShareGame => keys::Q_MEDIA_COULDNT_SEND_THE_GAME_HERE,
            })));
            cx.notify();
            return;
        }
        if let (Some((bot_id, parameter)), Some(live)) = (then_start, self.live.as_mut()) {
            let _ = live.driver.send_bot_start_message(chat, bot_id, &parameter);
        }
        if kind == BoxKind::ShareGame {
            self.set_note(StatusNote::silent(tr(keys::Q_MEDIA_GAME_SENT)));
        }
        self.close_profile_dialog(cx);
        self.select_listed_chat(chat, window, cx);
        self.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
    }
}

//! Where the box opens from: the QR button on profile rows, the account
//! settings row, the `settingsSectionQrCode` link, and the invite, chat
//! and proxy link QR spots.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::*;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};
use quill::peer_qr::link::eligible;
use quill::peer_qr::state::{CustomQr, QrPeer, QrSubject};
use quill::telegram::envelope::ChatKind;

/// The small QR button at the end of a profile row (tdesktop
/// `AddQrButton`). The row under it copies on click, so the click stops
/// here.
pub(in crate::ui) fn qr_row_button(
    id: impl Into<ElementId>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    let label = tr(keys::GROUP_INVITE_CONTEXT_QR);
    Button::new(id)
        .icon(IconName::QrCode)
        .ghost()
        .small()
        .tooltip(label)
        .accessibility_label(label)
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx);
        })
}

impl QuillApp {
    /// tdesktop `DefaultShowFillPeerQrBoxCallback`: yourself, or anyone
    /// with a public username.
    pub(in crate::ui) fn qr_eligible(&self, peer: QrPeer) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        match peer {
            QrPeer::User(user_id) => session
                .user(user_id)
                .is_some_and(|user| eligible(self.qr_is_self(peer), &user.active_usernames)),
            QrPeer::Chat(chat_id) => {
                session
                    .chats
                    .get(&chat_id)
                    .is_some_and(|chat| match chat.kind {
                        ChatKind::Supergroup { supergroup_id, .. } => session
                            .supergroup_username(supergroup_id)
                            .is_some_and(|username| !username.is_empty()),
                        _ => false,
                    })
            }
        }
    }

    /// Your own code (the account settings row, the deep link).
    pub(in crate::ui) fn open_own_qr_box(&mut self, cx: &mut Context<Self>) {
        let Some(me) = self.session().and_then(|session| session.my_user_id) else {
            return;
        };
        self.open_qr_box(QrSubject::Peer(QrPeer::User(me)), cx);
    }

    pub(in crate::ui) fn open_peer_qr_box(&mut self, peer: QrPeer, cx: &mut Context<Self>) {
        if self.qr_eligible(peer) {
            self.open_qr_box(QrSubject::Peer(peer), cx);
        }
    }

    /// An invite, chat or proxy link with its title and about line.
    pub(in crate::ui) fn open_custom_qr_box(
        &mut self,
        link: String,
        title: Key,
        about: Option<Key>,
        cx: &mut Context<Self>,
    ) {
        self.open_qr_box(
            QrSubject::Custom(CustomQr {
                link,
                title: Some(title),
                about,
            }),
            cx,
        );
    }
}

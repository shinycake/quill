//! What the open box shows, read from the box state and the session each
//! frame: the link and the name under it, the colors and who may change
//! them.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use quill::lang::Key;
use quill::local_time::now_unix;
use quill::peer_qr::appearance::{Appearance, PeerQrColors, Swatch, swatches};
use quill::peer_qr::link::{OwnLinkKind, OwnLinkState, username_label};
use quill::peer_qr::state::{QrEnv, QrPeer, QrSubject, ShownLink};
use quill::privacy::NewChatPrivacyState;
use quill::profile_forms::profile_link;
use quill::telegram::envelope::ChatKind;

/// One frame of the box.
pub(super) struct QrView {
    /// The link the code encodes; `None` while it loads or when there is
    /// none.
    pub(super) shown: Option<ShownLink>,
    /// The name, number or countdown under the code.
    pub(super) label: Option<String>,
    pub(super) title: Option<Key>,
    pub(super) about: Option<Key>,
    /// Your own box (the free-message pill).
    pub(super) own: bool,
    /// The colors and emoji buttons show.
    pub(super) can_style: bool,
    /// Share needs a signed-in session (tdesktop hides it without one).
    pub(super) can_share: bool,
    pub(super) swatches: Vec<Swatch>,
    pub(super) appearance: Appearance,
    pub(super) colors: PeerQrColors,
    pub(super) env: QrEnv,
}

impl QrView {
    /// A peer's box leaves room for the pill and the name; a custom one is
    /// a square card with a title and an about line.
    pub(super) fn peer_box(&self) -> bool {
        self.title.is_none() && self.about.is_none()
    }
}

impl QuillApp {
    pub(super) fn qr_is_self(&self, peer: QrPeer) -> bool {
        match peer {
            QrPeer::User(user_id) => self
                .session()
                .is_some_and(|session| session.my_user_id == Some(user_id)),
            QrPeer::Chat(_) => false,
        }
    }

    /// Where your own link stands; `None` for any other box.
    pub(super) fn qr_own_link(&self, subject: &QrSubject) -> Option<OwnLinkState> {
        let QrSubject::Peer(peer @ QrPeer::User(user_id)) = subject else {
            return None;
        };
        if !self.qr_is_self(*peer) {
            return None;
        }
        let session = self.session()?;
        let user = session.user(*user_id)?;
        let phone_label = if user.phone_number.is_empty() {
            String::new()
        } else {
            crate::ui::group_panels::format_phone(&user.phone_number)
        };
        session.own_qr_link(&phone_label, now_unix())
    }

    /// The Stars your account charges per message (0 when free).
    fn qr_message_price(&self) -> i64 {
        match self
            .session()
            .and_then(|session| session.settings.privacy_data.new_chat.as_ref())
        {
            Some(NewChatPrivacyState::Ready(privacy)) => {
                privacy.incoming_paid_message_star_count.max(0)
            }
            _ => 0,
        }
    }

    /// The reducer's view of the clock and the account.
    pub(super) fn qr_env(&self) -> QrEnv {
        let own = self
            .peer_qr
            .open
            .as_ref()
            .and_then(|open| self.qr_own_link(&open.state.subject));
        let Some(own) = own else {
            return QrEnv {
                now: now_unix(),
                price: 0,
                temporary_only: false,
            };
        };
        QrEnv {
            now: now_unix(),
            price: self.qr_message_price(),
            temporary_only: matches!(own, OwnLinkState::Ready(link) if link.kind == OwnLinkKind::Contact),
        }
    }

    /// Another user's or a public chat's link and its `@NAME`.
    fn qr_peer_link(&self, peer: QrPeer) -> Option<(String, String)> {
        let session = self.session()?;
        let username = match peer {
            QrPeer::User(user_id) => session.user(user_id)?.username.clone(),
            QrPeer::Chat(chat_id) => match session.chats.get(&chat_id)?.kind {
                ChatKind::Supergroup { supergroup_id, .. } => {
                    session.supergroup_username(supergroup_id)?.to_string()
                }
                _ => return None,
            },
        };
        (!username.is_empty()).then(|| (profile_link(&username), username_label(&username)))
    }

    /// tdesktop `ShareThemes::CanSave`: your own box, or a public channel
    /// or group whose info you may change.
    fn qr_can_style(&self, peer: QrPeer) -> bool {
        match peer {
            QrPeer::User(_) => self.qr_is_self(peer),
            QrPeer::Chat(chat_id) => {
                self.session().is_some_and(|session| {
                    session
                        .chats
                        .get(&chat_id)
                        .is_some_and(|chat| chat.can_change_info())
                }) && self.qr_peer_link(peer).is_some()
            }
        }
    }

    /// The open box's frame, or `None` when no box is open.
    pub(super) fn qr_view(&self) -> Option<QrView> {
        let open = self.peer_qr.open.as_ref()?;
        let env = self.qr_env();
        let swatches = swatches(
            self.session()
                .map(|session| session.users_state.peer_qr.backdrops.backdrops())
                .unwrap_or_default(),
        );
        let appearance = open.state.appearance(&swatches);
        let colors = PeerQrColors::compute(appearance.center, appearance.edge);
        let can_share = self.session().is_some();
        let plain = |link: String, expires_at: i64| ShownLink {
            link,
            expires_at,
            free_code: false,
        };
        let (shown, label, title, about, own, can_style) = match &open.state.subject {
            QrSubject::Custom(custom) => (
                Some(plain(custom.link.clone(), 0)),
                None,
                custom.title,
                custom.about,
                false,
                false,
            ),
            QrSubject::Peer(peer) => match self.qr_own_link(&open.state.subject) {
                Some(own) => {
                    let (link, label) = match own {
                        OwnLinkState::Ready(link) => (
                            Some(plain(link.link, link.expires_at)),
                            Some(link.label).filter(|label| !label.is_empty()),
                        ),
                        _ => (None, None),
                    };
                    (
                        open.state.shown_link(link, env),
                        label,
                        None,
                        None,
                        true,
                        true,
                    )
                }
                None => {
                    let (link, label) = self.qr_peer_link(*peer).unzip();
                    (
                        link.map(|link| plain(link, 0)),
                        label,
                        None,
                        None,
                        false,
                        self.qr_can_style(*peer),
                    )
                }
            },
        };
        Some(QrView {
            shown,
            label,
            title,
            about,
            own,
            can_style,
            can_share,
            swatches,
            appearance,
            colors,
            env,
        })
    }
}

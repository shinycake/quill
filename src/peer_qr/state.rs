//! The open box and its reducer: what it shows, which color and emoji
//! are picked, which panel is open and which free code is shown. The UI
//! renders from this and the session alone, and runs the effects
//! [`PeerQrBox::apply`] returns.

use super::appearance::{Appearance, Swatch, SwatchKey};
use super::free::{FreeMenuItem, FreeMode, FreeRequest, FreeResult, FreeState, FreeTicket};
use super::link::SHARE_CUTOFF_SECS;
use crate::lang::Key;

/// tdesktop `kMaxEmojiPicks`.
pub const MAX_EMOJI: usize = 6;

/// A user, group or channel whose own link the box encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QrPeer {
    /// The current user or another user, by user id.
    User(i64),
    /// A public group or channel, by chat id.
    Chat(i64),
}

/// A box for a link that is not a peer's own (an invite link, a business
/// chat link, a proxy): a square code with a title and an about line, and
/// no colors or emoji (tdesktop `FillPeerQrBox` with a `customLink`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomQr {
    pub link: String,
    pub title: Option<Key>,
    pub about: Option<Key>,
}

/// What the box encodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrSubject {
    Peer(QrPeer),
    Custom(CustomQr),
}

/// The bottom panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QrPanel {
    Colors,
    Emoji,
}

/// Something the user did in the box, or a fact it reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrAction {
    OpenPanel(QrPanel),
    ClosePanel,
    PickColor(SwatchKey),
    /// Add the emoji, or remove it when already picked; a seventh drops
    /// the oldest (tdesktop `ChooseEmoji`).
    ToggleEmoji(String),
    /// The free-code pill under the card.
    PillClicked,
    MenuDismissed,
    MenuChose(FreeMenuItem),
    /// A free-code `getUserLink` came back.
    FreeAnswer {
        ticket: FreeTicket,
        result: FreeResult,
    },
    /// The clock moved: a temporary code may have run out.
    Tick,
    /// The account's price or its own link changed (tdesktop's price and
    /// `temporaryOnly` watchers).
    Sync,
}

/// What the reducer needs from outside for the free-code actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QrEnv {
    /// Unix time.
    pub now: i64,
    /// The account's price per message in Stars; 0 when messages are free.
    pub price: i64,
    /// The account's own link is itself temporary, so codes are too.
    pub temporary_only: bool,
}

/// A toast the box shows over itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrToast {
    /// `lng_no_paid_token_toast_expires`, with the seconds left.
    CodeExpires(i64),
    CodePermanent,
    CodeReset,
    FeeRestored,
    CodeExpired,
    /// `lng_no_paid_token_toast_expiring` for a free code, else
    /// `lng_peer_qr_link_expiring`.
    Expiring {
        free_code: bool,
    },
    Unavailable,
    /// The server's reason a request failed.
    Failed(String),
    /// The card image is on the clipboard.
    Copied,
    /// Only the link is on the clipboard (`lng_username_copied`).
    LinkCopied,
}

/// Work the UI does after an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrEffect {
    RequestFreeCode(FreeTicket),
    Toast(QrToast),
}

/// The link the code currently shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownLink {
    pub link: String,
    /// Unix time it stops working; 0 when permanent.
    pub expires_at: i64,
    pub free_code: bool,
}

/// The open box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerQrBox {
    /// Tells this box's free-code answers from an earlier box's.
    pub id: u64,
    pub subject: QrSubject,
    pub color: SwatchKey,
    pub emoji: Vec<String>,
    pub panel: Option<QrPanel>,
    pub free: FreeState,
}

impl PeerQrBox {
    pub fn new(subject: QrSubject, id: u64) -> Self {
        Self {
            id,
            subject,
            color: SwatchKey::Default,
            emoji: Vec::new(),
            panel: None,
            free: FreeState::default(),
        }
    }

    /// Colors and emoji are for a peer's own code only.
    pub fn styled(&self) -> bool {
        matches!(self.subject, QrSubject::Peer(_))
    }

    pub fn apply(&mut self, action: QrAction, env: QrEnv) -> Vec<QrEffect> {
        match action {
            QrAction::OpenPanel(panel) => {
                if self.styled() && self.panel.is_none() {
                    self.panel = Some(panel);
                    self.free.menu_open = false;
                }
            }
            QrAction::ClosePanel => self.panel = None,
            QrAction::PickColor(key) => {
                if self.styled() {
                    self.color = key;
                }
            }
            QrAction::ToggleEmoji(emoji) => {
                if self.styled() && !emoji.is_empty() {
                    self.toggle_emoji(emoji);
                }
            }
            QrAction::PillClicked => return self.pill_clicked(env),
            QrAction::MenuDismissed => self.free.menu_open = false,
            QrAction::MenuChose(item) => return self.menu_chose(item, env),
            QrAction::FreeAnswer { ticket, result } => {
                return self.free_answer(ticket, result, env);
            }
            QrAction::Tick => return self.tick(env),
            QrAction::Sync => self.sync(env),
        }
        Vec::new()
    }

    fn toggle_emoji(&mut self, emoji: String) {
        if let Some(at) = self.emoji.iter().position(|picked| *picked == emoji) {
            self.emoji.remove(at);
            return;
        }
        if self.emoji.len() >= MAX_EMOJI {
            self.emoji.remove(0);
        }
        self.emoji.push(emoji);
    }

    fn request(&mut self, what: FreeRequest, env: QrEnv) -> Vec<QrEffect> {
        if self.free.requesting.is_some()
            || env.price <= 0
            || (what.permanent() && !self.free.permanent_allowed(env.temporary_only))
        {
            return Vec::new();
        }
        self.free.menu_open = false;
        self.free.requesting = Some(what);
        self.free.serial += 1;
        vec![QrEffect::RequestFreeCode(FreeTicket {
            request: what,
            box_id: self.id,
            serial: self.free.serial,
        })]
    }

    /// tdesktop `dropPending`.
    fn drop_pending(&mut self) {
        self.free.requesting = None;
        self.free.serial += 1;
        self.free.menu_open = false;
    }

    fn pill_clicked(&mut self, env: QrEnv) -> Vec<QrEffect> {
        if self.free.requesting.is_some() || env.price <= 0 {
            return Vec::new();
        }
        match self.free.mode {
            FreeMode::Fee => self.request(FreeRequest::Temporary, env),
            FreeMode::Temporary { .. } | FreeMode::Permanent { .. } => {
                self.free.menu_open = !self.free.menu_open;
                Vec::new()
            }
        }
    }

    fn menu_chose(&mut self, item: FreeMenuItem, env: QrEnv) -> Vec<QrEffect> {
        self.free.menu_open = false;
        let temporary = matches!(self.free.mode, FreeMode::Temporary { .. });
        match item {
            FreeMenuItem::Reset if temporary => self.request(FreeRequest::ResetTemporary, env),
            FreeMenuItem::Reset => self.request(FreeRequest::ResetPermanent, env),
            FreeMenuItem::ToggleTimer if temporary => self.request(FreeRequest::Permanent, env),
            FreeMenuItem::ToggleTimer => self.request(FreeRequest::Temporary, env),
            FreeMenuItem::RestoreFee => {
                self.free.mode = FreeMode::Fee;
                vec![QrEffect::Toast(QrToast::FeeRestored)]
            }
        }
    }

    /// tdesktop `applyCode`, and its request's fail handler: a server
    /// error is only toasted; an empty or expired code is unavailable, and
    /// after a reset the fee is back.
    fn free_answer(&mut self, ticket: FreeTicket, result: FreeResult, env: QrEnv) -> Vec<QrEffect> {
        if ticket.box_id != self.id
            || ticket.serial != self.free.serial
            || self.free.requesting != Some(ticket.request)
        {
            return Vec::new();
        }
        self.free.requesting = None;
        let request = ticket.request;
        if request.permanent() && env.temporary_only {
            return Vec::new();
        }
        let code = match result {
            Ok(code) => code,
            Err(reason) if reason.is_empty() => return vec![QrEffect::Toast(QrToast::Unavailable)],
            Err(reason) => return vec![QrEffect::Toast(QrToast::Failed(reason))],
        };
        let temporary = !request.permanent();
        if code.link.url.is_empty() || (temporary && code.link.expires_at <= env.now) {
            if request.revoke() {
                self.drop_pending();
                self.free.mode = FreeMode::Fee;
            }
            return vec![QrEffect::Toast(QrToast::Unavailable)];
        }
        if temporary {
            let left = code.link.expires_at - env.now;
            self.free.mode = FreeMode::Temporary { code };
            return vec![QrEffect::Toast(QrToast::CodeExpires(left))];
        }
        self.free.mode = FreeMode::Permanent { code };
        vec![QrEffect::Toast(if request.revoke() {
            QrToast::CodeReset
        } else {
            QrToast::CodePermanent
        })]
    }

    fn tick(&mut self, env: QrEnv) -> Vec<QrEffect> {
        match &self.free.mode {
            FreeMode::Temporary { code } if code.link.expires_at <= env.now => {
                self.free.mode = FreeMode::Fee;
                self.free.menu_open = false;
                vec![QrEffect::Toast(QrToast::CodeExpired)]
            }
            _ => Vec::new(),
        }
    }

    /// Free messages turn the codes off; a temporary own link drops a
    /// permanent code and a permanent request.
    fn sync(&mut self, env: QrEnv) {
        if env.price <= 0 {
            if self.free.requesting.is_some() || self.free.menu_open {
                self.drop_pending();
            }
            self.free.mode = FreeMode::Fee;
            return;
        }
        if !env.temporary_only {
            return;
        }
        if self.free.requesting.is_some_and(FreeRequest::permanent) {
            self.drop_pending();
        }
        if matches!(self.free.mode, FreeMode::Permanent { .. }) {
            self.free.mode = FreeMode::Fee;
        }
    }

    /// The link the code shows: a free code when one is on, else `plain`.
    pub fn shown_link(&self, plain: Option<ShownLink>, env: QrEnv) -> Option<ShownLink> {
        if env.price <= 0 {
            return plain;
        }
        match &self.free.mode {
            FreeMode::Fee => plain,
            FreeMode::Temporary { code } | FreeMode::Permanent { code } => Some(ShownLink {
                link: code.link.url.clone(),
                expires_at: code.link.expires_at,
                free_code: true,
            }),
        }
    }

    /// The link copy and share may use, or why they do nothing: a code
    /// request in flight (silently), no link, or a temporary link about to
    /// run out (tdesktop `RefuseImage`).
    pub fn check_share(
        &self,
        shown: Option<ShownLink>,
        now: i64,
    ) -> Result<ShownLink, Option<QrToast>> {
        if self.free.requesting.is_some() {
            return Err(None);
        }
        let Some(shown) = shown.filter(|shown| !shown.link.is_empty()) else {
            return Err(Some(QrToast::Unavailable));
        };
        if shown.expires_at != 0 && shown.expires_at - now <= SHARE_CUTOFF_SECS {
            return Err(Some(QrToast::Expiring {
                free_code: shown.free_code,
            }));
        }
        Ok(shown)
    }

    /// The backdrop and emoji to draw. A color whose backdrop is no longer
    /// offered shows the default one.
    pub fn appearance(&self, swatches: &[Swatch]) -> Appearance {
        let swatch = swatches
            .iter()
            .find(|swatch| swatch.key == self.color)
            .copied()
            .unwrap_or(Swatch::DEFAULT);
        Appearance {
            center: swatch.center,
            edge: swatch.edge,
            emoji: self.emoji.clone(),
        }
    }
}

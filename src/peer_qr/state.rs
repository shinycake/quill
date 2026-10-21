//! The open box and its reducer: what it shows, which color, emoji or
//! gift is picked, which panel is open, which free code is shown and
//! when the theme is saved. The UI renders from this and the session
//! alone, and runs the effects [`PeerQrBox::apply`] returns.

use super::appearance::{Appearance, Swatch, SwatchKey};
use super::free::{FreeMenuItem, FreeMode, FreeRequest, FreeResult, FreeState, FreeTicket};
use super::link::SHARE_CUTOFF_SECS;
use super::theme::{ThemeChoice, ThemeEmoji, ThemeGift};
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
    Gifts,
}

/// Tells this box's save answers from an earlier box's or request's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SaveTicket {
    pub box_id: u64,
    pub serial: u64,
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
    /// A tile of the gifts panel (tdesktop `ChooseGift`).
    PickGift(Box<ThemeGift>),
    /// The gifts panel scrolled near its end.
    MoreGifts,
    /// The peer's saved theme, as the session now knows it.
    Served(ThemeChoice),
    /// A `setProfileShareTheme` came back: the theme it saved, or the
    /// reason it failed (empty when there is none to tell).
    SaveAnswer {
        ticket: SaveTicket,
        result: Result<ThemeChoice, String>,
    },
    /// The box is closing: a pick still unsaved is saved now.
    Closing,
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
    /// The theme may be changed and saved (tdesktop `ShareThemes::CanSave`).
    pub can_save: bool,
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
    /// `setProfileShareTheme` with `choice`.
    SaveTheme {
        ticket: SaveTicket,
        choice: ThemeChoice,
    },
    /// The owned upgraded gifts: the first page again, or the next one.
    LoadGifts {
        more: bool,
    },
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
    pub emoji: Vec<ThemeEmoji>,
    /// The gift shown instead of the color and emoji.
    pub gift: Option<Box<ThemeGift>>,
    pub panel: Option<QrPanel>,
    pub free: FreeState,
    /// The peer's saved theme as last heard; `None` until it is known.
    pub served: Option<ThemeChoice>,
    /// Something was picked in this box (tdesktop `chosen`): the box
    /// stops following the saved theme and saves the pick.
    pub chosen: bool,
    /// The save in flight, by serial.
    pub saving: Option<(u64, ThemeChoice)>,
    pub save_serial: u64,
    /// The pick whose save last failed: not sent again until the pick
    /// changes.
    pub failed: Option<ThemeChoice>,
}

impl PeerQrBox {
    pub fn new(subject: QrSubject, id: u64) -> Self {
        Self {
            id,
            subject,
            color: SwatchKey::Default,
            emoji: Vec::new(),
            gift: None,
            panel: None,
            free: FreeState::default(),
            served: None,
            chosen: false,
            saving: None,
            save_serial: 0,
            failed: None,
        }
    }

    /// Colors and emoji are for a peer's own code only.
    pub fn styled(&self) -> bool {
        matches!(self.subject, QrSubject::Peer(_))
    }

    pub fn apply(&mut self, action: QrAction, env: QrEnv) -> Vec<QrEffect> {
        let styling = self.styled() && env.can_save;
        match action {
            QrAction::OpenPanel(panel) => {
                if styling && self.panel.is_none() {
                    self.panel = Some(panel);
                    self.free.menu_open = false;
                    if panel == QrPanel::Gifts {
                        return vec![QrEffect::LoadGifts { more: false }];
                    }
                }
            }
            QrAction::ClosePanel => {
                if self.panel.take().is_some() {
                    return self.save(env);
                }
            }
            QrAction::PickColor(key) => {
                if styling {
                    self.color = key;
                    self.gift = None;
                    self.picked();
                }
            }
            QrAction::ToggleEmoji(emoji) => {
                if styling && !emoji.is_empty() {
                    // A gift has no emoji: the picks go on the default
                    // backdrop (tdesktop `BackdropHalf`).
                    if self.gift.take().is_some() {
                        self.color = SwatchKey::Default;
                    }
                    self.toggle_emoji(ThemeEmoji::Plain(emoji));
                    self.picked();
                }
            }
            QrAction::PickGift(gift) => {
                if styling {
                    self.emoji.clear();
                    self.gift = Some(gift);
                    self.picked();
                }
            }
            QrAction::MoreGifts => {
                if styling && self.panel == Some(QrPanel::Gifts) {
                    return vec![QrEffect::LoadGifts { more: true }];
                }
            }
            QrAction::Served(choice) => self.served(choice),
            QrAction::SaveAnswer { ticket, result } => return self.save_answer(ticket, result),
            QrAction::Closing => return self.save(env),
            QrAction::PillClicked => return self.pill_clicked(env),
            QrAction::MenuDismissed => self.free.menu_open = false,
            QrAction::MenuChose(item) => return self.menu_chose(item, env),
            QrAction::FreeAnswer { ticket, result } => {
                return self.free_answer(ticket, result, env);
            }
            QrAction::Tick => return self.tick(env),
            QrAction::Sync => {
                self.sync(env);
                if !env.can_save {
                    self.lose_rights();
                }
            }
        }
        Vec::new()
    }

    /// What the box shows now.
    pub fn choice(&self) -> ThemeChoice {
        match &self.gift {
            Some(gift) => ThemeChoice::Gift(gift.clone()),
            None => ThemeChoice::Emoji {
                color: self.color,
                emoji: self.emoji.clone(),
            },
        }
    }

    fn show(&mut self, choice: ThemeChoice) {
        match choice {
            ThemeChoice::Emoji { color, emoji } => {
                self.color = color;
                self.emoji = emoji;
                self.gift = None;
            }
            ThemeChoice::Gift(gift) => {
                self.emoji.clear();
                self.gift = Some(gift);
            }
        }
    }

    /// The saved theme changed: shown unless something was picked
    /// (tdesktop `QrServedThemeValue` and `ApplyServedTheme`).
    fn served(&mut self, choice: ThemeChoice) {
        if self.served.as_ref() == Some(&choice) {
            return;
        }
        self.served = Some(choice.clone());
        if !self.chosen {
            self.show(choice);
        }
    }

    /// Something was picked. A pick away from the one that failed to
    /// save lets that one be sent again later.
    fn picked(&mut self) {
        self.chosen = true;
        let choice = self.choice();
        if self
            .failed
            .as_ref()
            .is_some_and(|failed| !failed.same_key(&choice))
        {
            self.failed = None;
        }
    }

    /// The theme may no longer be changed: the panel closes unsaved and
    /// the box shows the saved theme again (tdesktop `RevertToServed`).
    fn lose_rights(&mut self) {
        self.panel = None;
        if self.chosen {
            self.chosen = false;
            let served = self.served.clone().unwrap_or_default();
            self.show(served);
        }
    }

    /// Save the pick, once, when a panel or the box closes; nothing when
    /// it is the saved theme or the one being saved (tdesktop
    /// `SaveAppearance`), the pick whose save just failed, or the default
    /// while the saved theme is not known. A gift with no received id
    /// cannot be sent.
    fn save(&mut self, env: QrEnv) -> Vec<QrEffect> {
        if !self.chosen || !env.can_save || !self.styled() {
            return Vec::new();
        }
        let choice = self.choice();
        let baseline = self
            .saving
            .as_ref()
            .map(|(_, choice)| choice)
            .or(self.served.as_ref());
        let unchanged = match baseline {
            Some(baseline) => baseline.same_key(&choice),
            // The saved theme is not known yet: a pick back to the
            // default could erase a theme this box never saw.
            None => choice.same_key(&ThemeChoice::default()),
        };
        let failed_before = self
            .failed
            .as_ref()
            .is_some_and(|failed| failed.same_key(&choice));
        if unchanged || failed_before || choice.input_json().is_none() {
            return Vec::new();
        }
        self.save_serial += 1;
        self.saving = Some((self.save_serial, choice.clone()));
        vec![QrEffect::SaveTheme {
            ticket: SaveTicket {
                box_id: self.id,
                serial: self.save_serial,
            },
            choice,
        }]
    }

    /// tdesktop's save `done` and `fail`: the saved theme becomes the
    /// baseline; a failure is toasted and the pick stays on screen.
    fn save_answer(
        &mut self,
        ticket: SaveTicket,
        result: Result<ThemeChoice, String>,
    ) -> Vec<QrEffect> {
        if ticket.box_id != self.id {
            return Vec::new();
        }
        let sent = self
            .saving
            .take_if(|(serial, _)| *serial == ticket.serial)
            .map(|(_, choice)| choice);
        match result {
            Ok(saved) => {
                self.served = Some(saved);
                self.failed = None;
                Vec::new()
            }
            Err(reason) => {
                if sent.is_some() {
                    self.failed = sent;
                }
                if reason.is_empty() {
                    Vec::new()
                } else {
                    vec![QrEffect::Toast(QrToast::Failed(reason))]
                }
            }
        }
    }

    fn toggle_emoji(&mut self, emoji: ThemeEmoji) {
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

    /// The backdrop and pattern to draw: the gift's, or the color's with
    /// the emoji. A color whose backdrop is not offered shows the default
    /// one.
    pub fn appearance(&self, swatches: &[Swatch]) -> Appearance {
        if let Some(gift) = &self.gift {
            return Appearance {
                center: gift.center,
                edge: gift.edge,
                emoji: Vec::new(),
                gift: Some(gift.id),
            };
        }
        let swatch = swatches
            .iter()
            .find(|swatch| swatch.key == self.color)
            .copied()
            .unwrap_or(Swatch::DEFAULT);
        Appearance {
            center: swatch.center,
            edge: swatch.edge,
            emoji: self.emoji.clone(),
            gift: None,
        }
    }
}

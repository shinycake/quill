//! The open box and its reducer: what it shows, which color, emoji or
//! gift is picked, which panel is open, which free code is shown and
//! when the theme is saved. The UI renders from this and the session
//! alone, and runs the effects [`PeerQrBox::apply`] returns.

use super::appearance::{Appearance, Swatch, SwatchKey};
use super::free::{FreeMenuItem, FreeMode, FreeRequest, FreeResult, FreeState, FreeUi};
use super::link::SHARE_CUTOFF_SECS;
use super::theme::{ThemeChoice, ThemeEmoji, ThemeGift};
use crate::ids::RequestId;
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
    /// Retry on the gifts panel's failure: the first page again.
    RetryGifts,
    /// The peer's saved theme, as the session now knows it.
    Served(ThemeChoice),
    /// `setProfileShareTheme` for `choice` went out as `request`: the box
    /// waits for that request's answer.
    SaveSent {
        request: RequestId,
        choice: ThemeChoice,
    },
    /// `setProfileShareTheme` for `choice` could not be sent: the pick
    /// stays on screen and is not sent again until it changes.
    SaveUnsent {
        choice: ThemeChoice,
    },
    /// The `setProfileShareTheme` sent as `request` came back: the theme it
    /// saved, or the reason it failed (empty when there is none to tell).
    /// Only the request the box waits for counts.
    SaveAnswer {
        request: RequestId,
        result: Result<ThemeChoice, String>,
    },
    /// The box is closing: a pick still unsaved is saved now.
    Closing,
    /// The free-code pill under the card.
    PillClicked,
    MenuDismissed,
    MenuChose(FreeMenuItem),
    /// A free-code `getUserLink` of this `what` went out as `request`.
    FreeSent {
        what: FreeRequest,
        request: RequestId,
    },
    /// A free-code `getUserLink` could not be sent.
    FreeUnsent,
    /// The free-code `getUserLink` sent as `request` came back (or was
    /// found gone, as an empty error). Only the request the pill waits for
    /// counts.
    FreeAnswer {
        request: RequestId,
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
    /// The code or its colors changed while a Copy or Share picture was made.
    Changed,
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
    /// `getUserLink` for a free code; the UI tells the box how it went
    /// ([`QrAction::FreeSent`] or [`QrAction::FreeUnsent`]).
    RequestFreeCode(FreeRequest),
    Toast(QrToast),
    /// `setProfileShareTheme` with `choice`; the UI tells the box how it
    /// went ([`QrAction::SaveSent`] or [`QrAction::SaveUnsent`]).
    SaveTheme {
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
    pub subject: QrSubject,
    /// What the card shows: a color with emoji, or a gift (one value, so
    /// a gift never has emoji and the colors panel never ringed a gift's).
    pub shown: ThemeChoice,
    pub panel: Option<QrPanel>,
    pub free: FreeState,
    /// The peer's saved theme as last heard; `None` until it is known.
    pub served: Option<ThemeChoice>,
    /// Whether the box follows the saved theme or shows a pick, and the
    /// save of the pick ([`ThemeStage`]).
    pub theme: ThemeStage,
}

/// The theme of the box, one stage at a time. `Following` leaves on the
/// first pick; `Picked` leaves when a save goes out (`Saving`) or the right
/// to save is lost (`Following`, showing the saved theme again); `Saving`
/// leaves on its own answer, a newer save, or the lost right. An answer
/// applies only while its request is the one out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ThemeStage {
    /// Nothing was picked: the box shows the saved theme and follows it
    /// when it changes (tdesktop `chosen` false).
    #[default]
    Following,
    /// Something was picked (tdesktop `chosen`): the box stops following
    /// the saved theme and saves the pick. `failed` is the pick whose save
    /// last failed: it is not sent again until the pick changes.
    Picked { failed: Option<ThemeChoice> },
    /// The save of `choice` is out as `request`.
    Saving {
        request: RequestId,
        choice: ThemeChoice,
    },
}

impl ThemeStage {
    /// The choice being saved, if a save is out.
    fn out(&self) -> Option<&ThemeChoice> {
        match self {
            Self::Saving { choice, .. } => Some(choice),
            Self::Following | Self::Picked { .. } => None,
        }
    }

    fn failed(&self) -> Option<&ThemeChoice> {
        match self {
            Self::Picked { failed } => failed.as_ref(),
            Self::Following | Self::Saving { .. } => None,
        }
    }
}

/// Add the emoji, or remove it when already picked; past [`MAX_EMOJI`] the
/// oldest goes.
fn toggle_emoji(picks: &mut Vec<ThemeEmoji>, emoji: ThemeEmoji) {
    if let Some(at) = picks.iter().position(|picked| *picked == emoji) {
        picks.remove(at);
        return;
    }
    if picks.len() >= MAX_EMOJI {
        picks.remove(0);
    }
    picks.push(emoji);
}

impl PeerQrBox {
    pub fn new(subject: QrSubject) -> Self {
        Self {
            subject,
            shown: ThemeChoice::default(),
            panel: None,
            free: FreeState::default(),
            served: None,
            theme: ThemeStage::default(),
        }
    }

    /// The choice whose save is out, if one is.
    pub fn saving(&self) -> Option<&ThemeChoice> {
        self.theme.out()
    }

    /// The session request the box's save waits for, if one is out.
    pub fn saving_request(&self) -> Option<RequestId> {
        match &self.theme {
            ThemeStage::Saving { request, .. } => Some(*request),
            ThemeStage::Following | ThemeStage::Picked { .. } => None,
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
                    self.free.close_menu();
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
                    self.shown = ThemeChoice::Emoji {
                        color: key,
                        emoji: self.emoji().to_vec(),
                    };
                    self.picked();
                }
            }
            QrAction::ToggleEmoji(emoji) => {
                if styling && !emoji.is_empty() {
                    // A gift has no emoji: the picks go on the default
                    // backdrop (tdesktop `BackdropHalf`).
                    let mut picks = self.emoji().to_vec();
                    toggle_emoji(&mut picks, ThemeEmoji::Plain(emoji));
                    self.shown = ThemeChoice::Emoji {
                        color: self.color(),
                        emoji: picks,
                    };
                    self.picked();
                }
            }
            QrAction::PickGift(gift) => {
                if styling {
                    self.shown = ThemeChoice::Gift(gift);
                    self.picked();
                }
            }
            QrAction::MoreGifts => {
                if styling && self.panel == Some(QrPanel::Gifts) {
                    return vec![QrEffect::LoadGifts { more: true }];
                }
            }
            QrAction::RetryGifts => {
                if styling && self.panel == Some(QrPanel::Gifts) {
                    return vec![QrEffect::LoadGifts { more: false }];
                }
            }
            QrAction::Served(choice) => self.served(choice),
            QrAction::SaveSent { request, choice } => self.save_sent(request, choice),
            QrAction::SaveUnsent { choice } => self.save_unsent(choice),
            QrAction::SaveAnswer { request, result } => return self.save_answer(request, result),
            QrAction::Closing => return self.save(env),
            QrAction::PillClicked => return self.pill_clicked(env),
            QrAction::MenuDismissed => self.free.close_menu(),
            QrAction::MenuChose(item) => return self.menu_chose(item, env),
            QrAction::FreeSent { what, request } => self.free.sent(what, request),
            QrAction::FreeUnsent => return vec![QrEffect::Toast(QrToast::Unavailable)],
            QrAction::FreeAnswer { request, result } => {
                return self.free_answer(request, result, env);
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
        self.shown.clone()
    }

    /// The backdrop color picked; the default one while a gift shows (a
    /// gift has its own backdrop).
    pub fn color(&self) -> SwatchKey {
        match &self.shown {
            ThemeChoice::Emoji { color, .. } => *color,
            ThemeChoice::Gift(_) => SwatchKey::Default,
        }
    }

    /// The emoji picked; none while a gift shows.
    pub fn emoji(&self) -> &[ThemeEmoji] {
        match &self.shown {
            ThemeChoice::Emoji { emoji, .. } => emoji,
            ThemeChoice::Gift(_) => &[],
        }
    }

    /// The gift shown instead of the color and emoji.
    pub fn gift(&self) -> Option<&ThemeGift> {
        match &self.shown {
            ThemeChoice::Gift(gift) => Some(gift),
            ThemeChoice::Emoji { .. } => None,
        }
    }

    fn show(&mut self, choice: ThemeChoice) {
        self.shown = choice;
    }

    /// The saved theme changed: shown unless something was picked
    /// (tdesktop `QrServedThemeValue` and `ApplyServedTheme`).
    fn served(&mut self, choice: ThemeChoice) {
        if self.served.as_ref() == Some(&choice) {
            return;
        }
        self.served = Some(choice.clone());
        if self.theme == ThemeStage::Following {
            self.show(choice);
        }
    }

    /// Something was picked. A pick away from the one that failed to
    /// save lets that one be sent again later.
    fn picked(&mut self) {
        let choice = self.choice();
        self.theme = match std::mem::take(&mut self.theme) {
            ThemeStage::Following => ThemeStage::Picked { failed: None },
            ThemeStage::Picked { failed } => ThemeStage::Picked {
                failed: failed.filter(|failed| failed.same_key(&choice)),
            },
            saving @ ThemeStage::Saving { .. } => saving,
        };
    }

    /// The theme may no longer be changed: the panel closes unsaved and
    /// the box shows the saved theme again (tdesktop `RevertToServed`).
    fn lose_rights(&mut self) {
        self.panel = None;
        if self.theme != ThemeStage::Following {
            self.theme = ThemeStage::Following;
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
        if self.theme == ThemeStage::Following || !env.can_save || !self.styled() {
            return Vec::new();
        }
        let choice = self.choice();
        let baseline = self.theme.out().or(self.served.as_ref());
        let unchanged = match baseline {
            Some(baseline) => baseline.same_key(&choice),
            // The saved theme is not known yet: a pick back to the
            // default could erase a theme this box never saw.
            None => choice.same_key(&ThemeChoice::default()),
        };
        let failed_before = self
            .theme
            .failed()
            .is_some_and(|failed| failed.same_key(&choice));
        if unchanged || failed_before || choice.input_json().is_none() {
            return Vec::new();
        }
        vec![QrEffect::SaveTheme { choice }]
    }

    /// The save of `choice` went out as `request`: the box waits for it. A
    /// save still out is let go (its answer no longer counts).
    fn save_sent(&mut self, request: RequestId, choice: ThemeChoice) {
        if matches!(
            self.theme,
            ThemeStage::Picked { .. } | ThemeStage::Saving { .. }
        ) {
            self.theme = ThemeStage::Saving { request, choice };
        }
    }

    /// The save of `choice` could not be sent: the pick stays, and is not
    /// sent again until it changes.
    fn save_unsent(&mut self, choice: ThemeChoice) {
        if matches!(
            self.theme,
            ThemeStage::Picked { .. } | ThemeStage::Saving { .. }
        ) {
            self.theme = ThemeStage::Picked {
                failed: Some(choice),
            };
        }
    }

    /// tdesktop's save `done` and `fail`: the saved theme becomes the
    /// baseline; a failure is toasted and the pick stays on screen.
    fn save_answer(
        &mut self,
        request: RequestId,
        result: Result<ThemeChoice, String>,
    ) -> Vec<QrEffect> {
        let sent = match std::mem::take(&mut self.theme) {
            ThemeStage::Saving {
                request: out,
                choice,
            } if out == request => Some(choice),
            other => {
                self.theme = other;
                None
            }
        };
        match result {
            // An answer for a save that is no longer the one out says nothing
            // about the newer pick; the saved theme reaches `served` from the
            // session (`QrServedThemeValue`).
            Ok(saved) => {
                if sent.is_some() {
                    self.served = Some(saved);
                    self.theme = ThemeStage::Picked { failed: None };
                }
                Vec::new()
            }
            Err(reason) => {
                // The UI tells the person about a failed save that is not
                // the one out through the app's note.
                if sent.is_none() {
                    return Vec::new();
                }
                self.theme = ThemeStage::Picked { failed: sent };
                if reason.is_empty() {
                    Vec::new()
                } else {
                    vec![QrEffect::Toast(QrToast::Failed(reason))]
                }
            }
        }
    }

    fn request(&mut self, what: FreeRequest, env: QrEnv) -> Vec<QrEffect> {
        if self.free.requesting().is_some()
            || env.price <= 0
            || (what.permanent() && !self.free.permanent_allowed(env.temporary_only))
        {
            return Vec::new();
        }
        vec![QrEffect::RequestFreeCode(what)]
    }

    fn pill_clicked(&mut self, env: QrEnv) -> Vec<QrEffect> {
        if self.free.requesting().is_some() || env.price <= 0 {
            return Vec::new();
        }
        match self.free.mode {
            FreeMode::Fee => self.request(FreeRequest::Temporary, env),
            FreeMode::Temporary { .. } | FreeMode::Permanent { .. } => {
                self.free.ui = if self.free.menu_open() {
                    FreeUi::Idle
                } else {
                    FreeUi::MenuOpen
                };
                Vec::new()
            }
        }
    }

    fn menu_chose(&mut self, item: FreeMenuItem, env: QrEnv) -> Vec<QrEffect> {
        self.free.close_menu();
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
    fn free_answer(
        &mut self,
        answered: RequestId,
        result: FreeResult,
        env: QrEnv,
    ) -> Vec<QrEffect> {
        let Some(request) = self.free.settle_request(answered) else {
            return Vec::new();
        };
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
                self.free.mode = FreeMode::Fee;
            }
            return vec![QrEffect::Toast(QrToast::Unavailable)];
        }
        if temporary {
            let left = code.link.expires_at.saturating_sub(env.now);
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
                self.free.close_menu();
                vec![QrEffect::Toast(QrToast::CodeExpired)]
            }
            _ => Vec::new(),
        }
    }

    /// Free messages turn the codes off; a temporary own link drops a
    /// permanent code and a permanent request.
    fn sync(&mut self, env: QrEnv) {
        if env.price <= 0 {
            self.free.drop_request();
            self.free.mode = FreeMode::Fee;
            return;
        }
        if !env.temporary_only {
            return;
        }
        if self.free.requesting().is_some_and(FreeRequest::permanent) {
            self.free.drop_request();
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
        if self.free.requesting().is_some() {
            return Err(None);
        }
        let Some(shown) = shown.filter(|shown| !shown.link.is_empty()) else {
            return Err(Some(QrToast::Unavailable));
        };
        if shown.expires_at != 0 && shown.expires_at.saturating_sub(now) <= SHARE_CUTOFF_SECS {
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
        if let Some(gift) = self.gift() {
            return Appearance {
                center: gift.center,
                edge: gift.edge,
                emoji: Vec::new(),
                gift: Some(gift.id),
            };
        }
        let swatch = swatches
            .iter()
            .find(|swatch| swatch.key == self.color())
            .copied()
            .unwrap_or(Swatch::DEFAULT);
        Appearance {
            center: swatch.center,
            edge: swatch.edge,
            emoji: self.emoji().to_vec(),
            gift: None,
        }
    }
}

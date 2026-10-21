//! The QR code box (`parity:profile-qr-code-box`, tdesktop
//! `FillPeerQrBox`): a peer's link, or an invite, chat or proxy link, as a
//! code on a white card over a colored backdrop, with copy, share, the
//! colors, emoji and gifts panels that style and save the peer's share
//! theme (`parity:profile-share-theme`) and, in your own box, the
//! free-message pill.
//!
//! [`quill::peer_qr`] owns the state and its reducer; this module keeps
//! the open box, runs the reducer's effects, and draws the box as a
//! function of that state and the session. See
//! docs/decisions/peer-qr-box.md and docs/decisions/profile-share-theme.md.
#![deny(clippy::indexing_slicing)]

mod art;
mod card;
mod entry;
pub(in crate::ui) use entry::qr_row_button;
mod export;
mod gift_art;
mod gift_view;
mod gifts_panel;
mod panels;
mod pattern_view;
mod qr_path;
mod surface;
mod view_model;

#[cfg(feature = "demo-capture")]
mod demo;

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::peer_qr::free::FreeMode;
use quill::peer_qr::link::OwnLinkState;
use quill::peer_qr::session::PeerQrData;
use quill::peer_qr::state::{
    PeerQrBox, QrAction, QrEffect, QrPanel, QrPeer, QrSubject, QrToast, SaveTicket,
};
use quill::peer_qr::theme::ThemeChoice;
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// tdesktop `kToastDuration`.
const TOAST: Duration = Duration::from_millis(2800);
/// How often a countdown on the box moves.
const CLOCK: Duration = Duration::from_secs(1);

/// The panel's open or close motion, for drawing only: the reducer's
/// `panel` says what is open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PanelMotion {
    /// The panel being shown, kept while it closes.
    pub(super) shown: Option<QrPanel>,
    pub(super) opening: bool,
    pub(super) started: Option<Instant>,
}

impl PanelMotion {
    const IDLE: PanelMotion = PanelMotion {
        shown: None,
        opening: false,
        started: None,
    };

    /// 0 closed, 1 open.
    pub(super) fn progress(&self, now: Instant, animate: bool) -> f64 {
        let target = if self.opening { 1. } else { 0. };
        let Some(started) = self.started.filter(|_| animate) else {
            return target;
        };
        let duration = if self.opening {
            quill::peer_qr::layout::PANEL_OPEN_MS
        } else {
            quill::peer_qr::layout::PANEL_CLOSE_MS
        };
        let t = now.duration_since(started).as_secs_f64() * 1000. / duration;
        let eased = quill::peer_qr::layout::panel_ease(t);
        if self.opening { eased } else { 1. - eased }
    }

    pub(super) fn running(&self, now: Instant, animate: bool) -> bool {
        let duration = if self.opening {
            quill::peer_qr::layout::PANEL_OPEN_MS
        } else {
            quill::peer_qr::layout::PANEL_CLOSE_MS
        };
        animate
            && self
                .started
                .is_some_and(|started| now.duration_since(started).as_secs_f64() * 1000. < duration)
    }

    /// Milliseconds since the panel started to open.
    pub(super) fn open_elapsed_ms(&self, now: Instant) -> f64 {
        match self.started {
            Some(started) if self.opening => now.duration_since(started).as_secs_f64() * 1000.,
            _ => f64::MAX,
        }
    }
}

/// The emoji the pattern showed before the last pick, while the tiles
/// swap (tdesktop `PeerQrEmojiField::setPicks`).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PatternSwap {
    pub(super) previous: Vec<String>,
    pub(super) started: Instant,
}

/// The plain emoji of `picks`, the ones the pattern draws.
pub(super) fn plain_picks(picks: &[quill::peer_qr::theme::ThemeEmoji]) -> Vec<String> {
    picks
        .iter()
        .filter_map(|pick| pick.plain().map(str::to_string))
        .collect()
}

/// The box on screen.
pub(super) struct OpenQr {
    pub(super) state: PeerQrBox,
    pub(super) opened: Instant,
    pub(super) motion: PanelMotion,
    pub(super) toast: Option<(QrToast, Instant)>,
    pub(super) swap: Option<PatternSwap>,
    /// When the shown gift's pattern began to come in (tdesktop
    /// `PeerQrGiftField::startEntry`).
    pub(super) gift_entry: Instant,
    /// The card image to attach in the chat the share chooser opens.
    pub(super) share_image: Option<PathBuf>,
    /// A card image is being made for copy or share.
    pub(super) exporting: bool,
    /// The countdown clock is ticking.
    clock_running: bool,
}

/// The QR box's part of the app.
pub(crate) struct PeerQrUi {
    pub(super) open: Option<Box<OpenQr>>,
    art: art::Art,
    gift_art: gift_art::GiftArt,
    /// The last box's id; each box gets the next one.
    last_box: u64,
    /// A shared card waiting for its chat's composer.
    pub(super) pending_attach: Option<(ChatId, PathBuf)>,
}

impl PeerQrUi {
    pub(crate) fn new(_cx: &mut App) -> Self {
        Self {
            open: None,
            art: art::Art::default(),
            gift_art: gift_art::GiftArt::default(),
            last_box: 0,
            pending_attach: None,
        }
    }
}

impl QuillApp {
    /// Only the demo serves gifts pages without a server.
    #[cfg(not(feature = "demo-capture"))]
    fn qr_demo_more_gifts(&mut self, _now: i64) {}

    /// The session's part for the box, live or demo.
    fn qr_data_mut(&mut self) -> Option<&mut PeerQrData> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session.users_state.peer_qr),
            None => self
                .demo_session
                .as_mut()
                .map(|session| &mut session.users_state.peer_qr),
        }
    }

    /// Open the box for `subject`, replacing any open one, and ask the
    /// session for what it shows: the colors, the peer's saved theme, and
    /// for your own code the phone privacy rules and the message price.
    pub(super) fn open_qr_box(&mut self, subject: QrSubject, cx: &mut Context<Self>) {
        if self.peer_qr.open.is_some() {
            self.close_qr_box(cx);
        }
        let own = matches!(
            subject,
            QrSubject::Peer(peer) if self.qr_is_self(peer)
        );
        let peer = match subject {
            QrSubject::Peer(peer) => Some(peer),
            QrSubject::Custom(_) => None,
        };
        self.drop_shared_card(cx);
        self.peer_qr.last_box += 1;
        let now = Instant::now();
        self.peer_qr.open = Some(Box::new(OpenQr {
            state: PeerQrBox::new(subject, self.peer_qr.last_box),
            opened: now,
            motion: PanelMotion::IDLE,
            toast: None,
            swap: None,
            gift_entry: now,
            share_image: None,
            exporting: false,
            clock_running: false,
        }));
        if own && let Some(data) = self.qr_data_mut() {
            data.retry_contact_link();
        }
        if let Some(live) = self.live.as_mut() {
            if let Some(peer) = peer {
                let _ = live
                    .driver
                    .fetch_share_theme_backdrops(quill::local_time::now_unix());
                let _ = live.driver.ensure_share_theme(peer);
            }
            if own {
                let _ = live
                    .driver
                    .fetch_privacy_rules(PrivacySettingKey::ShowPhoneNumber);
                let _ = live
                    .driver
                    .fetch_privacy_rules(PrivacySettingKey::AllowFindingByPhoneNumber);
                let _ = live.driver.fetch_new_chat_privacy();
            }
        }
        self.qr_serve_theme(cx);
        self.qr_refresh(cx);
        cx.notify();
    }

    /// Close the box. A pick still unsaved is saved first (tdesktop saves
    /// when the box is destroyed).
    pub(super) fn close_qr_box(&mut self, cx: &mut Context<Self>) {
        self.qr_act(QrAction::Closing, cx);
        if let Some(path) = self.peer_qr.open.take().and_then(|open| open.share_image) {
            remove_card_file(path, cx);
        }
        self.peer_qr.art.clear();
        self.peer_qr.gift_art.clear();
        cx.notify();
    }

    /// The peer the open box styles, if it is a peer's box.
    fn qr_peer(&self) -> Option<QrPeer> {
        match self.peer_qr.open.as_ref()?.state.subject {
            QrSubject::Peer(peer) => Some(peer),
            QrSubject::Custom(_) => None,
        }
    }

    /// Hand the box the peer's saved theme as the session knows it.
    fn qr_serve_theme(&mut self, cx: &mut Context<Self>) {
        let Some(peer) = self.qr_peer() else {
            return;
        };
        let served = self.session().and_then(|session| {
            let theme = session.qr_served_theme(peer)?;
            Some(theme.choice(&session.users_state.peer_qr.backdrops.list))
        });
        if let Some(choice) = served {
            self.qr_act(QrAction::Served(choice), cx);
        }
    }

    /// Run `action` through the box's reducer and do what it asks.
    pub(super) fn qr_act(&mut self, action: QrAction, cx: &mut Context<Self>) {
        let env = self.qr_env();
        let now = Instant::now();
        let Some(open) = self.peer_qr.open.as_mut() else {
            return;
        };
        let before = open.state.clone();
        let effects = open.state.apply(action, env);
        let changed = open.state != before;
        if open.state.emoji != before.emoji {
            open.swap = Some(PatternSwap {
                previous: plain_picks(&before.emoji),
                started: now,
            });
        }
        let gift = |state: &PeerQrBox| state.gift.as_ref().map(|gift| gift.id);
        if gift(&open.state) != gift(&before) {
            open.gift_entry = now;
        }
        if open.state.panel != before.panel {
            open.motion = match open.state.panel {
                Some(panel) => PanelMotion {
                    shown: Some(panel),
                    opening: true,
                    started: Some(now),
                },
                None => PanelMotion {
                    shown: open.motion.shown,
                    opening: false,
                    started: Some(now),
                },
            };
        }
        if !changed && effects.is_empty() {
            return;
        }
        self.run_qr_effects(effects, cx);
        self.qr_refresh(cx);
        cx.notify();
    }

    fn run_qr_effects(&mut self, effects: Vec<QrEffect>, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                QrEffect::RequestFreeCode(ticket) => {
                    let sent = self
                        .live
                        .as_mut()
                        .is_some_and(|live| live.driver.fetch_own_user_link(Some(ticket)).is_ok());
                    if !sent {
                        self.qr_act(
                            QrAction::FreeAnswer {
                                ticket,
                                result: Err(String::new()),
                            },
                            cx,
                        );
                    }
                }
                QrEffect::Toast(toast) => self.show_qr_toast(toast, cx),
                QrEffect::SaveTheme { ticket, choice } => self.qr_save_theme(ticket, choice, cx),
                QrEffect::LoadGifts { more } => {
                    let now = quill::local_time::now_unix();
                    if let (Some(peer), Some(live)) = (self.qr_peer(), self.live.as_mut()) {
                        let _ = live.driver.fetch_share_theme_gifts(peer, more, now);
                    } else if more {
                        self.qr_demo_more_gifts(now);
                    }
                }
            }
        }
    }

    /// `setProfileShareTheme`; its answer comes back on a poll step. The
    /// demo has no server: the save is taken at once.
    fn qr_save_theme(&mut self, ticket: SaveTicket, choice: ThemeChoice, cx: &mut Context<Self>) {
        let Some(peer) = self.qr_peer() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            // A refused request leaves its own answer to take.
            let _ = live.driver.save_share_theme(ticket, peer, choice);
            return;
        }
        let owner = self
            .session()
            .and_then(|session| session.qr_theme_owner(peer));
        if let (Some(owner), Some(data)) = (owner, self.qr_data_mut()) {
            data.start_save(ticket, owner, choice);
            data.finish_save(ticket);
        }
        self.qr_take_save_answers(cx);
    }

    /// Save answers, for this box or an earlier one.
    fn qr_take_save_answers(&mut self, cx: &mut Context<Self>) -> bool {
        let answers = self
            .qr_data_mut()
            .map(|data| data.take_save_answers())
            .unwrap_or_default();
        let any = !answers.is_empty();
        for (ticket, result) in answers {
            self.qr_act(QrAction::SaveAnswer { ticket, result }, cx);
        }
        any
    }

    /// After the box or what it shows changed (and on every poll step):
    /// ask for your contact link once the privacy rules call for one, make
    /// the pictures the box needs, and run the clock while a countdown
    /// shows.
    fn qr_refresh(&mut self, cx: &mut Context<Self>) {
        self.ensure_qr_contact_link();
        self.qr_prepare_art(cx);
        self.qr_prepare_gift_art(cx);
        self.sync_qr_clock(cx);
    }

    /// A toast over the box's bottom (tdesktop `ShowFreeToast`).
    pub(super) fn show_qr_toast(&mut self, toast: QrToast, cx: &mut Context<Self>) {
        let Some(open) = self.peer_qr.open.as_mut() else {
            return;
        };
        let shown = Instant::now();
        open.toast = Some((toast, shown));
        Self::spawn_main(cx, async move |this, cx| {
            cx.background_executor().timer(TOAST).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(open) = this.peer_qr.open.as_mut()
                    && open.toast.as_ref().is_some_and(|(_, at)| *at == shown)
                {
                    open.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// A free code or your temporary contact link counts down.
    fn qr_counts_down(&self) -> bool {
        let Some(open) = self.peer_qr.open.as_ref() else {
            return false;
        };
        matches!(open.state.free.mode, FreeMode::Temporary { .. })
            || matches!(
                self.qr_own_link(&open.state.subject),
                Some(OwnLinkState::Ready(link)) if link.expires_at != 0
            )
    }

    /// Start the once-a-second clock when a countdown shows; it stops
    /// itself when none does or the box closes.
    fn sync_qr_clock(&mut self, cx: &mut Context<Self>) {
        if !self.qr_counts_down() {
            return;
        }
        let Some(open) = self.peer_qr.open.as_mut() else {
            return;
        };
        if open.clock_running {
            return;
        }
        open.clock_running = true;
        let id = open.state.id;
        Self::spawn_main(cx, async move |this, cx| {
            loop {
                cx.background_executor().timer(CLOCK).await;
                let Ok(going) = this.update(cx, |this, cx| this.qr_clock(id, cx)) else {
                    return;
                };
                if !going {
                    return;
                }
            }
        })
        .detach();
    }

    /// One clock tick for box `id`: a code may have run out, a countdown
    /// moved. `false` once nothing counts down or the box is gone.
    fn qr_clock(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        if self.peer_qr.open.as_ref().map(|open| open.state.id) != Some(id) {
            return false;
        }
        self.ensure_qr_contact_link();
        self.qr_act(QrAction::Tick, cx);
        cx.notify();
        let going = self.qr_counts_down();
        if !going && let Some(open) = self.peer_qr.open.as_mut() {
            open.clock_running = false;
        }
        going
    }

    /// Poll step: hand the box the free-code and save answers, a failed
    /// contact link's reason and the peer's saved theme, and follow
    /// account changes.
    pub(super) fn drain_peer_qr(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.peer_qr.open.is_some();
        let own = self
            .peer_qr
            .open
            .as_ref()
            .and_then(|open| self.qr_own_link(&open.state.subject))
            .is_some();
        let Some(data) = self.qr_data_mut() else {
            return false;
        };
        let answers = data.take_free_answers();
        let failed = if own { data.contact_error.take() } else { None };
        if !open {
            data.save_answers.clear();
            return false;
        }
        let saved = self.qr_take_save_answers(cx);
        let progressed = !answers.is_empty() || failed.is_some() || saved;
        for (ticket, result) in answers {
            self.qr_act(QrAction::FreeAnswer { ticket, result }, cx);
        }
        if let Some(reason) = failed {
            let toast = if reason.is_empty() {
                QrToast::Unavailable
            } else {
                QrToast::Failed(reason)
            };
            self.show_qr_toast(toast, cx);
        }
        self.qr_serve_theme(cx);
        self.qr_act(QrAction::Sync, cx);
        self.qr_refresh(cx);
        progressed
    }

    /// Your own box: the driver asks for a temporary contact link when
    /// there is none (or it ran out) and none is on its way.
    fn ensure_qr_contact_link(&mut self) {
        let own = self
            .peer_qr
            .open
            .as_ref()
            .is_some_and(|open| self.qr_own_link(&open.state.subject).is_some());
        if own && let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .ensure_own_contact_link(quill::local_time::now_unix());
        }
    }

    /// The box hides under the share chooser; a cancelled share brings it
    /// back as it was, and its card file goes.
    pub(super) fn qr_share_cancelled(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self
            .peer_qr
            .open
            .as_mut()
            .and_then(|open| open.share_image.take())
        {
            remove_card_file(path, cx);
        }
    }

    /// A chat was picked for the shared link: the box is done, and the
    /// card waits to go into that chat's composer.
    pub(super) fn qr_share_chosen(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(open) = self.peer_qr.open.as_mut() else {
            return;
        };
        let card = open.share_image.take();
        self.drop_shared_card(cx);
        self.peer_qr.pending_attach = card.map(|path| (chat_id, path));
        self.close_qr_box(cx);
    }

    /// The shared link's chat is open: attach the card there.
    pub(super) fn attach_shared_qr_card(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        match self.peer_qr.pending_attach.take() {
            Some((chat, path)) if chat == chat_id => self.attach_dropped_files(&[path], cx),
            Some((_, path)) => remove_card_file(path, cx),
            None => {}
        }
    }

    /// A card no chat took. The share sets its chat's open together with
    /// the card, and the open attaches it at once, so once a resolved
    /// link's chat open has run, a card still waiting never will be.
    pub(super) fn drop_shared_card(&mut self, cx: &mut Context<Self>) {
        if let Some((_, path)) = self.peer_qr.pending_attach.take() {
            remove_card_file(path, cx);
        }
    }
}

/// Remove a card image nothing will attach, off the UI thread.
fn remove_card_file(path: PathBuf, cx: &mut Context<QuillApp>) {
    cx.background_spawn(async move {
        let _ = std::fs::remove_file(path);
    })
    .detach();
}

crate::ui::shell::register_dialogs! {
    /// Opens over profiles, admin panels, the business page and the proxy
    /// list; the share chooser (2900) opens over it.
    PeerQr => DialogSpec::new(
        3050,
        |app| app.peer_qr.open.is_some(),
        QuillApp::build_peer_qr_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

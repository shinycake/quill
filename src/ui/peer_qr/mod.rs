//! The QR code box (`parity:profile-qr-code-box`, tdesktop
//! `FillPeerQrBox`): a peer's link, or an invite, chat or proxy link, as a
//! code on a white card over a colored backdrop, with copy, share, the
//! colors and emoji panels and, in your own box, the free-message pill.
//!
//! [`quill::peer_qr`] owns the state and its reducer; this module keeps
//! the open box, runs the reducer's effects, and draws the box as a
//! function of that state and the session. See
//! docs/decisions/peer-qr-box.md.
#![deny(clippy::indexing_slicing)]

mod art;
mod card;
mod entry;
pub(in crate::ui) use entry::qr_row_button;
mod export;
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
use quill::peer_qr::state::{PeerQrBox, QrAction, QrEffect, QrPanel, QrSubject, QrToast};
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// tdesktop `kToastDuration`.
const TOAST: Duration = Duration::from_millis(2800);
/// How often the open box checks its clock (code expiry, countdowns).
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

/// The box on screen.
pub(super) struct OpenQr {
    pub(super) state: PeerQrBox,
    pub(super) opened: Instant,
    pub(super) motion: PanelMotion,
    pub(super) toast: Option<(QrToast, Instant)>,
    pub(super) swap: Option<PatternSwap>,
    /// The card image to attach in the chat the share chooser opens.
    pub(super) share_image: Option<PathBuf>,
    /// A card image is being made for copy or share.
    pub(super) exporting: bool,
    _clock: Task<()>,
}

/// The QR box's part of the app.
pub(crate) struct PeerQrUi {
    pub(super) open: Option<OpenQr>,
    art: art::Art,
    pub(super) focus: FocusHandle,
    /// A shared card waiting for its chat's composer.
    pub(super) pending_attach: Option<(ChatId, PathBuf)>,
    /// The name's measured fit, kept until the name changes.
    pub(super) fit: Option<(String, Option<quill::peer_qr::layout::UsernameFit>)>,
}

impl PeerQrUi {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            open: None,
            art: art::Art::default(),
            focus: cx.focus_handle(),
            pending_attach: None,
            fit: None,
        }
    }
}

impl QuillApp {
    /// Open the box for `subject`, replacing any open one, and ask the
    /// session for what it shows: the colors, and for your own code the
    /// phone privacy rules and the message price.
    pub(super) fn open_qr_box(&mut self, subject: QrSubject, cx: &mut Context<Self>) {
        let own = matches!(
            subject,
            QrSubject::Peer(peer) if self.qr_is_self(peer)
        );
        let styled = matches!(subject, QrSubject::Peer(_));
        let clock = Self::spawn_main(cx, async move |this, cx| {
            loop {
                cx.background_executor().timer(CLOCK).await;
                let Ok(open) = this.update(cx, |this, cx| this.qr_clock(cx)) else {
                    return;
                };
                if !open {
                    return;
                }
            }
        });
        self.peer_qr.open = Some(OpenQr {
            state: PeerQrBox::new(subject),
            opened: Instant::now(),
            motion: PanelMotion::IDLE,
            toast: None,
            swap: None,
            share_image: None,
            exporting: false,
            _clock: clock,
        });
        if let Some(live) = self.live.as_mut() {
            if styled {
                let _ = live.driver.fetch_share_theme_backdrops();
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
        self.ensure_qr_contact_link();
        cx.notify();
    }

    pub(super) fn close_qr_box(&mut self, cx: &mut Context<Self>) {
        self.peer_qr.open = None;
        self.peer_qr.art.clear();
        cx.notify();
    }

    /// Run `action` through the box's reducer and do what it asks.
    pub(super) fn qr_act(&mut self, action: QrAction, cx: &mut Context<Self>) {
        let env = self.qr_env();
        let now = Instant::now();
        let Some(open) = self.peer_qr.open.as_mut() else {
            return;
        };
        let picks_before = open.state.emoji.clone();
        let panel_before = open.state.panel;
        let effects = open.state.apply(action, env);
        if open.state.emoji != picks_before {
            open.swap = Some(PatternSwap {
                previous: picks_before,
                started: now,
            });
        }
        if open.state.panel != panel_before {
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
        self.run_qr_effects(effects, cx);
        cx.notify();
    }

    fn run_qr_effects(&mut self, effects: Vec<QrEffect>, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                QrEffect::RequestFreeCode(request) => {
                    let sent = self
                        .live
                        .as_mut()
                        .is_some_and(|live| live.driver.fetch_own_user_link(Some(request)).is_ok());
                    if !sent {
                        self.qr_act(
                            QrAction::FreeAnswer {
                                request,
                                code: None,
                            },
                            cx,
                        );
                    }
                }
                QrEffect::Toast(toast) => self.show_qr_toast(toast, cx),
            }
        }
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
                    && open.toast.is_some_and(|(_, at)| at == shown)
                {
                    open.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Once a second while the box is open: a temporary code may have
    /// run out, a countdown moved. `false` once the box is closed.
    fn qr_clock(&mut self, cx: &mut Context<Self>) -> bool {
        if self.peer_qr.open.is_none() {
            return false;
        }
        self.ensure_qr_contact_link();
        self.qr_act(QrAction::Tick, cx);
        true
    }

    /// Poll step: hand the box the free-code answers that came in.
    pub(super) fn drain_peer_qr(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let answers = live.driver.session.users_state.peer_qr.take_free_answers();
        if answers.is_empty() {
            return false;
        }
        for (request, code) in answers {
            self.qr_act(QrAction::FreeAnswer { request, code }, cx);
        }
        true
    }

    /// Your own box with no username and a hidden number shows a
    /// temporary contact link; ask for one when there is none (or it ran
    /// out). The driver skips the request while one is on its way.
    fn ensure_qr_contact_link(&mut self) {
        let needs = self
            .peer_qr
            .open
            .as_ref()
            .and_then(|open| self.qr_own_link(&open.state.subject))
            .is_some_and(|state| state == quill::peer_qr::link::OwnLinkState::NeedsContactLink);
        if needs && let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_own_user_link(None);
        }
    }

    /// The box hides under the share chooser; a cancelled share brings it
    /// back as it was.
    pub(super) fn qr_share_cancelled(&mut self) {
        if let Some(open) = self.peer_qr.open.as_mut() {
            open.share_image = None;
        }
    }

    /// A chat was picked for the shared link: the box is done, and the
    /// card waits to go into that chat's composer.
    pub(super) fn qr_share_chosen(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(open) = self.peer_qr.open.as_mut() else {
            return;
        };
        self.peer_qr.pending_attach = open.share_image.take().map(|path| (chat_id, path));
        self.close_qr_box(cx);
    }

    /// The shared link's chat is open: attach the card there.
    pub(super) fn attach_shared_qr_card(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        match self.peer_qr.pending_attach.take() {
            Some((chat, path)) if chat == chat_id => self.attach_dropped_files(&[path], cx),
            _ => {}
        }
    }
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

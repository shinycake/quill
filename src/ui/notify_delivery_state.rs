//! Notification delivery: OS clicks, in-flight workers, reply targets,
//! photo waits, sounds, the poll's redraw batching and Quill's own
//! notification windows. All of it is the app's: each item names the
//! account it is for, which need not be the one on screen. The mute, sound
//! and auto-delete menus a person opens for the account on screen are
//! `notify_state::NotifyUi`, on `OnScreen`.
#![deny(clippy::indexing_slicing)]

use quill::ids::ChatId;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

pub(crate) struct NotifyDelivery {
    /// Phase 8.1: clicks on notifications of the account on screen, waiting
    /// for the next render to open the chat. Each names the account it was
    /// shown for; one whose account is no longer on screen goes back to
    /// `background.clicks` instead of running here.
    pub(super) notify_clicks: Arc<Mutex<Vec<quill::notify_accounts::ChatClick>>>,
    /// Phase 8.1: in-flight OS notification workers; capped so a message
    /// burst cannot stack threads.
    pub(super) notify_inflight: Arc<AtomicUsize>,
    /// The message each shown notification announces, by the account it
    /// was shown for and the chat: an inline reply typed into it answers
    /// that message (`quill::notify::plan_inline_reply`).
    pub(super) reply_targets:
        std::collections::HashMap<(quill::ids::AccountKey, ChatId), quill::ids::MessageId>,
    /// Notifications waiting for their photo (`notifications::photo_preview`),
    /// by the account they are for and the chat. A newer notification for
    /// the chat, or its dismissal, removes the entry and the wait ends
    /// without showing.
    pub(super) preview_waits: std::collections::HashMap<(quill::ids::AccountKey, ChatId), u64>,
    pub(super) next_preview_wait: u64,
    pub(super) notification_sounds: super::audio::NotificationSounds,
    /// Redraws the TDLib poll asked for, batched by urgency
    /// (`notifications::PolledRedraw`).
    pub(super) polled_redraw: super::notifications::PolledRedraw,
    /// Quill's own notification windows (`notify_windows`).
    pub(super) windows: super::notify_windows::NotifyWindows,
}

impl NotifyDelivery {
    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notify_inflight: Arc::new(AtomicUsize::new(0)),
            reply_targets: std::collections::HashMap::new(),
            preview_waits: std::collections::HashMap::new(),
            next_preview_wait: 0,
            notification_sounds: super::audio::NotificationSounds::new(audio_output.clone()),
            polled_redraw: super::notifications::PolledRedraw::new(std::time::Instant::now()),
            windows: super::notify_windows::NotifyWindows::new(),
        }
    }
}

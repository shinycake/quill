//! One-to-one call UI: the call window, tick, sounds, images and notifications.

use gpui_kit::*;
use std::sync::Arc;
use std::sync::Mutex;

pub(crate) struct CallUi {
    /// Phase C2e/C2l: decoded video tiles cached by `(frame seq,
    /// is_screen)`, rebuilt only when the key changes. The peer's
    /// screen and camera streams share `seq` numbering (all demo
    /// fixtures use seq 0), so `is_screen` is part of the key — a
    /// seq-only key would cross-render.
    pub(super) remote_image: Option<((u64, bool), Arc<RenderImage>)>,
    pub(super) local_image: Option<((u64, bool), Arc<RenderImage>)>,
    /// Accept / Decline / body picks on incoming-call notifications.
    pub(super) notify_clicks: Arc<Mutex<Vec<(i32, quill::notify_call::CallNotificationAction)>>>,
    /// The incoming call last announced with a system notification.
    pub(super) notified: Option<i32>,
    /// Phase C1: whether the call-duration 1s tick task is running
    /// (keeps the overlay's ringing/connected clock fresh). Mirrors
    /// `voice_tick`.
    pub(super) tick_active: bool,
    /// The call window (tdesktop's call panel), and its bookkeeping: a
    /// call whose window you closed stays closed until the call bar
    /// reopens it; an incoming call raises it once.
    pub(super) window: Option<AnyWindowHandle>,
    pub(super) window_opening: bool,
    pub(super) window_raised: bool,
    pub(super) window_closed_by_user: Option<i32>,
    pub(super) ended_at: Option<(i32, std::time::Instant)>,
    pub(super) sounds: super::call_sounds::CallSounds,
    pub(super) sound_marks: super::call_sounds::SoundMarks,
    /// Phase C2i: pending "call again" / profile-call confirmation when
    /// the confirm-before-calling pref is on: `(user_id, is_video)`.
    pub(super) confirm: Option<(i64, bool)>,
}

impl CallUi {
    pub(super) fn new(audio_output: super::audio::SharedOutput) -> Self {
        Self {
            remote_image: None,
            local_image: None,
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notified: None,
            tick_active: false,
            window: None,
            window_opening: false,
            window_raised: false,
            window_closed_by_user: None,
            ended_at: None,
            sounds: super::call_sounds::CallSounds::new(audio_output),
            sound_marks: Default::default(),
            confirm: None,
        }
    }
}

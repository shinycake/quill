//! One-to-one call UI: the call window, tick, sounds, pictures and notifications.
#![deny(clippy::indexing_slicing)]

use super::video_player::VideoPlayer;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;

pub(crate) struct CallUi {
    /// The call window's live players (`calls::call_video`): the peer's
    /// screen share or camera, and your camera (mirrored), each following
    /// its feed.
    pub(super) remote_player: Option<VideoPlayer>,
    pub(super) local_player: Option<VideoPlayer>,
    /// Accept / Decline / body picks on incoming-call notifications.
    pub(super) notify_clicks: Arc<Mutex<Vec<(i32, quill::notify_call::CallNotificationAction)>>>,
    /// The incoming call last announced with a system notification.
    pub(super) notified: Option<i32>,
    /// Phase C1: whether the call-duration 1s tick task is running
    /// (keeps the overlay's ringing/connected clock fresh). Mirrors
    /// `voice_tick`.
    pub(super) tick_active: bool,
    /// The call window's panel view, while the window is open: the
    /// clock's tick redraws it alone.
    pub(super) panel_view: Rc<Cell<Option<EntityId>>>,
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
    /// The Calls tab's scroll region, read to load older calls on scroll.
    pub(super) history_scroll: ScrollHandle,
    pub(super) history_asked: super::scroll_load::Asked,
    /// A call waiting on microphone (and camera) permission before its
    /// `createCall`: `(user_id, is_video)`.
    pub(super) dialing: Option<(i64, bool)>,
    /// Settings > Calls: the microphone test and its level meter
    /// (`ui::call_audio_ui`), `Some` while testing.
    pub(super) mic_test: Option<super::call_audio_ui::MicTest>,
    /// The microphone test's device is being opened off the UI thread.
    pub(super) mic_test_opening: bool,
    /// "Add People" over the call window (`ui::call_add_people`), `Some`
    /// while the box is open.
    pub(super) add_people: Option<super::call_add_people::AddPeopleBox>,
    /// A conference call's link waiting on "Join Group Call"
    /// (`ui::call_conference_ui`).
    pub(super) conference_link: Option<String>,
}

impl CallUi {
    /// The call window is gone (closed by the user or by the app): drop its
    /// handle and its panel view, so the clock's tick has no dead view to
    /// notify.
    pub(super) fn forget_window(&mut self) {
        self.window = None;
        self.panel_view.set(None);
    }

    pub(super) fn new(audio_output: super::audio::SharedOutput) -> Self {
        Self {
            remote_player: None,
            local_player: None,
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notified: None,
            tick_active: false,
            panel_view: Rc::default(),
            window: None,
            window_opening: false,
            window_raised: false,
            window_closed_by_user: None,
            ended_at: None,
            sounds: super::call_sounds::CallSounds::new(audio_output),
            sound_marks: Default::default(),
            confirm: None,
            history_scroll: ScrollHandle::new(),
            history_asked: Default::default(),
            dialing: None,
            mic_test: None,
            mic_test_opening: false,
            add_people: None,
            conference_link: None,
        }
    }
}

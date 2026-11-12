//! One-to-one call UI: the call window, tick, sounds, pictures and notifications.
#![deny(clippy::indexing_slicing)]

use super::video_player::VideoPlayer;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;

/// The 1s call clock: at most one task, which exits when unwanted. The
/// stage holds the task, so replacing it ends the clock.
#[derive(Debug, Default)]
pub(crate) enum CallTick {
    #[default]
    Idle,
    Running {
        _task: Task<()>,
    },
}

impl CallTick {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, CallTick::Running { .. })
    }
}

/// The call window: not there, being opened (at most one open at a time),
/// or open.
#[derive(Debug, Default)]
pub(crate) enum CallWindow {
    #[default]
    Closed,
    /// The open is queued; the stage holds the task that does it, so
    /// leaving the stage cancels the open.
    Opening {
        _open: Task<()>,
    },
    Open(AnyWindowHandle),
}

impl CallWindow {
    /// The open window's handle.
    pub(crate) fn handle(&self) -> Option<AnyWindowHandle> {
        match self {
            CallWindow::Open(handle) => Some(*handle),
            CallWindow::Closed | CallWindow::Opening { .. } => None,
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        matches!(self, CallWindow::Closed)
    }
}

/// The ended call the window lingers on.
#[derive(Debug, Default)]
pub(crate) enum CallEnd {
    /// A screen no sync has looked at: a call summary the session already
    /// holds (the screen was built after the call ended, as on a switch
    /// back to the account) is not a new ending. The first sync seeds the
    /// stage from the target.
    #[default]
    Unobserved,
    /// No linger runs: no call ended, or a busy line's summary this screen
    /// saw, which stays until Cancel or Redial (`CallWindowTarget::lingers`).
    NotEnded,
    /// The call ended; the window lingers on "call ended" until the timer
    /// fires. The stage holds the timer, so a new call or a switch of
    /// account drops it.
    Lingering {
        call_id: i32,
        _timer: Task<()>,
    },
    /// The linger is over, or the summary was there before this screen
    /// looked.
    Over {
        call_id: i32,
    },
}

impl CallEnd {
    pub(crate) fn call_id(&self) -> Option<i32> {
        match self {
            CallEnd::Unobserved | CallEnd::NotEnded => None,
            CallEnd::Lingering { call_id, .. } | CallEnd::Over { call_id } => Some(*call_id),
        }
    }
}

/// Whether the call window may open for the call it shows. One value for
/// the account's call: the next call's window starts from `Open`
/// (`sync_call_window` resets it when no call shows).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WindowWanted {
    #[default]
    Open,
    /// The user closed the window of this call: it stays shut (a second
    /// call ringing, or the call bar, reopens it).
    ClosedByUser(i32),
    /// The window of this call could not be opened: not tried again on
    /// every sync, not even while a second call rings (the call bar or a
    /// notification's Open tries once more).
    OpenFailed(i32),
}

pub(crate) struct CallUi {
    /// The call window's live players (`calls::call_video`): the peer's
    /// screen share or camera, and your camera (mirrored), each following
    /// its feed.
    pub(super) remote_player: Option<VideoPlayer>,
    pub(super) local_player: Option<VideoPlayer>,
    /// Accept / Decline / body picks on incoming-call notifications, each
    /// with the account whose call rang.
    pub(super) notify_clicks: Arc<Mutex<Vec<quill::notify_call::CallPick>>>,
    /// The incoming call last announced with a system notification, and
    /// the account it rang on (call ids are per account).
    pub(super) notified: Option<(quill::ids::AccountKey, i32)>,
    /// Phase C1: the call-duration 1s tick task (keeps the overlay's
    /// ringing/connected clock fresh). Mirrors `voice_tick`.
    pub(super) tick: CallTick,
    /// The call window's panel view, while the window is open: the
    /// clock's tick redraws it alone.
    pub(super) panel_view: Rc<Cell<Option<EntityId>>>,
    /// The call window (tdesktop's call panel), and its bookkeeping: a
    /// call whose window you closed stays closed until the call bar
    /// reopens it or a second call rings.
    pub(super) window: CallWindow,
    pub(super) sounds: super::call_sounds::CallSounds,
}

/// What the account on screen remembers about its 1:1 call: the call
/// whose window you closed, when the window's call ended, and which
/// sounds that call already played. TDLib numbers calls per client from 1,
/// so kept app-wide these would match the next account's call of the same
/// id: its window would stay shut and its chimes would be skipped. The
/// call itself does not outlive the screen (`retire_live_to_background`
/// closes a client in a call). A field of `OnScreen`.
#[derive(Default)]
pub(crate) struct CallScreen {
    pub(super) wanted: WindowWanted,
    /// Each call that rings in raises the open window once: the last one
    /// that did. Per account, as call ids are.
    pub(super) raised_for: Option<i32>,
    /// The window's ended call and its linger.
    pub(super) end: CallEnd,
    pub(super) sound_marks: super::call_sounds::SoundMarks,
}

/// Ids and marks of the account's own calls are dropped with the screen;
/// the call window follows the session on screen and closes on the next
/// sync. The screen share chooser and its privacy box name a call of this
/// account (`ScreenFor::Private` holds its id), which ends with the
/// switch, so they close here.
impl super::on_screen::Farewell for CallScreen {
    fn farewell(
        app: &mut super::app::QuillApp,
        _window: &mut Window,
        cx: &mut Context<super::app::QuillApp>,
    ) {
        app.close_screen_chooser(cx);
        app.screen_share.close_privacy();
        // The ring belonged to this account's call; the next sync starts
        // one for a call ringing on the account now on screen.
        app.calls.sounds.keep_looping(None);
    }
}

/// The boxes the account on screen opens around calls. A field of
/// `OnScreen`.
pub(crate) struct CallBoxes {
    /// Phase C2i: pending "call again" / profile-call confirmation when
    /// the confirm-before-calling pref is on: `(user_id, is_video)`.
    pub(super) confirm: Option<(i64, bool)>,
    /// The Calls tab's scroll region, read to load older calls on scroll.
    pub(super) history_scroll: ScrollHandle,
    pub(super) history_asked: super::scroll_load::Asked,
    /// Settings > Calls: the microphone test and its level meter
    /// (`ui::call_audio_ui`).
    pub(super) mic_test: super::call_audio_ui::MicTestRun,
    pub(super) open_mic_test: super::call_audio_ui::OpenMicTest,
    /// "Add People" over the call window (`ui::call_add_people`), `Some`
    /// while the box is open.
    pub(super) add_people: Option<super::call_add_people::AddPeopleBox>,
    /// A conference call's link waiting on "Join Group Call"
    /// (`ui::call_conference_ui`).
    pub(super) conference_link: Option<String>,
    /// The window that pressed Call (`None`: the main window): a failure
    /// to start and the rating afterwards show there
    /// (`QuillApp::call_card_window`). An answered call's card is the
    /// main window's.
    pub(super) card_window: Option<quill::ids::ChatId>,
}

impl CallBoxes {
    pub(super) fn new() -> Self {
        Self {
            confirm: None,
            history_scroll: ScrollHandle::new(),
            history_asked: Default::default(),
            mic_test: Default::default(),
            open_mic_test: super::call_audio_ui::open_default_mic_test(),
            add_people: None,
            conference_link: None,
            card_window: None,
        }
    }
}

/// The microphone test's device closes with the value; the boxes hold ids
/// and flags.
/// The call link's leave confirm is the session's
/// (`calls.call_link_stage`): it is cancelled here, or it would come
/// back on switch-back.
impl super::on_screen::Farewell for CallBoxes {
    fn farewell(
        app: &mut super::app::QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<super::app::QuillApp>,
    ) {
        if app
            .session()
            .is_some_and(|session| session.calls.call_link_stage.confirm().is_some())
        {
            app.cancel_call_link_leave(cx);
        }
        // The Add People box and its search go with the account's call.
        app.end_call_add_people();
        // A confirmed link waiting for its call to end is abandoned too: it
        // must not open a join screen after switching back.
        if let Some(live) = app.live.as_mut() {
            live.driver.session.calls.call_link_stage.abandon();
        }
    }
}

impl CallUi {
    /// The call window is gone (closed by the user or by the app): drop its
    /// handle and its panel view, so the clock's tick has no dead view to
    /// notify.
    pub(super) fn forget_window(&mut self) {
        if matches!(self.window, CallWindow::Open(_)) {
            self.window = CallWindow::Closed;
        }
        self.panel_view.set(None);
    }

    pub(super) fn new(audio_output: super::audio::SharedOutput) -> Self {
        Self {
            remote_player: None,
            local_player: None,
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notified: None,
            tick: CallTick::Idle,
            panel_view: Rc::default(),
            window: CallWindow::Closed,
            sounds: super::call_sounds::CallSounds::new(audio_output),
        }
    }
}

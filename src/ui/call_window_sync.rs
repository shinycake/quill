//! The 1:1 call window follows its call: it opens for a new call, comes to
//! the front when one rings in, stays shut if you closed it, and lingers
//! briefly on "call ended". Split from `call_panel.rs` (file size).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::calls_state::{CallEnd, WindowWanted};
use gpui_kit::*;
use quill::telegram::envelope::CallState;
use std::time::Duration;

/// How long an ended call's window lingers on "call ended".
pub(super) const ENDED_LINGER: Duration = Duration::from_millis(1600);

/// What the 1:1 window shows: one value, read from the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum CallWindowTarget {
    None,
    /// The active call.
    Active {
        id: i32,
    },
    /// An incoming call rings.
    Ringing {
        id: i32,
    },
    /// The ended call's summary.
    Ended {
        id: i32,
    },
    /// The summary of a line that was busy.
    Busy {
        id: i32,
    },
    /// A conference invitation rings, under its invite key.
    Invite {
        key: i32,
    },
}

impl CallWindowTarget {
    /// The call the window shows.
    pub(in crate::ui) fn id(self) -> Option<i32> {
        match self {
            Self::None => None,
            Self::Active { id } | Self::Ringing { id } | Self::Ended { id } | Self::Busy { id } => {
                Some(id)
            }
            Self::Invite { key } => Some(key),
        }
    }

    /// The call that rings in, if any.
    fn ringing(self) -> Option<i32> {
        match self {
            Self::Ringing { id } => Some(id),
            Self::Invite { key } => Some(key),
            _ => None,
        }
    }

    /// The call has ended (its summary shows).
    fn ended(self) -> bool {
        matches!(self, Self::Ended { .. } | Self::Busy { .. })
    }
}

impl QuillApp {
    /// Open, raise or close the call window to follow the call: it opens
    /// for a new call (and comes to the front when one rings in), stays
    /// shut if you closed it, and lingers briefly on "call ended". A second
    /// call ringing during the call opens and raises it even when you
    /// closed it: its Decline and End & Answer are there
    /// (`swap_call_window_card`), and a minimized main window shows none.
    pub(in crate::ui) fn sync_call_window(&mut self, cx: &mut Context<Self>) {
        self.close_stale_call_add_people();
        let target = self.call_window_target();
        let call_id = target.id();
        let second = self
            .session()
            .and_then(|session| session.calls.second_call.prompt())
            .map(|(id, _, _)| id);
        let ringing = second.or(target.ringing());
        self.follow_call_end(target, cx);
        // No call at all: what a closed or failed window wanted ended with
        // the call.
        if target == CallWindowTarget::None {
            self.call_screen.wanted = WindowWanted::Open;
        }
        let ended = target.ended();
        let lingering = self.call_screen.end.lingers();
        let wanted = call_id.filter(|id| {
            (!ended || lingering) && window_allowed(*id, self.call_screen.wanted, second.is_some())
        });
        match (wanted, self.calls.window.handle()) {
            (Some(_), Some(handle)) => {
                if ringing.is_some() && self.call_screen.raised_for != ringing {
                    self.call_screen.raised_for = ringing;
                    let _ = handle.update(cx, |_, window, _| window.activate_window());
                }
            }
            (Some(id), None) => self.open_call_window(id, cx),
            (None, Some(handle)) => {
                self.calls.forget_window();
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            (None, None) => {}
        }
        if wanted.is_none() {
            self.release_call_players(cx);
        }
    }

    /// The window's ended call: it lingers on "call ended" under a timer
    /// the stage holds (a busy line stays until its buttons dismiss it); a
    /// call still going, or none, ends the stage and drops the timer.
    fn follow_call_end(&mut self, target: CallWindowTarget, cx: &mut Context<Self>) {
        match target {
            CallWindowTarget::Ended { id } if self.call_screen.end.call_id() != Some(id) => {
                self.call_screen.end = CallEnd::Lingering {
                    call_id: id,
                    _timer: self.linger_timer(cx),
                };
            }
            CallWindowTarget::Busy { id } if self.call_screen.end.call_id() != Some(id) => {
                self.call_screen.end = CallEnd::Busy { call_id: id };
            }
            CallWindowTarget::Ended { .. } | CallWindowTarget::Busy { .. } => {}
            _ => self.call_screen.end = CallEnd::Running,
        }
    }

    /// Closes the window once the linger is over, whichever window draws.
    fn linger_timer(&self, cx: &mut Context<Self>) -> Task<()> {
        // window-scope: the call window is the app's, no chat window's.
        // screen-scope: held by the screen's `CallEnd`, so a switch drops it.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ENDED_LINGER).await;
            let _ = this.update(cx, |this, cx| {
                if let CallEnd::Lingering { call_id, .. } = &this.call_screen.end {
                    let call_id = *call_id;
                    this.call_screen.end = CallEnd::Over { call_id };
                }
                this.sync_call_windows(cx);
                cx.notify();
            });
        })
    }

    /// The 1:1 and group call windows follow their calls, and what was
    /// opened for a call (the screen chooser, its access box, the video
    /// chat's boxes) goes when the call or its window does. Runs from the
    /// poll and from every window's frame: a hidden or minimized main
    /// window does not draw, and an incoming call still needs its window
    /// while you work in a chat window.
    pub(in crate::ui) fn sync_call_windows(&mut self, cx: &mut Context<Self>) {
        self.take_offered_call_link(cx);
        self.sync_call_window(cx);
        self.sync_group_call_window(cx);
        self.sync_screen_share_call(cx);
    }

    /// The call the 1:1 window shows: the active call (ringing, when it
    /// is incoming and pending), its ended summary, or a ringing
    /// conference invite.
    pub(in crate::ui) fn call_window_target(&self) -> CallWindowTarget {
        let Some(session) = self.session() else {
            return CallWindowTarget::None;
        };
        match (
            &session.calls.active_call,
            crate::ui::call_conference_ui::shown_call_summary(session),
        ) {
            (Some(call), _)
                if matches!(call.phase(), CallState::Pending { .. }) && !call.is_outgoing =>
            {
                CallWindowTarget::Ringing { id: call.id }
            }
            (Some(call), _) => CallWindowTarget::Active { id: call.id },
            (None, Some(summary)) if summary.busy() => CallWindowTarget::Busy {
                id: summary.call_id,
            },
            (None, Some(summary)) => CallWindowTarget::Ended {
                id: summary.call_id,
            },
            _ => match crate::ui::call_conference_ui::ringing_invite_key(session) {
                Some(key) => CallWindowTarget::Invite { key },
                None => CallWindowTarget::None,
            },
        }
    }

    /// Bring the call window back (the call bar's click).
    pub(in crate::ui) fn show_call_window(&mut self, cx: &mut Context<Self>) {
        self.call_screen.wanted = WindowWanted::Open;
        match self.calls.window.handle() {
            Some(handle) => {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            }
            // Pressed in a chat window: the window opens now, not at the
            // main window's next frame.
            None => self.sync_call_windows(cx),
        }
        cx.notify();
    }
}

/// Whether the call's window may open: not after you closed it (a second
/// call ringing reopens it for its Decline and End & Answer), and not for
/// a call whose window already failed to open.
pub(in crate::ui) fn window_allowed(call: i32, wanted: WindowWanted, second_ringing: bool) -> bool {
    match wanted {
        WindowWanted::Open => true,
        WindowWanted::ClosedByUser(closed) => closed != call || second_ringing,
        WindowWanted::OpenFailed(failed) => failed != call,
    }
}

#[cfg(test)]
mod tests {
    use super::{CallWindowTarget, WindowWanted, window_allowed};

    #[test]
    fn a_second_call_reopens_a_closed_window_but_not_a_failed_one() {
        assert!(window_allowed(1, WindowWanted::Open, false));
        assert!(!window_allowed(1, WindowWanted::ClosedByUser(1), false));
        assert!(window_allowed(1, WindowWanted::ClosedByUser(1), true));
        assert!(!window_allowed(1, WindowWanted::OpenFailed(1), true));
        // Another call's close or failure does not hold this one back.
        assert!(window_allowed(2, WindowWanted::OpenFailed(1), false));
        assert!(window_allowed(2, WindowWanted::ClosedByUser(1), false));
    }

    #[test]
    fn the_target_names_the_call_and_what_rings_or_ended() {
        assert_eq!(CallWindowTarget::None.id(), None);
        assert_eq!(CallWindowTarget::Active { id: 3 }.ringing(), None);
        assert_eq!(CallWindowTarget::Ringing { id: 3 }.ringing(), Some(3));
        assert_eq!(CallWindowTarget::Invite { key: 9 }.ringing(), Some(9));
        assert_eq!(CallWindowTarget::Invite { key: 9 }.id(), Some(9));
        assert!(CallWindowTarget::Busy { id: 4 }.ended());
        assert!(CallWindowTarget::Ended { id: 4 }.ended());
        assert!(!CallWindowTarget::Ringing { id: 4 }.ended());
    }
}

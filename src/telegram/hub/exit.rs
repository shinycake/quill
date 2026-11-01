//! Process exit while TDLib may still be working.
//!
//! `exit` runs TDLib's static destructors, and TDLib aborts if a client it
//! still serves runs on its workers then. A quit that ran past
//! `QUIT_CLOSE_DEADLINE` (a large database flush, a stalled network, several
//! accounts) reaches `exit` like that, and on macOS the abort shows "Quill
//! quit unexpectedly" for a quit the user asked for. So the exit hook ends
//! the process with `_exit` while any client made here is not confirmed
//! Closed. TDLib's SQLite and binlog are crash-safe, so this leaves the files
//! no worse than the abort did. Quill's own quit work runs in `on_app_quit`,
//! before `exit`.

use super::{EXIT_STOP_WAIT, SHARED};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{AuthPayload, AuthorizationState, EnvelopePayload};
use crate::telegram::ffi::TdJson;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Client ids made in this process that TDLib has not confirmed Closed.
static MAY_BE_OPEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// The status a process that leaves through `_exit` ends with: `atexit`
/// hooks never learn the one `exit` was given.
static EXIT_STATUS: AtomicI32 = AtomicI32::new(0);

/// Record the status the coming `exit` is given. Call it before every
/// `std::process::exit` that may run with a client made; a quit through
/// `cx.quit()` exits with 0, the default.
pub fn set_exit_status(status: i32) {
    EXIT_STATUS.store(status, Ordering::SeqCst);
}

/// `client_id` was just made: TDLib may serve it until it says Closed.
pub(crate) fn note_made(client_id: i32) {
    crate::locks::lock_or_recover(&MAY_BE_OPEN).push(client_id);
}

/// What the receive loop read: an `authorizationStateClosed` ends its
/// client's claim on TDLib's workers.
pub(super) fn note_received(owned: &OwnedEnvelope) {
    let closed = matches!(
        owned.envelope.payload,
        EnvelopePayload::Auth(AuthPayload::UpdateAuthorizationState(
            AuthorizationState::Closed
        ))
    );
    if let (true, Some(client_id)) = (closed, owned.client_id) {
        crate::locks::lock_or_recover(&MAY_BE_OPEN).retain(|id| *id != client_id);
    }
}

/// Whether TDLib may still serve a client. A slot held elsewhere counts as
/// yes: exit never blocks on it.
fn tdlib_may_be_busy() -> bool {
    crate::locks::try_lock_or_recover(&MAY_BE_OPEN).is_none_or(|open| !open.is_empty())
}

static EXIT_HOOK: std::sync::Once = std::sync::Once::new();

/// Set when the wake ([`TdJson::wake_receive`]) did not get TDLib's answer
/// for a client it never made: then only a short `td_receive` lets a stop
/// end it in time, before exit tears TDLib down under the thread.
static WAKE_FAILED: AtomicBool = AtomicBool::new(false);

/// What TDLib answers the wake with.
fn wakes(answer: Option<&str>) -> bool {
    answer.is_some_and(|answer| answer.contains("Invalid TDLib instance"))
}

/// Seconds one `td_receive` call may block: `long` while the wake works,
/// the 0.2 s it was before the wake otherwise.
pub(super) fn receive_timeout_secs(long: f64) -> f64 {
    if WAKE_FAILED.load(Ordering::Relaxed) {
        0.2
    } else {
        long
    }
}

/// Stop the shared hub's receive thread when the process exits, before
/// TDLib destroys its static client manager under it, and skip that
/// teardown while a client may be open. Call right after a client id is
/// made and before its route exists: `atexit` hooks and static destructors
/// run in reverse order of registration, and TDLib registers the manager's
/// destructor then, so a hook registered after that runs first.
///
/// TDLib's function-local statics go in that order too, so one first used
/// after the hook is registered is gone when the hook runs, and a `td_send`
/// in the hook aborts on it. The wake therefore runs once here, and
/// `td_receive` is free because no route exists yet. TDLib answers the wake
/// inside `td_send`, so its answer also says whether the wake works.
pub fn stop_shared_before_tdlib_teardown(api: &TdJson) {
    EXIT_HOOK.call_once(|| {
        api.wake_receive();
        if !wakes(api.receive(0.0).as_deref()) {
            WAKE_FAILED.store(true, Ordering::Relaxed);
            log::warn!("TDLib did not answer the receive wake; td_receive polls every 0.2 s");
        }
        register_exit_hook();
    });
}

fn register_exit_hook() {
    // SAFETY: `stop_shared_at_exit` is a plain `extern "C" fn()` that
    // catches every panic. A failed registration leaves exit as before.
    let _ = unsafe { libc::atexit(stop_shared_at_exit) };
}

/// Runs inside `exit`, so it never calls `std::thread::current()`: on macOS
/// that ends the process silently once `exit` has torn down thread locals.
extern "C" fn stop_shared_at_exit() {
    crate::thread_guard::ffi_boundary((), || {
        // Never block exit on the slot. It is held only while a hub is
        // being made; that rare quit keeps the old behaviour.
        let hub = crate::locks::try_lock_or_recover(&SHARED).and_then(|slot| slot.clone());
        if let Some(hub) = hub {
            hub.stop_within(EXIT_STOP_WAIT);
        }
        if tdlib_may_be_busy() {
            leave_without_teardown();
        }
    });
}

/// `_exit` skips every `atexit` hook registered before this one. Quill's
/// call reaper registers at quit, so it has run already; the scratch
/// directories may have registered first, so they go here.
fn leave_without_teardown() {
    crate::scratch::remove_all();
    // SAFETY: `_exit` takes a plain status and never returns.
    unsafe { libc::_exit(EXIT_STATUS.load(Ordering::SeqCst)) }
}

#[cfg(test)]
#[path = "exit_tests.rs"]
mod tests;

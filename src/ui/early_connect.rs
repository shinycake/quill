//! The run's first connection, begun in `main` before AppKit and the window
//! start. A worker does what `start_connection` does before a client opens
//! (the startup sweep, the database key from the keychain, the account
//! folders) and loads TDLib and the call engine's library; [`open_if_ready`]
//! then opens the client on the main thread, so TDLib reads its database and
//! the chat list while the window is built. Begun from `QuillApp::new`, all
//! of it waited for the window instead.
#![deny(clippy::indexing_slicing)]

use super::background_accounts::deleting::{Sweep, live_secrets};
use super::connect_ui::{Prepared, prepare_startup};
use futures_channel::oneshot;
use quill::connect::{ConnectBlocker, LiveConnect, start_prepared_live_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::DiscardSink;
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;

/// How far the first connection got before `QuillApp` asked for it.
pub(super) enum First {
    /// The preparation on the worker: `None` when it left the connection to
    /// `start_connection` (a local passcode, or no app folder).
    Preparing(oneshot::Receiver<Option<Prepared>>),
    Opened {
        /// The folders the startup sweep deleted.
        swept: (PathBuf, Vec<quill::ids::AccountKey>),
        client: Box<Result<LiveConnect, ConnectBlocker>>,
    },
}

thread_local! {
    /// Main thread only: the opened client is bound to it.
    static FIRST: Cell<Option<First>> = const { Cell::new(None) };
}

/// Prepare the active account's connection on a worker thread. Call once,
/// on the main thread, before the application starts.
pub fn begin(credentials: Option<TelegramCredentials>) {
    let (done, early) = oneshot::channel();
    let spawned = quill::thread_guard::spawn_guarded("quill-early-connect", move || {
        let prepared = prepare(credentials);
        quill::startup_trace::mark("early_prepared");
        let _ = done.send(prepared);
        // After the send: the client must not wait for the call engine.
        quill::calls::engine::preload_tgcalls();
    });
    if spawned.is_ok() {
        FIRST.set(Some(First::Preparing(early)));
    }
}

/// What `start_connection` does on a worker before the first client opens.
fn prepare(credentials: Option<TelegramCredentials>) -> Option<Prepared> {
    let root = quill::settings::safe_app_root()?;
    // With a local passcode the database key is wrapped: the connection
    // starts after the first unlock.
    if quill::passcode::load_config(&root).is_some() {
        return None;
    }
    let secrets = live_secrets();
    let swept = Sweep::pending(root.clone(), secrets.clone()).run();
    let prepared = prepare_startup(credentials, Some(root), secrets().as_ref(), &swept.1);
    if prepared.is_ok() {
        // The client opens on the main thread, over the library loaded here.
        let _ = quill::telegram::hub::TdHub::shared(Arc::new(DiscardSink));
    }
    Some((Some(swept), prepared))
}

/// Open the first client if its preparation is done; otherwise leave it
/// to `start_connection`. Main thread.
pub fn open_if_ready() {
    // Taken out for the whole call, so opening the client can never meet
    // a held borrow; whatever is still waiting goes back at the end.
    let mut early = match FIRST.take() {
        Some(First::Preparing(early)) => early,
        other => {
            FIRST.set(other);
            return;
        }
    };
    match early.try_recv() {
        Ok(Some(Some((Some(swept), prepared)))) => {
            let client = prepared.and_then(|(credentials, prepared)| {
                start_prepared_live_connect(credentials, prepared, Arc::new(DiscardSink))
            });
            FIRST.set(Some(First::Opened {
                swept,
                client: Box::new(client),
            }));
        }
        // Still preparing.
        Ok(None) => FIRST.set(Some(First::Preparing(early))),
        // Left to `start_connection`, or the worker died: it prepares again.
        Ok(Some(_)) | Err(_) => {}
    }
}

/// The early connection, for the run's first connection only.
pub(super) fn take() -> Option<First> {
    FIRST.take()
}

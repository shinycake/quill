//! Follows the macOS accessibility options that decide whether glass is
//! drawn (Reduce transparency) and how (Increase contrast). They are read
//! once at startup and again whenever the system posts
//! `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`. The
//! notification block only stores the new values and pokes
//! [`quill::wake::GLASS`]; a task on the UI executor then redraws the
//! windows, so nothing blocks and nothing unwinds into Objective-C.
//! `QUILL_REDUCE_TRANSPARENCY=1` and `QUILL_INCREASE_CONTRAST=1` switch the
//! options on for screenshot demos. Off macOS the options are always off.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::glass::{self, OsTransparency};

/// Read the options and start following them. Call once, at startup.
pub(crate) fn start(cx: &mut App) {
    #[cfg(target_os = "macos")]
    {
        glass::set_os(mac::read().with_env_overrides());
        mac::observe();
        cx.spawn(async move |cx| {
            let mut seen = quill::wake::GLASS.epoch();
            loop {
                seen = quill::wake::GLASS.wait_since(seen).await;
                cx.update(|cx| cx.refresh_windows());
            }
        })
        .detach();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cx;
        glass::set_os(OsTransparency::default().with_env_overrides());
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{OsTransparency, glass};
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;

    pub(super) fn read() -> OsTransparency {
        let workspace = NSWorkspace::sharedWorkspace();
        OsTransparency {
            reduce_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
            increase_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
        }
    }

    /// Watch for changes for the rest of the process's life.
    pub(super) fn observe() {
        let block = RcBlock::new(|_: NonNull<NSNotification>| {
            glass::set_os(read().with_env_overrides());
            quill::wake::GLASS.poke();
        });
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        // SAFETY: the name is the system's notification constant, there is
        // no sender filter or queue (the block runs on the posting thread),
        // and the block only touches atomics and a wake signal, so it is
        // safe to run on any thread and cannot unwind.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                None,
                None,
                &block,
            )
        };
        // The observer stays registered until the process exits.
        std::mem::forget(token);
    }
}

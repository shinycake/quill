//! Viewer playback and chrome: video start decision, control auto-hide
//! timing, the Delete gate, seeking and keyboard shortcuts.

use super::*;

/// Parity slice 5: viewer video start decision (pure, testable).
///
/// `clip_local` is whether the clip file (`play_file_id`) is downloaded.
/// The UI layer (`maybe_autoplay_viewer_video`,
/// `resume_pending_viewer_video`, `toggle_viewer_video`) routes through
/// this so every start path agrees. The player opens the file itself, so a
/// local clip plays at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerVideoStart {
    /// Clip local: start playback immediately.
    PlayNow,
    /// Clip not downloaded: park the request, download, resume later.
    ParkDownload,
    /// Not a video item, or no playable file: do nothing.
    Nothing,
}

pub fn decide_viewer_video_start(item: &MediaViewerItem, clip_local: bool) -> ViewerVideoStart {
    if !item.plays_clip() || item.play_file_id.is_none() {
        return ViewerVideoStart::Nothing;
    }
    if clip_local {
        ViewerVideoStart::PlayNow
    } else {
        ViewerVideoStart::ParkDownload
    }
}

/// Telegram Desktop `mediaviewWaitHide` (media_view.style:279): controls
/// hide after this long without mouse movement.
pub const VIEWER_WAIT_HIDE_MS: u64 = 1100;
/// `mediaviewFadeDuration` (media_view.style:282): the cross-fade of a
/// control's hover state in tdesktop.
pub const VIEWER_FADE_MS: u64 = 150;
/// `mediaviewShowDuration` (media_view.style:281): overlay fade-in time, and
/// the time the controls take to show again.
pub const VIEWER_SHOW_MS: u64 = 200;
/// `mediaviewHideDuration` (media_view.style:280): the controls fade out
/// over this long after the idle wait.
pub const VIEWER_HIDE_MS: u64 = 600;

/// How long the viewer controls take to fade to `hidden`: out over
/// [`VIEWER_HIDE_MS`], back in over [`VIEWER_SHOW_MS`]
/// (`OverlayWidget::updateControlsAnimation`).
pub fn controls_fade_ms(hidden: bool) -> u64 {
    if hidden {
        VIEWER_HIDE_MS
    } else {
        VIEWER_SHOW_MS
    }
}

/// Whether the viewer controls should be hidden now: the pointer has been
/// still for `VIEWER_WAIT_HIDE_MS` and is not resting on a control.
pub fn controls_should_hide(idle_ms: u64, over_controls: bool) -> bool {
    !over_controls && idle_ms >= VIEWER_WAIT_HIDE_MS
}

/// Milliseconds until the auto-hide timer must look again (`0` = hide
/// now).
pub fn controls_hide_wait_ms(idle_ms: u64) -> u64 {
    VIEWER_WAIT_HIDE_MS.saturating_sub(idle_ms)
}

/// Whether the viewer offers Delete for a message, and whether "delete
/// for everyone" is possible: `Some(can_revoke)` when allowed, `None`
/// when not. Mirrors the message menu: TDLib's `messageProperties` when
/// they have arrived, else (a local call, milliseconds) any message but a
/// channel post, revocable for your own messages outside Saved Messages.
pub fn viewer_delete_gate(
    actions: Option<MessageActions>,
    is_outgoing: bool,
    is_channel_post: bool,
    saved_messages: bool,
) -> Option<bool> {
    let allowed = actions.map_or(!is_channel_post, |a| a.can_be_deleted());
    let can_revoke =
        !saved_messages && actions.map_or(is_outgoing, |a| a.can_be_deleted_for_all_users);
    allowed.then_some(can_revoke)
}

/// Seconds the viewer seeks per `J` / `L` (tdesktop `kSeekTimeMsLong`,
/// media_view_overlay_widget.cpp:209).
pub const VIEWER_SEEK_LONG_SECS: f64 = 10.0;
/// Seconds per arrow key while a video is full screen (`kSeekTimeMs`,
/// :208).
pub const VIEWER_SEEK_SECS: f64 = 5.0;

/// New playhead after seeking `delta_secs`, kept inside the clip.
pub fn seek_target_secs(position: f64, duration: f64, delta_secs: f64) -> f64 {
    (position + delta_secs).clamp(0.0, duration.max(0.0))
}

/// What a viewer key does. Mirrors the `_streamed` branch of
/// `OverlayWidget::handleKeyPress` (media_view_overlay_widget.cpp
/// :7322-7402); the generic keys (Escape, arrows, `H`/`V`, copy, save,
/// zoom) stay on the app's action bindings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewerKeyAction {
    /// K / Enter on a playing clip.
    TogglePlayback,
    /// Space: a tap toggles pause on release, a hold plays at 2x until the
    /// release (`mediaviewSpeedBoostHoldDelay`, see [`super::BoostState`]).
    HoldSpace,
    /// `.` (forward) / `,` (back) while paused: one frame
    /// (`_frameStepPending`, :7346).
    StepFrame(bool),
    /// `J` / `L` (10 s) and the arrows in full-screen video (5 s).
    SeekBy(f64),
    /// Full-screen video: `0` restarts, `1`..`9` jump to n/10 of the clip.
    SeekToFraction(f64),
    /// Alt/Ctrl(Cmd) + Enter.
    ToggleFullscreen,
    /// Ctrl+0 (Cmd+0 on macOS), on any item: natural size, or back to fit
    /// from it (tdesktop `filterApplicationEvent` calls `zoomReset`). It
    /// is handled here, before the app's bindings, because the same chord
    /// opens Saved Messages when the viewer is closed.
    ResetZoom,
}

/// Modifier keys of a viewer keystroke; `primary` is Cmd on macOS and
/// Ctrl elsewhere.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewerKeyMods {
    pub primary: bool,
    pub alt: bool,
    pub shift: bool,
}

impl ViewerKeyMods {
    fn none(self) -> bool {
        !self.primary && !self.alt && !self.shift
    }
}

/// Map a keystroke (`key` as GPUI names it: "space", "enter", "k", "left",
/// "5") to the viewer action it triggers, for a viewer showing `kind`.
/// Ctrl+0 works on every item; the rest are clip keys.
/// `video_fullscreen` is the viewer's video full-screen mode. `None` when
/// the key is not a playback key (it then reaches the normal bindings).
/// `paused` is whether the clip is paused (frame step works only then).
pub fn viewer_key_action(
    key: &str,
    mods: ViewerKeyMods,
    kind: MediaViewerKind,
    video_fullscreen: bool,
    paused: bool,
) -> Option<ViewerKeyAction> {
    if key == "0" && mods.primary && !mods.alt && !mods.shift {
        return Some(ViewerKeyAction::ResetZoom);
    }
    if !kind.is_playable() {
        return None;
    }
    if (mods.alt || mods.primary) && matches!(key, "enter" | "return") {
        return Some(ViewerKeyAction::ToggleFullscreen);
    }
    if !mods.none() {
        return None;
    }
    match key {
        "space" => Some(ViewerKeyAction::HoldSpace),
        "k" | "enter" | "return" => Some(ViewerKeyAction::TogglePlayback),
        "." if paused => Some(ViewerKeyAction::StepFrame(true)),
        "," if paused => Some(ViewerKeyAction::StepFrame(false)),
        "j" => Some(ViewerKeyAction::SeekBy(-VIEWER_SEEK_LONG_SECS)),
        "l" => Some(ViewerKeyAction::SeekBy(VIEWER_SEEK_LONG_SECS)),
        "left" if video_fullscreen => Some(ViewerKeyAction::SeekBy(-VIEWER_SEEK_SECS)),
        "right" if video_fullscreen => Some(ViewerKeyAction::SeekBy(VIEWER_SEEK_SECS)),
        d if video_fullscreen && d.len() == 1 => d
            .chars()
            .next()
            .and_then(|c| c.to_digit(10))
            .map(|n| ViewerKeyAction::SeekToFraction(f64::from(n) / 10.0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_viewer::test_support::item;

    fn video_item_with_clip() -> MediaViewerItem {
        let mut item = item(MediaViewerKind::Video, 20);
        item.play_file_id = Some(FileId(96));
        item.duration_secs = Some(12);
        item.mime_type = Some("video/mp4".to_string());
        item.start_timestamp = Some(0);
        item
    }

    #[test]
    fn decide_start_local_clip_plays_now() {
        let item = video_item_with_clip();
        assert_eq!(
            decide_viewer_video_start(&item, true),
            ViewerVideoStart::PlayNow
        );
    }

    #[test]
    fn decide_start_undownloaded_clip_parks_for_download() {
        let item = video_item_with_clip();
        assert_eq!(
            decide_viewer_video_start(&item, false),
            ViewerVideoStart::ParkDownload
        );
    }

    #[test]
    fn decide_start_photo_or_missing_clip_does_nothing() {
        let photo = item(MediaViewerKind::Photo, 10);
        assert_eq!(
            decide_viewer_video_start(&photo, true),
            ViewerVideoStart::Nothing
        );
        let no_clip = item(MediaViewerKind::Video, 21);
        assert_eq!(
            decide_viewer_video_start(&no_clip, true),
            ViewerVideoStart::Nothing
        );
    }

    #[test]
    fn controls_hide_only_after_idle_and_not_over_controls() {
        assert!(!controls_should_hide(0, false));
        assert!(!controls_should_hide(VIEWER_WAIT_HIDE_MS - 1, false));
        assert!(controls_should_hide(VIEWER_WAIT_HIDE_MS, false));
        assert!(!controls_should_hide(10_000, true));
        assert_eq!(controls_hide_wait_ms(0), VIEWER_WAIT_HIDE_MS);
        assert_eq!(controls_hide_wait_ms(400), VIEWER_WAIT_HIDE_MS - 400);
        assert_eq!(controls_hide_wait_ms(5_000), 0);
    }

    #[test]
    fn delete_gate_follows_message_properties() {
        // Before properties arrive: not for channel posts; revoke for own.
        assert_eq!(viewer_delete_gate(None, true, false, false), Some(true));
        assert_eq!(viewer_delete_gate(None, false, false, false), Some(false));
        assert_eq!(viewer_delete_gate(None, true, true, false), None);
        assert_eq!(viewer_delete_gate(None, true, false, true), Some(false));
        // TDLib's answer wins.
        let only_self = MessageActions {
            can_be_deleted_only_for_self: true,
            ..Default::default()
        };
        assert_eq!(
            viewer_delete_gate(Some(only_self), true, false, false),
            Some(false)
        );
        let everyone = MessageActions {
            can_be_deleted_for_all_users: true,
            ..Default::default()
        };
        assert_eq!(
            viewer_delete_gate(Some(everyone), false, true, false),
            Some(true)
        );
        assert_eq!(
            viewer_delete_gate(Some(MessageActions::default()), true, false, false),
            None
        );
    }

    fn keys(key: &str, kind: MediaViewerKind, fullscreen: bool) -> Option<ViewerKeyAction> {
        viewer_key_action(key, ViewerKeyMods::default(), kind, fullscreen, false)
    }

    #[test]
    fn playback_keys_follow_tdesktop() {
        use MediaViewerKind::{Animation, Photo, Video};
        // Space arms the hold-to-boost timer (:7332); K and Enter pause /
        // resume at once (:7329, :7416).
        assert_eq!(
            keys("space", Video, false),
            Some(ViewerKeyAction::HoldSpace)
        );
        assert_eq!(
            keys("space", Animation, true),
            Some(ViewerKeyAction::HoldSpace)
        );
        for key in ["k", "enter"] {
            assert_eq!(
                keys(key, Video, false),
                Some(ViewerKeyAction::TogglePlayback)
            );
            assert_eq!(
                keys(key, Animation, false),
                Some(ViewerKeyAction::TogglePlayback)
            );
        }
        // J / L seek 10 s (:7338-7345).
        assert_eq!(
            keys("j", Video, false),
            Some(ViewerKeyAction::SeekBy(-10.0))
        );
        assert_eq!(keys("l", Video, false), Some(ViewerKeyAction::SeekBy(10.0)));
        // Arrows page outside full-screen video, seek 5 s inside (:7394).
        assert_eq!(keys("left", Video, false), None);
        assert_eq!(keys("right", Video, false), None);
        assert_eq!(
            keys("left", Video, true),
            Some(ViewerKeyAction::SeekBy(-5.0))
        );
        assert_eq!(
            keys("right", Video, true),
            Some(ViewerKeyAction::SeekBy(5.0))
        );
        // Full-screen digits jump to n/10 of the clip, 0 restarts (:7387).
        assert_eq!(
            keys("0", Video, true),
            Some(ViewerKeyAction::SeekToFraction(0.0))
        );
        assert_eq!(
            keys("5", Video, true),
            Some(ViewerKeyAction::SeekToFraction(0.5))
        );
        assert_eq!(
            keys("5", Video, false),
            None,
            "zoom keys outside full screen"
        );
        // Photos have no playback keys.
        for key in ["space", "k", "j", "l", "enter"] {
            assert_eq!(keys(key, Photo, false), None);
        }
        // Unrelated keys fall through to the normal bindings.
        assert_eq!(keys("h", Video, false), None);
        assert_eq!(keys("x", Video, true), None);
    }

    #[test]
    fn modified_enter_toggles_fullscreen_and_other_chords_pass() {
        let ctrl = ViewerKeyMods {
            primary: true,
            ..Default::default()
        };
        let alt = ViewerKeyMods {
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            viewer_key_action("enter", ctrl, MediaViewerKind::Video, false, false),
            Some(ViewerKeyAction::ToggleFullscreen)
        );
        assert_eq!(
            viewer_key_action("enter", alt, MediaViewerKind::Video, false, false),
            Some(ViewerKeyAction::ToggleFullscreen)
        );
        assert_eq!(
            viewer_key_action("enter", alt, MediaViewerKind::Photo, false, false),
            None
        );
        // Cmd/Ctrl + K is quick switch, not play/pause.
        assert_eq!(
            viewer_key_action("k", ctrl, MediaViewerKind::Video, false, false),
            None
        );
    }

    #[test]
    fn primary_zero_resets_zoom_on_every_item() {
        use MediaViewerKind::{Photo, Video};
        let primary = ViewerKeyMods {
            primary: true,
            ..Default::default()
        };
        for (kind, fullscreen) in [(Photo, false), (Video, false), (Video, true)] {
            assert_eq!(
                viewer_key_action("0", primary, kind, fullscreen, false),
                Some(ViewerKeyAction::ResetZoom)
            );
        }
        // Plain 0 is the rebindable zoom reset on a photo and a seek in
        // full screen; Ctrl+Shift+0 is not the reset.
        assert_eq!(keys("0", Photo, false), None);
        assert_eq!(
            keys("0", Video, true),
            Some(ViewerKeyAction::SeekToFraction(0.0))
        );
        let shifted = ViewerKeyMods {
            shift: true,
            ..primary
        };
        assert_eq!(viewer_key_action("0", shifted, Photo, false, false), None);
    }

    #[test]
    fn frame_step_keys_work_only_while_paused() {
        use MediaViewerKind::{Photo, Video};
        let paused =
            |key: &str, kind| viewer_key_action(key, ViewerKeyMods::default(), kind, false, true);
        assert_eq!(paused(".", Video), Some(ViewerKeyAction::StepFrame(true)));
        assert_eq!(paused(",", Video), Some(ViewerKeyAction::StepFrame(false)));
        // Full screen too.
        assert_eq!(
            viewer_key_action(".", ViewerKeyMods::default(), Video, true, true),
            Some(ViewerKeyAction::StepFrame(true))
        );
        // Playing: the keys pass through (tdesktop checks `paused()`).
        assert_eq!(keys(".", Video, false), None);
        assert_eq!(keys(",", Video, false), None);
        // With a modifier, or on a photo: not a frame step.
        let shift = ViewerKeyMods {
            shift: true,
            ..Default::default()
        };
        assert_eq!(viewer_key_action(".", shift, Video, false, true), None);
        assert_eq!(paused(".", Photo), None);
        // Space and K keep their meaning while paused.
        assert_eq!(paused("space", Video), Some(ViewerKeyAction::HoldSpace));
        assert_eq!(paused("k", Video), Some(ViewerKeyAction::TogglePlayback));
        assert_eq!(paused("l", Video), Some(ViewerKeyAction::SeekBy(10.0)));
    }

    #[test]
    fn controls_show_fast_and_hide_slowly() {
        assert_eq!(controls_fade_ms(false), 200);
        assert_eq!(controls_fade_ms(true), 600);
    }

    #[test]
    fn seek_target_stays_inside_the_clip() {
        assert_eq!(seek_target_secs(30.0, 60.0, 10.0), 40.0);
        assert_eq!(seek_target_secs(3.0, 60.0, -10.0), 0.0);
        assert_eq!(seek_target_secs(55.0, 60.0, 10.0), 60.0);
        assert_eq!(seek_target_secs(1.0, 0.0, 5.0), 0.0);
    }
}

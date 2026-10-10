//! Pure logic for the media viewer extras: the playback speed dial, the
//! "saved to your Downloads folder" toast, and the mapping of OS media keys
//! onto the viewer's video.
//!
//! Telegram Desktop references: `media/player/media_player_dropdown.cpp`
//! (speed slider and presets, `kSpeedMin`/`kSpeedMax` in
//! `media/media_common.h`), `media_view_overlay_widget.cpp`
//! (`showSaveMsgToast`) and `media/system_media_controls_manager.cpp`.

use crate::media_session::{Command, NowPlaying};
use std::path::{Path, PathBuf};

/// Slowest and fastest playback speed (`kSpeedMin`, `kSpeedMax`).
pub const SPEED_MIN: f64 = 0.5;
pub const SPEED_MAX: f64 = 2.5;

/// Speeds the slider snaps to when released nearby (`kSpeedStickedValues`),
/// each with the distance at which it grabs.
const STICKY_SPEEDS: [f64; 7] = [0.8, 1.0, 1.2, 1.5, 1.7, 2.0, 2.2];
const STICKY_RANGE: f64 = 0.05;

/// The preset list under the slider (`FillSpeedMenu`).
pub const SPEED_PRESETS: [(f64, &str); 6] = [
    (0.5, "Slow"),
    (1.0, "Normal"),
    (1.2, "Medium"),
    (1.5, "Fast"),
    (1.7, "Very fast"),
    (2.0, "Super fast"),
];

/// Whether two speeds are the same setting (`EqualSpeeds`).
pub fn equal_speeds(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

/// Keep a speed inside the supported range.
pub fn clamp_speed(speed: f64) -> f64 {
    if speed.is_finite() {
        speed.clamp(SPEED_MIN, SPEED_MAX)
    } else {
        1.0
    }
}

/// Slider position (0 to 1) of a speed (`SpeedToSliderValue`).
pub fn speed_to_slider(speed: f64) -> f64 {
    (clamp_speed(speed) - SPEED_MIN) / (SPEED_MAX - SPEED_MIN)
}

/// Speed at a slider position, rounded to a tenth (`SliderValueToSpeed`).
pub fn slider_to_speed(value: f64) -> f64 {
    let speed = value.clamp(0.0, 1.0) * (SPEED_MAX - SPEED_MIN) + SPEED_MIN;
    (speed * 10.0).round() / 10.0
}

/// The speed a drag settles on: a sticky preset when close to one.
pub fn snap_speed(speed: f64) -> f64 {
    STICKY_SPEEDS
        .iter()
        .copied()
        .find(|sticky| (speed - sticky).abs() < STICKY_RANGE)
        .unwrap_or_else(|| clamp_speed(speed))
}

/// "1.5×" style label; whole speeds drop the decimal ("2×").
pub fn speed_label(speed: f64) -> String {
    let speed = (speed * 10.0).round() / 10.0;
    if (speed - speed.round()).abs() < 0.01 {
        format!("{}×", speed.round() as i32)
    } else {
        format!("{speed:.1}×")
    }
}

/// The preset name for a speed, when it is one of them.
pub fn preset_name(speed: f64) -> Option<&'static str> {
    SPEED_PRESETS
        .iter()
        .find(|(preset, _)| equal_speeds(*preset, speed))
        .map(|(_, name)| *name)
}

/// What the "saved" toast says: text, a folder link, then text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedToastText {
    pub before: String,
    pub folder: String,
    pub after: String,
}

/// Wording of `lng_mediaview_saved_to` / `lng_mediaview_video_saved_to`.
/// The folder reads "Downloads" for the downloads folder and otherwise the
/// folder's own name, so a custom location is never mislabeled.
pub fn saved_toast_text(dest: &Path, downloads: Option<&Path>, video: bool) -> SavedToastText {
    let parent = dest.parent();
    let folder = if parent.is_some() && parent == downloads {
        "Downloads".to_string()
    } else {
        parent
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("Downloads")
            .to_string()
    };
    SavedToastText {
        before: if video {
            "Video file was saved to your ".into()
        } else {
            "Image was saved to your ".into()
        },
        folder,
        after: " folder".into(),
    }
}

/// The saved file's toast: where it went and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedToast {
    pub dest: PathBuf,
    pub text: SavedToastText,
}

/// How long the toast stays up.
pub const SAVED_TOAST_MS: u64 = 4000;

/// Something the viewer's video should do for an OS transport command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewerMediaAction {
    Toggle,
    Stop,
    SeekTo(f64),
}

/// Map an OS command onto the viewer video. Play on a playing clip and
/// Pause on a paused one are no-ops; track skipping does not apply to a
/// single clip.
pub fn viewer_media_action(
    command: Command,
    playing: bool,
    duration_secs: f64,
) -> Option<ViewerMediaAction> {
    match command {
        Command::Play if !playing => Some(ViewerMediaAction::Toggle),
        Command::Pause if playing => Some(ViewerMediaAction::Toggle),
        Command::Toggle => Some(ViewerMediaAction::Toggle),
        Command::Stop => Some(ViewerMediaAction::Stop),
        Command::SeekTo(secs) if secs.is_finite() => Some(ViewerMediaAction::SeekTo(
            secs.clamp(0.0, duration_secs.max(0.0)),
        )),
        _ => None,
    }
}

/// What the OS widget shows for the viewer's video: the sender in the
/// title and the chat as the subtitle.
pub fn viewer_now_playing(
    sender: &str,
    chat: &str,
    duration_secs: f64,
    position_secs: f64,
    playing: bool,
    rate: f64,
) -> NowPlaying {
    let title = if sender.trim().is_empty() {
        "Video".to_string()
    } else {
        format!("Video from {}", sender.trim())
    };
    NowPlaying {
        title,
        artist: chat.trim().to_string(),
        duration_secs,
        position_secs: position_secs.clamp(0.0, duration_secs.max(0.0)),
        playing,
        rate,
        can_next: false,
        can_previous: false,
    }
}

/// Whose profile the viewer's sender name opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderProfile {
    User(i64),
    Supergroup(i64),
    BasicGroup(i64),
}

/// The profile behind the sender name: the user who sent the media, a
/// group or channel that posted as itself, else the chat the media is in.
pub fn sender_profile(
    sender: Option<&crate::telegram::envelope::MessageSender>,
    sender_chat: Option<&crate::telegram::envelope::ChatKind>,
    chat: Option<&crate::telegram::envelope::ChatKind>,
) -> Option<SenderProfile> {
    use crate::telegram::envelope::{ChatKind, MessageSender};
    let of_chat = |kind: &ChatKind| match kind {
        ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => {
            Some(SenderProfile::User(user_id.0))
        }
        ChatKind::BasicGroup { basic_group_id } => Some(SenderProfile::BasicGroup(*basic_group_id)),
        ChatKind::Supergroup { supergroup_id, .. } => {
            Some(SenderProfile::Supergroup(*supergroup_id))
        }
        ChatKind::Unknown => None,
    };
    match sender {
        Some(MessageSender::User { user_id }) => Some(SenderProfile::User(*user_id)),
        Some(MessageSender::Chat { .. }) => sender_chat.and_then(of_chat),
        None => chat.and_then(of_chat),
    }
}

/// The context menu entry that opens the chat's media list
/// (`lng_mediaview_photos_all`).
pub fn view_all_label(photo: bool) -> &'static str {
    if photo {
        "View all photos"
    } else {
        "View all videos"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_round_trips_whole_tenths() {
        for tenths in 5..=25 {
            let speed = f64::from(tenths) / 10.0;
            assert!(equal_speeds(slider_to_speed(speed_to_slider(speed)), speed));
        }
        assert_eq!(slider_to_speed(0.0), 0.5);
        assert_eq!(slider_to_speed(1.0), 2.5);
        assert_eq!(slider_to_speed(2.0), 2.5);
    }

    #[test]
    fn sticky_speeds_grab_nearby_values() {
        assert_eq!(snap_speed(1.03), 1.0);
        assert_eq!(snap_speed(1.48), 1.5);
        assert_eq!(snap_speed(2.2), 2.2);
        assert_eq!(snap_speed(1.35), 1.35);
        assert_eq!(snap_speed(9.0), SPEED_MAX);
        assert_eq!(snap_speed(0.1), SPEED_MIN);
    }

    #[test]
    fn labels_drop_a_whole_decimal() {
        assert_eq!(speed_label(1.0), "1×");
        assert_eq!(speed_label(2.0), "2×");
        assert_eq!(speed_label(1.5), "1.5×");
        assert_eq!(speed_label(1.2000000001), "1.2×");
        assert_eq!(speed_label(0.5), "0.5×");
    }

    #[test]
    fn presets_are_named_and_in_range() {
        assert_eq!(preset_name(1.0), Some("Normal"));
        assert_eq!(preset_name(1.7), Some("Very fast"));
        assert_eq!(preset_name(1.3), None);
        assert!(
            SPEED_PRESETS
                .iter()
                .all(|(s, _)| (SPEED_MIN..=SPEED_MAX).contains(s))
        );
    }

    #[test]
    fn non_finite_speed_falls_back_to_normal() {
        assert_eq!(clamp_speed(f64::NAN), 1.0);
        assert_eq!(clamp_speed(f64::INFINITY), 1.0);
    }

    #[test]
    fn toast_names_the_downloads_folder_or_the_custom_one() {
        let downloads = Path::new("/home/me/Downloads");
        let text = saved_toast_text(
            Path::new("/home/me/Downloads/cat.jpg"),
            Some(downloads),
            false,
        );
        assert_eq!(text.before, "Image was saved to your ");
        assert_eq!(text.folder, "Downloads");
        assert_eq!(text.after, " folder");
        let text = saved_toast_text(
            Path::new("/home/me/Pictures/clip.mp4"),
            Some(downloads),
            true,
        );
        assert_eq!(text.before, "Video file was saved to your ");
        assert_eq!(text.folder, "Pictures");
    }

    #[test]
    fn media_keys_map_onto_the_viewer_video() {
        use ViewerMediaAction::*;
        assert_eq!(
            viewer_media_action(Command::Play, false, 60.0),
            Some(Toggle)
        );
        assert_eq!(viewer_media_action(Command::Play, true, 60.0), None);
        assert_eq!(
            viewer_media_action(Command::Pause, true, 60.0),
            Some(Toggle)
        );
        assert_eq!(viewer_media_action(Command::Pause, false, 60.0), None);
        assert_eq!(
            viewer_media_action(Command::Toggle, true, 60.0),
            Some(Toggle)
        );
        assert_eq!(viewer_media_action(Command::Stop, true, 60.0), Some(Stop));
        assert_eq!(viewer_media_action(Command::Next, true, 60.0), None);
        assert_eq!(
            viewer_media_action(Command::SeekTo(90.0), true, 60.0),
            Some(SeekTo(60.0))
        );
        assert_eq!(
            viewer_media_action(Command::SeekTo(f64::NAN), true, 60.0),
            None
        );
    }

    #[test]
    fn now_playing_names_the_sender_and_chat() {
        let info = viewer_now_playing("Ada", "Family", 30.0, 45.0, true, 1.5);
        assert_eq!(info.title, "Video from Ada");
        assert_eq!(info.artist, "Family");
        assert_eq!(info.position_secs, 30.0);
        assert!(!info.can_next && !info.can_previous);
        assert_eq!(
            viewer_now_playing(" ", "", 1.0, 0.0, false, 1.0).title,
            "Video"
        );
    }

    #[test]
    fn sender_name_opens_the_right_profile() {
        use crate::ids::UserId;
        use crate::telegram::envelope::{ChatKind, MessageSender};
        let user = MessageSender::User { user_id: 7 };
        let group = ChatKind::Supergroup {
            supergroup_id: 3,
            is_channel: false,
        };
        assert_eq!(
            sender_profile(Some(&user), None, Some(&group)),
            Some(SenderProfile::User(7))
        );
        let chat_sender = MessageSender::Chat { chat_id: -100 };
        assert_eq!(
            sender_profile(Some(&chat_sender), Some(&group), None),
            Some(SenderProfile::Supergroup(3))
        );
        assert_eq!(
            sender_profile(None, None, Some(&ChatKind::Private { user_id: UserId(9) })),
            Some(SenderProfile::User(9))
        );
        assert_eq!(sender_profile(None, None, Some(&ChatKind::Unknown)), None);
    }

    #[test]
    fn view_all_label_follows_the_media_kind() {
        assert_eq!(view_all_label(true), "View all photos");
        assert_eq!(view_all_label(false), "View all videos");
    }
}

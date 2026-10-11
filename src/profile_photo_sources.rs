//! Profile photo sources beyond a still file (Telegram Desktop
//! `UserpicButton`): the emoji avatar builder, a video avatar with the
//! main frame chosen on a scrubber, and a camera snapshot.
//!
//! This module holds the parts with no UI: the `setProfilePhoto` request
//! bodies for `inputChatPhotoAnimation` and `inputChatPhotoSticker`, the
//! builder's gradient presets, the video limits and the ffmpeg command
//! lines for the avatar clip, the preview frames and the snapshot.

use crate::ids::RequestId;
use serde_json::{Value, json};

/// Telegram Desktop's emoji builder palette (`PaletteGradients`), as listed
/// in its source. Four colors each, `0xRRGGBB`.
const PALETTE: [[u32; 4]; 7] = [
    [0x20E2CD, 0x0EE1F1, 0x4D8DFF, 0x2BBFFF],
    [0x45F7B7, 0x1FF1D9, 0x5EB6FB, 0x1FCEEB],
    [0xC1E526, 0x80DF2B, 0x09D260, 0x5EDC40],
    [0xFFD412, 0xFFA743, 0xF5694E, 0xF5772C],
    [0xF6A730, 0xFF7742, 0xF64884, 0xEF5B41],
    [0xFFB23A, 0xFE7E62, 0xF94BA0, 0xFB5C80],
    [0xFF72A9, 0xE269FF, 0x837CFF, 0xB063FF],
];

/// How many gradient presets the builder offers.
pub const PRESET_COUNT: usize = PALETTE.len();

/// The colors of preset `index` (wrapping), in the order Telegram Desktop
/// uses them: its list rotated by 180 degrees (first and third color
/// swapped, second and fourth swapped).
pub fn gradient_preset(index: usize) -> [u32; 4] {
    let [a, b, c, d] = PALETTE[index % PALETTE.len()];
    [c, d, a, b]
}

/// `backgroundFill` for a chosen color list. One color is solid, two are a
/// vertical gradient, more are a freeform gradient. A chat photo sticker
/// ignores the rotation angle, so it is always 0.
pub fn background_fill(colors: &[u32]) -> Value {
    let rgb = |color: u32| i64::from(color & 0x00FF_FFFF);
    match colors {
        [] => json!({ "@type": "backgroundFillSolid", "color": 0 }),
        [one] => json!({ "@type": "backgroundFillSolid", "color": rgb(*one) }),
        [top, bottom] => json!({
            "@type": "backgroundFillGradient",
            "top_color": rgb(*top),
            "bottom_color": rgb(*bottom),
            "rotation_angle": 0,
        }),
        many => json!({
            "@type": "backgroundFillFreeformGradient",
            "colors": many.iter().map(|c| rgb(*c)).collect::<Vec<_>>(),
        }),
    }
}

/// What sits on the gradient (`ChatPhotoStickerType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhotoSticker {
    CustomEmoji {
        custom_emoji_id: i64,
    },
    RegularOrMask {
        sticker_set_id: i64,
        sticker_id: i64,
    },
}

/// `chatPhotoSticker type background_fill`.
pub fn chat_photo_sticker(sticker: PhotoSticker, colors: &[u32]) -> Value {
    let kind = match sticker {
        PhotoSticker::CustomEmoji { custom_emoji_id } => json!({
            "@type": "chatPhotoStickerTypeCustomEmoji",
            "custom_emoji_id": custom_emoji_id,
        }),
        PhotoSticker::RegularOrMask {
            sticker_set_id,
            sticker_id,
        } => json!({
            "@type": "chatPhotoStickerTypeRegularOrMask",
            "sticker_set_id": sticker_set_id,
            "sticker_id": sticker_id,
        }),
    };
    json!({
        "@type": "chatPhotoSticker",
        "type": kind,
        "background_fill": background_fill(colors),
    })
}

/// `setProfilePhoto` with `inputChatPhotoSticker` (TDLib 1.8.68,
/// `schema/td_api.tl:1051`): the emoji or sticker avatar.
pub fn set_profile_photo_sticker(
    extra: RequestId,
    sticker: PhotoSticker,
    colors: &[u32],
    is_public: bool,
) -> String {
    json!({
        "@type": "setProfilePhoto",
        "@extra": extra.as_extra(),
        "photo": {
            "@type": "inputChatPhotoSticker",
            "sticker": chat_photo_sticker(sticker, colors),
        },
        "is_public": is_public,
    })
    .to_string()
}

/// `setProfilePhoto` with `inputChatPhotoAnimation` (schema line 1048):
/// the video avatar. `main_frame_timestamp` is in seconds from the start
/// of `path`, the clip as it was encoded.
pub fn set_profile_photo_animation(
    extra: RequestId,
    path: &str,
    main_frame_timestamp: f64,
    is_public: bool,
) -> String {
    json!({
        "@type": "setProfilePhoto",
        "@extra": extra.as_extra(),
        "photo": {
            "@type": "inputChatPhotoAnimation",
            "animation": { "@type": "inputFileLocal", "path": path },
            "main_frame_timestamp": main_frame_timestamp,
        },
        "is_public": is_public,
    })
    .to_string()
}

/// `getDefaultProfilePhotoCustomEmojiStickers = Stickers;` (schema line
/// 15151): the custom emoji the builder offers.
pub fn get_default_profile_photo_custom_emoji_stickers(extra: RequestId) -> String {
    json!({
        "@type": "getDefaultProfilePhotoCustomEmojiStickers",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// The camera input of `media_tools::capture_input` without the
/// microphone. `args` is the full list (video and audio inputs).
pub fn video_only_input(args: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        kept.push(arg.clone());
        if arg == "-i" {
            if let Some(device) = iter.next() {
                kept.push(camera_device(device));
            }
            break;
        }
    }
    kept
}

/// The camera part of an input name: AVFoundation's `video:audio` pair
/// keeps the video and drops the sound, DirectShow's `video=..:audio=..`
/// the same.
fn camera_device(device: &str) -> String {
    if let Some((video, _)) = device.split_once(":audio=") {
        return video.to_string();
    }
    if let Some((video, _)) = device.split_once(':') {
        return format!("{video}:none");
    }
    device.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        PhotoSticker, background_fill, gradient_preset, set_profile_photo_animation,
        set_profile_photo_sticker, video_only_input,
    };
    use crate::ids::RequestId;
    use serde_json::{Value, json};

    fn parse(text: String) -> Value {
        serde_json::from_str(&text).unwrap()
    }

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn presets_are_the_desktop_palette_rotated_by_half() {
        assert_eq!(super::PRESET_COUNT, 7);
        assert_eq!(gradient_preset(0), [0x4D8DFF, 0x2BBFFF, 0x20E2CD, 0x0EE1F1]);
        assert_eq!(gradient_preset(6), [0x837CFF, 0xB063FF, 0xFF72A9, 0xE269FF]);
        assert_eq!(gradient_preset(7), gradient_preset(0));
    }

    #[test]
    fn background_fill_picks_the_fill_by_color_count() {
        assert_eq!(
            background_fill(&[0xFF0000]),
            json!({ "@type": "backgroundFillSolid", "color": 0xFF0000 })
        );
        assert_eq!(
            background_fill(&[0x112233, 0x445566]),
            json!({
                "@type": "backgroundFillGradient",
                "top_color": 0x112233,
                "bottom_color": 0x445566,
                "rotation_angle": 0,
            })
        );
        assert_eq!(
            background_fill(&gradient_preset(3)),
            json!({
                "@type": "backgroundFillFreeformGradient",
                "colors": [0xF5694E, 0xF5772C, 0xFFD412, 0xFFA743],
            })
        );
        // Alpha bits never leak into the RGB24 value.
        assert_eq!(background_fill(&[0xFF12_3456])["color"], 0x12_3456);
    }

    #[test]
    fn emoji_avatar_request_uses_a_custom_emoji_sticker() {
        let v = parse(set_profile_photo_sticker(
            RequestId(5),
            PhotoSticker::CustomEmoji {
                custom_emoji_id: 5_368_324_170_671_202_286,
            },
            &gradient_preset(0),
            false,
        ));
        assert_eq!(v["@type"], "setProfilePhoto");
        assert_eq!(v["@extra"], "5");
        assert_eq!(v["is_public"], false);
        assert_eq!(v["photo"]["@type"], "inputChatPhotoSticker");
        let sticker = &v["photo"]["sticker"];
        assert_eq!(sticker["@type"], "chatPhotoSticker");
        assert_eq!(sticker["type"]["@type"], "chatPhotoStickerTypeCustomEmoji");
        assert_eq!(
            sticker["type"]["custom_emoji_id"],
            5_368_324_170_671_202_286_i64
        );
        assert_eq!(
            sticker["background_fill"]["@type"],
            "backgroundFillFreeformGradient"
        );
    }

    #[test]
    fn sticker_avatar_request_names_set_and_sticker() {
        let v = parse(set_profile_photo_sticker(
            RequestId(6),
            PhotoSticker::RegularOrMask {
                sticker_set_id: 77,
                sticker_id: 88,
            },
            &[0x00FF00],
            true,
        ));
        assert_eq!(v["is_public"], true);
        let kind = &v["photo"]["sticker"]["type"];
        assert_eq!(kind["@type"], "chatPhotoStickerTypeRegularOrMask");
        assert_eq!(kind["sticker_set_id"], 77);
        assert_eq!(kind["sticker_id"], 88);
        assert_eq!(
            v["photo"]["sticker"]["background_fill"]["@type"],
            "backgroundFillSolid"
        );
    }

    #[test]
    fn video_avatar_request_carries_the_main_frame() {
        let v = parse(set_profile_photo_animation(
            RequestId(7),
            "/tmp/avatar.mp4",
            2.5,
            false,
        ));
        assert_eq!(v["photo"]["@type"], "inputChatPhotoAnimation");
        assert_eq!(v["photo"]["animation"]["@type"], "inputFileLocal");
        assert_eq!(v["photo"]["animation"]["path"], "/tmp/avatar.mp4");
        assert_eq!(v["photo"]["main_frame_timestamp"], 2.5);
    }

    #[test]
    fn camera_input_drops_the_microphone() {
        let mac = list(&[
            "-f",
            "avfoundation",
            "-framerate",
            "30",
            "-pixel_format",
            "nv12",
            "-i",
            "default:default",
        ]);
        let kept = video_only_input(&mac);
        assert_eq!(kept.last().unwrap(), "default:none");
        assert_eq!(kept.len(), mac.len());

        let linux = list(&[
            "-f",
            "v4l2",
            "-framerate",
            "15",
            "-i",
            "/dev/video0",
            "-f",
            "pulse",
            "-i",
            "default",
        ]);
        assert_eq!(
            video_only_input(&linux),
            list(&["-f", "v4l2", "-framerate", "15", "-i", "/dev/video0"])
        );

        let windows = list(&["-f", "dshow", "-i", "video=Cam:audio=Mic"]);
        assert_eq!(
            video_only_input(&windows),
            list(&["-f", "dshow", "-i", "video=Cam"])
        );
    }
}

//! The viewer's video quality picker (tdesktop `playbackControlsQualities`
//! and `Document::chooseQuality`): which renditions a video offers, which
//! one a saved preference picks, and the labels.

use super::{MediaViewer, MediaViewerItem};
use crate::ids::FileId;
use crate::telegram::envelope::VideoAlternative;

/// One playable rendition of a video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoQualityOption {
    /// Short side in pixels: the "720" of "720p".
    pub height: u32,
    pub file_id: FileId,
    /// The clip as the sender uploaded it.
    pub original: bool,
}

impl VideoQualityOption {
    /// "720p", with "(Original)" appended for the uploaded file.
    pub fn label(&self) -> String {
        if self.original {
            format!("{}p (Original)", self.height)
        } else {
            format!("{}p", self.height)
        }
    }
}

/// How well the app plays a codec; lower is preferred when two renditions
/// have the same height.
fn codec_rank(codec: &str) -> u8 {
    match codec.to_ascii_lowercase().as_str() {
        "h264" | "avc" | "avc1" => 0,
        "h265" | "hevc" => 1,
        "vp9" => 2,
        "av1" => 3,
        _ => 4,
    }
}

fn short_side(width: i32, height: i32) -> u32 {
    width.min(height).max(0) as u32
}

/// The renditions of a video, tallest first: the original plus each
/// alternative, one per height (the original wins a tie, then the codec
/// the app plays best). Fewer than two renditions means no picker.
pub fn build_quality_options(
    original_file: FileId,
    width: i32,
    height: i32,
    alternatives: &[VideoAlternative],
) -> Vec<VideoQualityOption> {
    let mut out: Vec<VideoQualityOption> = Vec::new();
    let original_height = short_side(width, height);
    if original_file.0 != 0 && original_height > 0 {
        out.push(VideoQualityOption {
            height: original_height,
            file_id: original_file,
            original: true,
        });
    }
    let mut extra: Vec<&VideoAlternative> = alternatives
        .iter()
        .filter(|alt| alt.file_id.0 != 0 && short_side(alt.width, alt.height) > 0)
        .collect();
    extra.sort_by_key(|alt| codec_rank(&alt.codec));
    for alt in extra {
        let alt_height = short_side(alt.width, alt.height);
        if out.iter().any(|option| option.height == alt_height) {
            continue;
        }
        out.push(VideoQualityOption {
            height: alt_height,
            file_id: alt.file_id,
            original: false,
        });
    }
    out.sort_by_key(|option| std::cmp::Reverse(option.height));
    if out.len() < 2 {
        out.clear();
    }
    out
}

/// The rendition a saved preference picks. `preferred_height` 0 is Auto:
/// the original. A number picks that height, else the tallest below it,
/// else the shortest above it.
pub fn pick_quality(
    options: &[VideoQualityOption],
    preferred_height: u32,
) -> Option<&VideoQualityOption> {
    if options.is_empty() {
        return None;
    }
    if preferred_height == 0 {
        return options
            .iter()
            .find(|option| option.original)
            .or_else(|| options.first());
    }
    options
        .iter()
        .filter(|option| option.height <= preferred_height)
        .max_by_key(|option| option.height)
        .or_else(|| options.iter().min_by_key(|option| option.height))
}

/// The rendition the item is playing now.
pub fn current_quality(item: &MediaViewerItem) -> Option<&VideoQualityOption> {
    let play = item.play_file_id?;
    item.quality_options
        .iter()
        .find(|option| option.file_id == play)
}

impl MediaViewer {
    /// Point the current video at the rendition `preferred_height` picks.
    /// Returns whether the file to play changed.
    pub fn apply_video_quality(&mut self, preferred_height: u32) -> bool {
        let Some(item) = self.items.get_mut(self.index) else {
            return false;
        };
        let Some(pick) = pick_quality(&item.quality_options, preferred_height) else {
            return false;
        };
        let file_id = pick.file_id;
        if item.play_file_id == Some(file_id) {
            return false;
        }
        item.play_file_id = Some(file_id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_viewer::{MediaViewer, MediaViewerKind};

    fn alt(width: i32, height: i32, codec: &str, id: i32) -> VideoAlternative {
        VideoAlternative {
            width,
            height,
            codec: codec.into(),
            file_id: FileId(id),
        }
    }

    fn sample() -> Vec<VideoQualityOption> {
        build_quality_options(
            FileId(10),
            1920,
            1080,
            &[
                alt(1280, 720, "av1", 21),
                alt(1280, 720, "h264", 22),
                alt(640, 360, "h264", 23),
                alt(1920, 1080, "h265", 24),
            ],
        )
    }

    #[test]
    fn lists_tallest_first_one_per_height_original_wins() {
        let options = sample();
        let heights: Vec<u32> = options.iter().map(|o| o.height).collect();
        assert_eq!(heights, vec![1080, 720, 360]);
        assert_eq!(options[0].file_id, FileId(10));
        assert!(options[0].original);
        // h264 beats av1 at the same height.
        assert_eq!(options[1].file_id, FileId(22));
    }

    #[test]
    fn portrait_video_uses_the_short_side() {
        let options = build_quality_options(FileId(1), 1080, 1920, &[alt(360, 640, "h264", 2)]);
        let heights: Vec<u32> = options.iter().map(|o| o.height).collect();
        assert_eq!(heights, vec![1080, 360]);
    }

    #[test]
    fn a_single_rendition_offers_no_picker() {
        assert!(build_quality_options(FileId(1), 640, 360, &[]).is_empty());
        assert!(build_quality_options(FileId(1), 640, 360, &[alt(640, 360, "h264", 2)]).is_empty());
    }

    #[test]
    fn auto_picks_the_original_and_numbers_pick_nearest_not_above() {
        let options = sample();
        assert_eq!(pick_quality(&options, 0).unwrap().file_id, FileId(10));
        assert_eq!(pick_quality(&options, 720).unwrap().file_id, FileId(22));
        assert_eq!(pick_quality(&options, 480).unwrap().file_id, FileId(23));
        // Below the smallest: the smallest there is.
        assert_eq!(pick_quality(&options, 144).unwrap().file_id, FileId(23));
        assert!(pick_quality(&[], 720).is_none());
    }

    #[test]
    fn labels_mark_the_original() {
        let options = sample();
        assert_eq!(options[0].label(), "1080p (Original)");
        assert_eq!(options[1].label(), "720p");
    }

    #[test]
    fn viewer_switches_the_play_file() {
        let mut item = crate::media_viewer::test_support::item(MediaViewerKind::Video, 5);
        item.play_file_id = Some(FileId(10));
        item.quality_options = sample();
        let mut viewer = MediaViewer::open(vec![item], 0);
        assert!(viewer.apply_video_quality(360));
        assert_eq!(viewer.current().unwrap().play_file_id, Some(FileId(23)));
        assert_eq!(
            current_quality(viewer.current().unwrap()).unwrap().height,
            360
        );
        assert!(!viewer.apply_video_quality(360));
        assert!(viewer.apply_video_quality(0));
        assert_eq!(viewer.current().unwrap().play_file_id, Some(FileId(10)));
    }
}

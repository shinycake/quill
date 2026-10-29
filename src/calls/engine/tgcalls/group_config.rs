//! The JSON a tgcalls group call takes, built the way Telegram Desktop
//! builds it (calls_group_call.cpp): how a join answer connects
//! (`ParseJoinResponse`), and which participant videos to receive at what
//! quality (`GroupCall::updateRequestedVideoChannels`).
use super::super::{GroupCallMode, GroupVideoSource, StreamQuality};
use quill_calls_sys::{QC_GROUP_QUALITY_FULL, QC_GROUP_QUALITY_MEDIUM, QC_GROUP_QUALITY_THUMBNAIL};
use serde_json::{Value, json};

/// tdesktop's budget: one Full video costs as much as four Medium ones,
/// and at most sixteen Medium ones are asked for.
const FULL_AS_MEDIUMS: usize = 4;
const MAX_MEDIUMS: usize = 16;

/// The endpoint of an RTMP stream's video (tdesktop's
/// `Data::RtmpEndpointId`, the one tgcalls delivers unified parts to).
pub(super) const RTMP_ENDPOINT: &str = "unified";

/// The least quality to ask of an RTMP stream's video: Full while its
/// tile is Full, as tdesktop asks, else whatever tgcalls can get.
pub(super) fn unified_min_quality(tile: Option<StreamQuality>) -> i32 {
    match tile {
        Some(StreamQuality::Full) => QC_GROUP_QUALITY_FULL,
        Some(StreamQuality::Medium | StreamQuality::Thumbnail) | None => QC_GROUP_QUALITY_THUMBNAIL,
    }
}

/// How the `joinVideoChat` answer connects: `"rtmp": true` is an RTMP
/// broadcast, `"stream": true` an MTProto one, anything else (a malformed
/// answer included) RTC. An RTMP answer is a broadcast with or without
/// `"stream"`, as `GroupCallMode::from_join_answer` reads it on the
/// connect side.
pub(super) fn join_mode(answer: &str) -> GroupCallMode {
    let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(answer) else {
        return GroupCallMode::Rtc;
    };
    let flag = |key: &str| fields.get(key).and_then(Value::as_bool) == Some(true);
    if flag("rtmp") {
        GroupCallMode::Rtmp
    } else if flag("stream") {
        GroupCallMode::Stream
    } else {
        GroupCallMode::Rtc
    }
}

/// One video to receive, with tgcalls' quality bounds (`QC_GROUP_QUALITY_*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestedVideo<'a> {
    pub(super) source: &'a GroupVideoSource,
    pub(super) min_quality: i32,
    pub(super) max_quality: i32,
}

/// The qualities to ask for. Quill does not tell the engine how large
/// each tile is, so every video is asked for as tdesktop asks for a
/// tile in its grid (a camera up to Medium, a screen up to Full), then
/// cut down the way tdesktop cuts its budget: screens fall back to
/// Thumbnail when they alone cost more than it, cameras when they do.
pub(super) fn requested_videos(sources: &[GroupVideoSource]) -> Vec<RequestedVideo<'_>> {
    let screens = sources.iter().filter(|source| source.screen).count();
    let cameras = sources.len() - screens;
    let screens_fit = screens * FULL_AS_MEDIUMS <= MAX_MEDIUMS;
    let cameras_fit = cameras <= MAX_MEDIUMS;
    sources
        .iter()
        .map(|source| {
            let max_quality = match (source.screen, screens_fit, cameras_fit) {
                (true, true, _) => QC_GROUP_QUALITY_FULL,
                (false, _, true) => QC_GROUP_QUALITY_MEDIUM,
                _ => QC_GROUP_QUALITY_THUMBNAIL,
            };
            RequestedVideo {
                source,
                min_quality: QC_GROUP_QUALITY_THUMBNAIL,
                max_quality,
            }
        })
        .collect()
}

/// `qc_group_set_requested_video`'s JSON for `sources`, and for an RTMP
/// stream's unified video at least at `unified` quality (up to Full, as
/// tdesktop asks for it). Each video names its participant's audio ssrc,
/// which tgcalls syncs it to, as tdesktop's `.audioSsrc =
/// participant->ssrc`; the unified video has none.
pub(super) fn requested_video_json(sources: &[GroupVideoSource], unified: Option<i32>) -> String {
    let mut channels: Vec<Value> = requested_videos(sources)
        .iter()
        .map(|video| {
            json!({
                "audio_ssrc": video.source.audio_ssrc,
                "user_id": video.source.user_id,
                "endpoint": video.source.endpoint,
                "ssrc_groups": video.source.ssrc_groups.iter().map(|group| json!({
                    "semantics": group.semantics,
                    "ssrcs": group.ssrcs,
                })).collect::<Vec<_>>(),
                "min_quality": video.min_quality,
                "max_quality": video.max_quality,
            })
        })
        .collect();
    if let Some(min_quality) = unified {
        channels.push(json!({
            "endpoint": RTMP_ENDPOINT,
            "min_quality": min_quality,
            "max_quality": QC_GROUP_QUALITY_FULL,
        }));
    }
    Value::Array(channels).to_string()
}

#[cfg(test)]
mod tests {
    use super::super::super::GroupVideoSourceGroup;
    use super::*;

    fn source(index: usize, screen: bool) -> GroupVideoSource {
        GroupVideoSource {
            user_id: 100 + index as i64,
            endpoint: format!("endpoint-{index}"),
            ssrc_groups: vec![GroupVideoSourceGroup {
                semantics: "SIM".to_owned(),
                ssrcs: vec![1, 2, 3],
            }],
            screen,
            audio_ssrc: 7000 + u32::try_from(index).unwrap(),
        }
    }

    fn maxima(sources: &[GroupVideoSource]) -> Vec<i32> {
        requested_videos(sources)
            .iter()
            .map(|video| video.max_quality)
            .collect()
    }

    #[test]
    fn answers_connect_as_tdesktop_reads_them() {
        assert_eq!(
            join_mode(r#"{"stream":true,"rtmp":true}"#),
            GroupCallMode::Rtmp
        );
        assert_eq!(join_mode(r#"{"stream":true}"#), GroupCallMode::Stream);
        assert_eq!(join_mode(r#"{"rtmp":true}"#), GroupCallMode::Rtmp);
        assert_eq!(join_mode(r#"{"stream":false}"#), GroupCallMode::Rtc);
        assert_eq!(join_mode(r#"{"transport":{}}"#), GroupCallMode::Rtc);
        assert_eq!(join_mode("not json"), GroupCallMode::Rtc);
    }

    #[test]
    fn a_small_call_gets_medium_cameras_and_full_screens() {
        let sources = [source(0, false), source(1, true)];
        assert_eq!(
            maxima(&sources),
            vec![QC_GROUP_QUALITY_MEDIUM, QC_GROUP_QUALITY_FULL]
        );
        assert!(
            requested_videos(&sources)
                .iter()
                .all(|video| video.min_quality == QC_GROUP_QUALITY_THUMBNAIL)
        );
    }

    #[test]
    fn screens_keep_full_while_they_fit_whatever_the_cameras_cost() {
        // As in tdesktop: 2 screens (8 mediums) + 9 cameras is over the
        // budget, but neither kind alone is, so nothing is cut.
        let mut sources: Vec<_> = (0..9).map(|index| source(index, false)).collect();
        sources.extend((9..11).map(|index| source(index, true)));
        let maxima = maxima(&sources);
        assert!(maxima[..9].iter().all(|q| *q == QC_GROUP_QUALITY_MEDIUM));
        assert!(maxima[9..].iter().all(|q| *q == QC_GROUP_QUALITY_FULL));
    }

    #[test]
    fn too_many_cameras_drop_to_thumbnails() {
        let sources: Vec<_> = (0..17).map(|index| source(index, false)).collect();
        assert!(
            maxima(&sources)
                .iter()
                .all(|q| *q == QC_GROUP_QUALITY_THUMBNAIL)
        );
    }

    #[test]
    fn too_many_screens_drop_to_thumbnails_and_cameras_keep_medium() {
        let mut sources: Vec<_> = (0..5).map(|index| source(index, true)).collect();
        sources.push(source(5, false));
        let maxima = maxima(&sources);
        assert!(maxima[..5].iter().all(|q| *q == QC_GROUP_QUALITY_THUMBNAIL));
        assert_eq!(maxima[5], QC_GROUP_QUALITY_MEDIUM);
    }

    #[test]
    fn the_json_carries_each_video() {
        let json: Value =
            serde_json::from_str(&requested_video_json(&[source(0, true)], None)).unwrap();
        let channel = &json[0];
        assert_eq!(channel["endpoint"], "endpoint-0");
        assert_eq!(channel["user_id"], 100);
        assert_eq!(channel["audio_ssrc"], 7000);
        assert_eq!(channel["ssrc_groups"][0]["semantics"], "SIM");
        assert_eq!(channel["ssrc_groups"][0]["ssrcs"], json!([1, 2, 3]));
        assert_eq!(channel["max_quality"], QC_GROUP_QUALITY_FULL);
        assert_eq!(requested_video_json(&[], None), "[]");
    }

    #[test]
    fn an_rtmp_stream_asks_for_its_unified_video() {
        let json: Value =
            serde_json::from_str(&requested_video_json(&[], Some(QC_GROUP_QUALITY_FULL))).unwrap();
        assert_eq!(
            json,
            json!([{
                "endpoint": "unified",
                "min_quality": QC_GROUP_QUALITY_FULL,
                "max_quality": QC_GROUP_QUALITY_FULL,
            }])
        );
        assert_eq!(
            unified_min_quality(Some(StreamQuality::Full)),
            QC_GROUP_QUALITY_FULL
        );
        for tile in [
            None,
            Some(StreamQuality::Medium),
            Some(StreamQuality::Thumbnail),
        ] {
            assert_eq!(unified_min_quality(tile), QC_GROUP_QUALITY_THUMBNAIL);
        }
    }
}

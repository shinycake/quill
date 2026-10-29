//! The JSON a tgcalls group call takes, built the way Telegram Desktop
//! builds it (calls_group_call.cpp): how a join answer connects
//! (`ParseJoinResponse`), and which participant videos to receive at what
//! quality (`GroupCall::updateRequestedVideoChannels`).
use super::super::{GroupCallMode, GroupVideoSource};
use quill_calls_sys::{QC_GROUP_QUALITY_FULL, QC_GROUP_QUALITY_MEDIUM, QC_GROUP_QUALITY_THUMBNAIL};
use serde_json::{Value, json};

/// tdesktop's budget: one Full video costs as much as four Medium ones,
/// and at most sixteen Medium ones are asked for.
const FULL_AS_MEDIUMS: usize = 4;
const MAX_MEDIUMS: usize = 16;

/// How the `joinVideoChat` answer connects, as tdesktop reads it: a
/// `"stream": true` answer is a broadcast (an RTMP one when `"rtmp"` is
/// true too), anything else, a malformed answer included, is RTC.
pub(super) fn join_mode(answer: &str) -> GroupCallMode {
    let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(answer) else {
        return GroupCallMode::Rtc;
    };
    let flag = |key: &str| fields.get(key).and_then(Value::as_bool) == Some(true);
    match (flag("stream"), flag("rtmp")) {
        (true, true) => GroupCallMode::Rtmp,
        (true, false) => GroupCallMode::Stream,
        (false, _) => GroupCallMode::Rtc,
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

/// `qc_group_set_requested_video`'s JSON for `sources`. tgcalls does not
/// use a video's audio ssrc, so 0 stands in for it.
pub(super) fn requested_video_json(sources: &[GroupVideoSource]) -> String {
    let channels: Vec<Value> = requested_videos(sources)
        .iter()
        .map(|video| {
            json!({
                "audio_ssrc": 0,
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
        assert_eq!(
            join_mode(r#"{"stream":false,"rtmp":true}"#),
            GroupCallMode::Rtc
        );
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
        let json: Value = serde_json::from_str(&requested_video_json(&[source(0, true)])).unwrap();
        let channel = &json[0];
        assert_eq!(channel["endpoint"], "endpoint-0");
        assert_eq!(channel["user_id"], 100);
        assert_eq!(channel["audio_ssrc"], 0);
        assert_eq!(channel["ssrc_groups"][0]["semantics"], "SIM");
        assert_eq!(channel["ssrc_groups"][0]["ssrcs"], json!([1, 2, 3]));
        assert_eq!(channel["max_quality"], QC_GROUP_QUALITY_FULL);
        assert_eq!(requested_video_json(&[]), "[]");
    }
}

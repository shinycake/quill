use crate::ids::RequestId;
use serde_json::{Value, json};

/// Phase C1: the `callProtocol` Quill advertises for signaling-only
/// calls (TDLib 1.8.67, `schema/td_api.tl:7008`):
/// `callProtocol udp_p2p:Bool udp_reflector:Bool min_layer:int32
/// max_layer:int32 library_versions:vector<string> = CallProtocol;`
/// The schema pins `min_layer = 65` / `max_layer = 92`. Quill claims
/// **no media capability** (`udp_p2p: false`, `udp_reflector: false`,
/// no tgcalls `library_versions`) — honest, because this slice has no
/// VoIP transport (TDLib does not move audio/video; official clients
/// use libtgvoip, the C2 spike). The remote side will see us as
/// "connecting" until it gives up; the UI says so explicitly.
pub fn call_protocol() -> Value {
    json!({
        "@type": "callProtocol",
        "udp_p2p": false,
        "udp_reflector": false,
        "min_layer": 65,
        "max_layer": 92,
        "library_versions": []
    })
}

/// Phase C1: `createCall` (TDLib 1.8.67, `schema/td_api.tl:14212`):
/// `createCall user_id:int53 protocol:callProtocol is_video:Bool =
/// CallId;` "Creates a new call". Phase C1b: `is_video: true` is
/// allowed — it starts video-call *signaling*; media transport is
/// still Phase C2, so the call carries no audio or video.
pub fn create_call(extra: RequestId, user_id: i64, is_video: bool) -> String {
    create_call_with_protocol(extra, user_id, is_video, &call_protocol())
}

/// Phase C2b: `createCall` with the protocol reported by the loaded engine.
pub fn create_call_with_protocol(
    extra: RequestId,
    user_id: i64,
    is_video: bool,
    protocol: &Value,
) -> String {
    json!({
        "@type": "createCall",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "protocol": protocol,
        "is_video": is_video,
    })
    .to_string()
}

/// Phase C1: `acceptCall` (TDLib 1.8.67, `schema/td_api.tl:14215`):
/// `acceptCall call_id:int32 protocol:callProtocol = Ok;`
/// "Accepts an incoming call".
pub fn accept_call(extra: RequestId, call_id: i32) -> String {
    accept_call_with_protocol(extra, call_id, &call_protocol())
}

/// Phase C2b: `acceptCall` with the protocol reported by the loaded engine.
pub fn accept_call_with_protocol(extra: RequestId, call_id: i32, protocol: &Value) -> String {
    json!({
        "@type": "acceptCall",
        "@extra": extra.as_extra(),
        "call_id": call_id,
        "protocol": protocol,
    })
    .to_string()
}

/// Phase C2b: forward engine-emitted signaling through TDLib
/// (`sendCallSignalingData`, `schema/td_api.tl:14218`). TDLib JSON `bytes`
/// fields use standard base64.
pub fn send_call_signaling_data(extra: RequestId, call_id: i32, data: &[u8]) -> String {
    use base64::Engine;
    json!({
        "@type": "sendCallSignalingData",
        "@extra": extra.as_extra(),
        "call_id": call_id,
        "data": base64::engine::general_purpose::STANDARD.encode(data),
    })
    .to_string()
}

/// Phase C1: `discardCall` (TDLib 1.8.67, `schema/td_api.tl:14227`):
/// `discardCall call_id:int32 is_disconnected:Bool invite_link:string
/// duration:int32 is_video:Bool connection_id:int64 = Ok;`
/// `duration` is the connected time in seconds (0 when the call never
/// reached `callStateReady`); `invite_link` is empty and
/// `connection_id` is 0 because there is no media connection yet (C2).
pub fn discard_call(
    extra: RequestId,
    call_id: i32,
    is_disconnected: bool,
    duration_secs: i32,
    is_video: bool,
) -> String {
    json!({
        "@type": "discardCall",
        "@extra": extra.as_extra(),
        "call_id": call_id,
        "is_disconnected": is_disconnected,
        "invite_link": "",
        "duration": duration_secs,
        "is_video": is_video,
        "connection_id": 0,
    })
    .to_string()
}

/// Phase C1: `sendCallRating` (TDLib 1.8.67, `schema/td_api.tl:14234`):
/// `sendCallRating call_id:InputCall rating:int32 comment:string
/// problems:vector<CallProblem> = Ok;` "Sends a call rating". The call
/// has ended, so the call is identified with `inputCallDiscarded`.
/// The star tap opens the rating detail editor (C2i:
/// `open_rating_detail` / `send_call_rating_detail`) instead of
/// calling this directly; this kept helper covers the
/// no-problems no-comment shape used in tests.
pub fn send_call_rating(extra: RequestId, call_id: i32, rating: i32) -> String {
    send_call_rating_detail(extra, call_id, rating, "", &[])
}

/// Phase C2d: `sendCallDebugInformation` (TDLib 1.8.67,
/// `schema/td_api.tl:14237`) identifies the ended call with
/// `inputCallDiscarded` (`schema/td_api.tl:7043`).
pub fn send_call_debug_information(
    extra: RequestId,
    call_id: i32,
    debug_information: &str,
) -> String {
    json!({
        "@type": "sendCallDebugInformation",
        "@extra": extra.as_extra(),
        "call_id": {
            "@type": "inputCallDiscarded",
            "call_id": call_id,
        },
        "debug_information": debug_information,
    })
    .to_string()
}

/// Phase C2i: `sendCallRating` with the full detail (TDLib 1.8.67,
/// `schema/td_api.tl:14234`):
/// `sendCallRating call_id:InputCall rating:int32 comment:string
/// problems:vector<CallProblem> = Ok;`
/// "comment: An optional user comment if the rating is less than 5;
/// problems: List of the exact types of problems with the call,
/// specified by the user". `problems` are `CallProblem` constructor
/// names (`callProblemEcho`, …, schema `:7253`-`:7277`); the call has
/// ended so it is identified with `inputCallDiscarded` (:7043).
pub fn send_call_rating_detail(
    extra: RequestId,
    call_id: i32,
    rating: i32,
    comment: &str,
    problems: &[&str],
) -> String {
    json!({
        "@type": "sendCallRating",
        "@extra": extra.as_extra(),
        "call_id": {
            "@type": "inputCallDiscarded",
            "call_id": call_id,
        },
        "rating": rating.clamp(1, 5),
        "comment": comment,
        "problems": problems.iter().map(|name| json!({"@type": name})).collect::<Vec<_>>(),
    })
    .to_string()
}

/// Phase C2i: `sendCallLog` (TDLib 1.8.67, `schema/td_api.tl:14240`):
/// `sendCallLog call_id:InputCall log_file:InputFile = Ok;`
/// "Only inputFileLocal and inputFileGenerated are supported".
pub fn send_call_log(extra: RequestId, call_id: i32, log_path: &str) -> String {
    json!({
        "@type": "sendCallLog",
        "@extra": extra.as_extra(),
        "call_id": {
            "@type": "inputCallDiscarded",
            "call_id": call_id,
        },
        "log_file": {
            "@type": "inputFileLocal",
            "path": log_path,
        },
    })
    .to_string()
}

/// `deleteAllCallMessages revoke:Bool = Ok` (TDLib 1.8.67,
/// `schema/td_api.tl:12348`): deletes all call messages; `revoke` also
/// removes them for the other side.
pub fn delete_all_call_messages(extra: RequestId, revoke: bool) -> String {
    json!({
        "@type": "deleteAllCallMessages",
        "@extra": extra.as_extra(),
        "revoke": revoke,
    })
    .to_string()
}

/// Phase C2i: `searchCallMessages` (TDLib 1.8.67,
/// `schema/td_api.tl:11903`): "Searches for call and group call
/// messages. Returns the results in reverse chronological order".
/// `searchCallMessages offset:string limit:int32 only_missed:Bool =
/// FoundMessages;`
pub fn search_call_messages(extra: RequestId, offset: &str, limit: i32) -> String {
    json!({
        "@type": "searchCallMessages",
        "@extra": extra.as_extra(),
        "offset": offset,
        "limit": limit,
        "only_missed": false,
    })
    .to_string()
}

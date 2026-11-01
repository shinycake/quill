/*
 * quill_calls.h: Quill's C API over Telegram's tgcalls call engine.
 *
 * Quill loads libquill_calls at runtime (crates/quill-calls-sys) and talks
 * to it only through the functions declared here. Everything is plain C so
 * the Rust side needs no C++ and the library can be swapped as the LGPL asks.
 *
 * Threads: callbacks run on tgcalls' own threads, never the caller's. They
 * must return quickly and must not call back into the same handle's
 * destroy function. Every other function may be called from any thread.
 *
 * Strings returned by the library stay valid for the life of the process
 * unless a function says to free them with qc_string_free.
 */
#ifndef QUILL_CALLS_H
#define QUILL_CALLS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#  if defined(QUILL_CALLS_BUILD)
#    define QC_EXPORT __declspec(dllexport)
#  else
#    define QC_EXPORT __declspec(dllimport)
#  endif
#else
#  define QC_EXPORT __attribute__((visibility("default")))
#endif

/* Bumped on every incompatible change; the loader refuses a mismatch. */
#define QC_API_VERSION 4

QC_EXPORT uint32_t qc_api_version(void);

/*
 * The call protocol versions this build of tgcalls speaks, separated by
 * '\n', for TDLib's callProtocol.library_versions. The order is Telegram
 * Desktop's: tgcalls::Meta::Versions() reversed (calls_call.cpp
 * CollectVersionsForApi).
 */
QC_EXPORT const char *qc_library_versions(void);

/* tgcalls::Meta::MaxLayer(), for callProtocol.max_layer. */
QC_EXPORT int32_t qc_max_layer(void);

/* Frees a string the library allocated for the caller. NULL is fine. */
QC_EXPORT void qc_string_free(char *string);

/* ---- Logging ------------------------------------------------------------ */

enum {
	QC_LOG_VERBOSE = 0,
	QC_LOG_INFO = 1,
	QC_LOG_WARNING = 2,
	QC_LOG_ERROR = 3,
};

/* One line of WebRTC and tgcalls logging at `level` or above. */
typedef void (*qc_log_fn)(void *user, int32_t level, const char *line);

/* Installs the process-wide log sink; NULL removes it. */
QC_EXPORT void qc_set_log(qc_log_fn log, void *user, int32_t min_level);

/* ---- Audio -------------------------------------------------------------- */

/*
 * Calls take their sound from Quill and give their sound to Quill: there are
 * no audio devices inside the library (tg_owt is built without audio
 * backends, as Telegram Desktop builds it). Audio is 48 kHz, mono, signed
 * 16-bit, in 10 ms frames of QC_AUDIO_FRAME_SAMPLES samples.
 */
#define QC_AUDIO_RATE 48000
#define QC_AUDIO_FRAME_SAMPLES 480

/* 10 ms of the call's mixed sound for Quill to play. */
typedef void (*qc_audio_out_fn)(void *user, const int16_t *samples, size_t count);

/* ---- Video -------------------------------------------------------------- */

typedef struct qc_video_frame {
	int32_t width;
	int32_t height;
	/* Clockwise degrees to rotate the picture for display: 0, 90, 180, 270. */
	int32_t rotation;
	int64_t timestamp_us;
	const uint8_t *y;
	const uint8_t *u;
	const uint8_t *v;
	int32_t stride_y;
	int32_t stride_u;
	int32_t stride_v;
} qc_video_frame;

/* A decoded or captured picture in I420; valid only during the call. */
typedef void (*qc_video_frame_fn)(void *user, const qc_video_frame *frame);

/*
 * The cameras tgcalls can open, as a JSON array of {"id": "...", "name":
 * "..."}. Free with qc_string_free.
 */
QC_EXPORT char *qc_video_input_devices(void);

enum {
	QC_VIDEO_INACTIVE = 0,
	QC_VIDEO_PAUSED = 1,
	QC_VIDEO_ACTIVE = 2,
};

/*
 * An outgoing video source (tgcalls::VideoCaptureInterface). Either a camera
 * tgcalls opens itself, as Telegram Desktop does, or an external source Quill
 * pushes pictures into (its own screen capture).
 */
typedef struct qc_video_capture qc_video_capture;

/* A camera by id from qc_video_input_devices; NULL or "" is the default. */
QC_EXPORT qc_video_capture *qc_video_capture_camera(const char *device_id);

/* A source fed with qc_video_capture_push_i420. */
QC_EXPORT qc_video_capture *qc_video_capture_external(int32_t is_screencast);

/* Pushes one picture into an external source; ignored for a camera. */
QC_EXPORT void qc_video_capture_push_i420(qc_video_capture *capture, const qc_video_frame *frame);

/* Switches a camera source to another device. */
QC_EXPORT void qc_video_capture_switch_camera(qc_video_capture *capture, const char *device_id);

/* QC_VIDEO_*; a paused source sends nothing and tells the peer it paused. */
QC_EXPORT void qc_video_capture_set_state(qc_video_capture *capture, int32_t state);

/* The local preview: every picture the source sends. NULL removes it. */
QC_EXPORT void qc_video_capture_set_preview(qc_video_capture *capture, qc_video_frame_fn preview, void *user);

/* Releases Quill's reference; calls holding the source keep it alive. */
QC_EXPORT void qc_video_capture_free(qc_video_capture *capture);

/* ---- 1:1 calls (tgcalls::Instance) -------------------------------------- */

typedef struct qc_p2p qc_p2p;

/* tgcalls::State */
enum {
	QC_P2P_WAIT_INIT = 0,
	QC_P2P_WAIT_INIT_ACK = 1,
	QC_P2P_ESTABLISHED = 2,
	QC_P2P_FAILED = 3,
	QC_P2P_RECONNECTING = 4,
};

typedef struct qc_p2p_callbacks {
	void *user;
	/* QC_P2P_* */
	void (*state)(void *user, int32_t state);
	/* 0 to 4 bars, tgcalls' connection quality. */
	void (*signal_bars)(void *user, int32_t bars);
	/* The peer's microphone (1 = on) and video (QC_VIDEO_*). */
	void (*remote_media_state)(void *user, int32_t audio_active, int32_t video_state);
	void (*remote_battery_low)(void *user, int32_t is_low);
	/* Bytes for TDLib sendCallSignalingData. */
	void (*signaling)(void *user, const uint8_t *data, size_t length);
	/* The call's sound, every 10 ms while the call runs. */
	qc_audio_out_fn audio_out;
	/* The peer's video. */
	qc_video_frame_fn remote_video;
} qc_p2p_callbacks;

/*
 * Starts a call from TDLib's callStateReady, as JSON:
 * {
 *   "version": "9.0.0",                 protocol.library_versions[0]
 *   "outgoing": true,
 *   "encryption_key": "<base64, 256 bytes>",
 *   "allow_p2p": true,
 *   "max_layer": 92,                     protocol.max_layer
 *   "connect_timeout_ms": 30000,         option call_connect_timeout_ms; optional, default 30000
 *   "packet_timeout_ms": 10000,          option call_packet_timeout_ms; optional, default 10000
 *   "custom_parameters": "{...}",        callStateReady.custom_parameters
 *   "servers": [                         callStateReady.servers
 *     {"id": "1", "ipv4": "...", "ipv6": "...", "port": 1400,
 *      "reflector": {"peer_tag": "<base64, 16 bytes>", "tcp": false}},
 *     {"id": "2", "ipv4": "...", "ipv6": "...", "port": 3478,
 *      "webrtc": {"username": "...", "password": "...", "turn": true, "stun": true}}
 *   ],                                   ids are int64 strings
 *   "proxy": {"host": "...", "port": 1080, "login": "...", "password": "..."}   optional
 * }
 * The servers become tgcalls endpoints and TURN/STUN servers exactly as
 * Telegram Desktop maps phoneCall.connections. Returns NULL and logs the
 * reason when tgcalls can't create the call (an unknown version, a malformed
 * key).
 */
QC_EXPORT qc_p2p *qc_p2p_create(const char *config_json, const qc_p2p_callbacks *callbacks, qc_video_capture *video);

/* TDLib updateNewCallSignalingData. */
QC_EXPORT void qc_p2p_receive_signaling(qc_p2p *call, const uint8_t *data, size_t length);

/* One 10 ms microphone frame (QC_AUDIO_FRAME_SAMPLES samples). */
QC_EXPORT void qc_p2p_push_audio(qc_p2p *call, const int16_t *samples, size_t count);

QC_EXPORT void qc_p2p_set_muted(qc_p2p *call, int32_t muted);

/* The source to send, or NULL to stop sending video. */
QC_EXPORT void qc_p2p_set_video_capture(qc_p2p *call, qc_video_capture *video);

/* The aspect ratio Quill shows the peer's video at, for the peer's encoder. */
QC_EXPORT void qc_p2p_set_requested_aspect(qc_p2p *call, float aspect);

QC_EXPORT void qc_p2p_set_low_battery(qc_p2p *call, int32_t is_low);

/* tgcalls' debug line for the log. Free with qc_string_free. */
QC_EXPORT char *qc_p2p_debug_info(qc_p2p *call);

/* Why the call failed, after QC_P2P_FAILED. Free with qc_string_free. */
QC_EXPORT char *qc_p2p_last_error(qc_p2p *call);

/*
 * Stops the call and frees it. Blocks until tgcalls has stopped, so no
 * callback runs after it returns; never call it from a callback.
 */
QC_EXPORT void qc_p2p_destroy(qc_p2p *call);

/* ---- Group calls (tgcalls::GroupInstanceCustomImpl) --------------------- */

/*
 * A group call (video chat) or its screen-sharing (presentation) instance,
 * driven the way Telegram Desktop drives them (calls_group_call.cpp,
 * GroupCall::tryCreateController and tryCreateScreencast). A presentation is
 * a second instance next to the call's main one, with its own join payload
 * and join response (TDLib startGroupCallScreenSharing).
 *
 * Quill answers tgcalls' requests (who an ssrc belongs to, the stream's
 * time, stream parts) asynchronously: a request callback hands Quill a
 * request id, and Quill later answers it with the matching
 * qc_group_answer_* function, from any thread. Answering an id that was
 * cancelled, already answered or never issued does nothing.
 */
typedef struct qc_group qc_group;

/* tgcalls::GroupConnectionMode */
enum {
	/* Not connected: before the join response, and while rejoining. */
	QC_GROUP_MODE_NONE = 0,
	/* WebRTC to the call's server, after joinGroupCall's answer. */
	QC_GROUP_MODE_RTC = 1,
	/* Stream parts Quill downloads (a live stream, or a call too big for RTC). */
	QC_GROUP_MODE_BROADCAST = 2,
};

/* tgcalls::VideoChannelDescription::Quality */
enum {
	QC_GROUP_QUALITY_THUMBNAIL = 0,
	QC_GROUP_QUALITY_MEDIUM = 1,
	QC_GROUP_QUALITY_FULL = 2,
};

/* tgcalls::BroadcastPart::Status */
enum {
	/* The part's bytes are attached. */
	QC_BROADCAST_PART_SUCCESS = 0,
	/* The part isn't there yet (a flood wait, TIME_TOO_BIG); asked again later. */
	QC_BROADCAST_PART_NOT_READY = 1,
	/* Any other failure: tgcalls asks for the server's time again. */
	QC_BROADCAST_PART_RESYNC_NEEDED = 2,
};

/* One speaker's level (tgcalls::GroupLevelUpdate). */
typedef struct qc_group_level {
	/* The participant's audio ssrc; 0 is Quill's own microphone. */
	uint32_t ssrc;
	/* 0 to 1. */
	float level;
	/* 1 when tgcalls hears speech rather than noise. */
	int32_t voice;
} qc_group_level;

/* Who an audio ssrc belongs to (tgcalls::MediaChannelDescription). */
typedef struct qc_group_media_channel {
	uint32_t audio_ssrc;
	/* The participant's Telegram id (a user, or a chat joined as). */
	int64_t user_id;
} qc_group_media_channel;

/* A stream part tgcalls wants (tdesktop's LoadPartTask). */
typedef struct qc_group_broadcast_part {
	/*
	 * The part's start in server milliseconds; 0 means now (tdesktop asks
	 * for the server's unixtime * 1000).
	 */
	int64_t timestamp_ms;
	/* 1000, 500, 250 or 125: TDLib getVideoChatStreamSegment's scale 0 to 3. */
	int64_t duration_ms;
	/* 0 for the audio part, 1 for a video channel's part. */
	int32_t is_video;
	/* For video: the stream channel and QC_GROUP_QUALITY_*. */
	int32_t video_channel;
	int32_t video_quality;
} qc_group_broadcast_part;

/*
 * One frame of a conference call (an end-to-end encrypted group call) for
 * callbacks.e2e: tgcalls' e2eEncryptDecrypt, which Telegram Desktop answers
 * with tde2e's call_encrypt and call_decrypt.
 */
typedef struct qc_group_e2e_frame {
	const uint8_t *data;
	size_t length;
	/* 1 to encrypt an outgoing frame, 0 to decrypt an incoming one. */
	int32_t is_encrypt;
	/* Decrypting: the Telegram user id of the participant who sent it. */
	int64_t user_id;
	/* Encrypting: how many bytes at the start stay in the clear. */
	int32_t unencrypted_prefix_size;
} qc_group_e2e_frame;

/*
 * Hands callbacks.e2e's answer back to the library, which copies it. Call
 * it at most once, before the callback returns, with the `result` the
 * callback was given.
 */
typedef void (*qc_group_e2e_result_fn)(void *result, const uint8_t *data, size_t length);

typedef struct qc_group_callbacks {
	void *user;
	/*
	 * tgcalls' network state: connected (to RTC or to the stream), and
	 * whether it is moving from the stream to RTC.
	 */
	void (*network_state)(void *user, int32_t connected, int32_t transitioning_to_rtc);
	/*
	 * Speaker levels, a batch at a time. A batch holding only Quill's own
	 * level (ssrc 0) comes only when that level changed, as in tdesktop.
	 * Not called for a presentation.
	 */
	void (*audio_levels)(void *user, const qc_group_level *levels, size_t count);
	/* The call's mixed sound, every 10 ms. Not called for a presentation. */
	qc_audio_out_fn audio_out;
	/*
	 * A picture from a participant's video, keyed by its endpoint id, for
	 * each endpoint added with qc_group_add_incoming_video.
	 */
	void (*incoming_video)(void *user, const char *endpoint, const qc_video_frame *frame);
	/*
	 * tgcalls heard ssrcs it doesn't know: answer with
	 * qc_group_answer_media_channels (tdesktop resolves them to
	 * participants, loading them when needed). NULL lets tgcalls accept
	 * every ssrc without knowing who it is, which a conference call can't
	 * do: callbacks.e2e needs the sender of every frame it decrypts. Not
	 * called for a presentation.
	 */
	void (*request_media_channels)(void *user, uint64_t request_id, const uint32_t *ssrcs, size_t count);
	/*
	 * Broadcast mode wants the stream's current server time in ms: answer
	 * with qc_group_answer_current_time. tdesktop answers with the last
	 * timestamp of getGroupCallStreamChannels' first channel for an RTMP
	 * stream, and with its own estimate of the server time otherwise.
	 */
	void (*request_current_time)(void *user, uint64_t request_id);
	/* Broadcast mode wants a stream part: answer with qc_group_answer_broadcast_part. */
	void (*request_broadcast_part)(void *user, uint64_t request_id, const qc_group_broadcast_part *part);
	/*
	 * tgcalls no longer wants request `request_id` (any kind): Quill may
	 * stop loading it. An answer after this is ignored.
	 */
	void (*request_cancelled)(void *user, uint64_t request_id);
	/*
	 * A conference call's frame to encrypt or decrypt, only for an
	 * instance created with "e2e": true. Synchronous, as tgcalls wants
	 * it: the transformed frame goes to `set_result(result, ...)` before
	 * the callback returns. A frame with no result is dropped, as tdesktop
	 * drops a frame tde2e refuses. Runs on tgcalls' media threads for
	 * every audio and video frame, so it must answer fast.
	 */
	void (*e2e)(void *user, const qc_group_e2e_frame *frame, qc_group_e2e_result_fn set_result, void *result);
} qc_group_callbacks;

/*
 * Creates a group call instance, configured as Telegram Desktop configures
 * its own. `config_json` is an object, every key optional:
 * {
 *   "presentation": false,     true makes the screen-sharing instance
 *                              (tdesktop's _screenInstance): screencast video,
 *                              no levels, sound or media channel requests
 *   "prioritize_vp8": false,   app config confcall_prioritize_vp8
 *   "e2e": false               a conference call: every frame goes through
 *                              callbacks.e2e (tdesktop's e2eEncryptDecrypt,
 *                              set for both the call and its presentation)
 * }
 * `video` is the source to send (NULL for none); a presentation sends an
 * external screencast source. The instance starts in QC_GROUP_MODE_NONE
 * with no connection until qc_group_set_connection_mode. Returns NULL and
 * logs the reason for a malformed config, and for "e2e": true without
 * callbacks.e2e (a conference call never runs in the clear).
 */
QC_EXPORT qc_group *qc_group_create(const char *config_json, const qc_group_callbacks *callbacks, qc_video_capture *video);

/*
 * The join payload for TDLib joinGroupCall (or startGroupCallScreenSharing),
 * tdesktop's GroupJoinPayload: JSON with "ufrag", "pwd", "fingerprints"
 * ([{"hash", "setup", "fingerprint"}]), "ssrc" (the audio ssrc as a signed
 * 32-bit number, as Telegram sends ssrcs) and "ssrc-groups" for video. Blocks until tgcalls has made
 * it, at most 2 seconds, and returns NULL if it did not; never call it from
 * a callback. Stores the audio ssrc it announces in `audio_ssrc` when that
 * is not NULL. Free with qc_string_free.
 */
QC_EXPORT char *qc_group_join_payload(qc_group *group, uint32_t *audio_ssrc);

/* The join response (groupCallJoinParameters' payload) for this payload. */
QC_EXPORT void qc_group_set_join_response(qc_group *group, const char *payload_json);

/*
 * QC_GROUP_MODE_*. A broadcast that was on stays on until RTC connects (as
 * tdesktop asks); `is_unified_broadcast` is 1 for an RTMP stream, whose
 * parts carry audio and video together.
 */
QC_EXPORT void qc_group_set_connection_mode(qc_group *group, int32_t mode, int32_t is_unified_broadcast);

/*
 * Mutes or unmutes the microphone; an instance starts muted. For a
 * presentation, unmuted shares the sound Quill pushes with the screen.
 */
QC_EXPORT void qc_group_set_muted(qc_group *group, int32_t muted);

/* One 10 ms microphone frame (QC_AUDIO_FRAME_SAMPLES samples). */
QC_EXPORT void qc_group_push_audio(qc_group *group, const int16_t *samples, size_t count);

/*
 * The volume of the participant sending audio ssrc `ssrc`: 1.0 is as sent,
 * 0 to 2 (TDLib's volume_level / 10000). tdesktop sets ssrc 1 for a
 * stream's single source.
 */
QC_EXPORT void qc_group_set_volume(qc_group *group, uint32_t ssrc, double volume);

/* Participants who left: stop playing their ssrcs. */
QC_EXPORT void qc_group_remove_ssrcs(qc_group *group, const uint32_t *ssrcs, size_t count);

/*
 * The participants' videos to receive, replacing the previous set, as a JSON
 * array from groupCallParticipantVideoInfo:
 * [{"audio_ssrc": 1234, "user_id": 42, "endpoint": "...",
 *   "ssrc_groups": [{"semantics": "SIM", "ssrcs": [1, 2, 3]}],
 *   "min_quality": 0, "max_quality": 2}]                      QC_GROUP_QUALITY_*
 * tdesktop caps what it asks for (one full video counts as four medium,
 * at most sixteen medium); that budget is the caller's. Returns 0 and
 * changes nothing for malformed JSON, 1 otherwise.
 */
QC_EXPORT int32_t qc_group_set_requested_video(qc_group *group, const char *channels_json);

/*
 * Delivers endpoint `endpoint`'s pictures to callbacks.incoming_video,
 * now or once it arrives. Adding an endpoint twice does nothing.
 */
QC_EXPORT void qc_group_add_incoming_video(qc_group *group, const char *endpoint);

/* Stops delivering `endpoint`'s pictures. */
QC_EXPORT void qc_group_remove_incoming_video(qc_group *group, const char *endpoint);

/* The source to send, or NULL to stop sending video. */
QC_EXPORT void qc_group_set_video_capture(qc_group *group, qc_video_capture *video);

/*
 * Answers request_media_channels: the channels Quill could resolve. An ssrc
 * left out is one Quill gave up on (tdesktop's resolved but unknown ssrc).
 */
QC_EXPORT void qc_group_answer_media_channels(qc_group *group, uint64_t request_id, const qc_group_media_channel *channels, size_t count);

/* Answers request_current_time; 0 means there is no stream yet. */
QC_EXPORT void qc_group_answer_current_time(qc_group *group, uint64_t request_id, int64_t timestamp_ms);

/*
 * Answers request_broadcast_part with QC_BROADCAST_PART_* and, on success,
 * the part's bytes (copied). `timestamp_ms` is the part Quill loaded: the
 * request's timestamp_ms, or the time Quill chose when that was 0.
 * `response_time` is the server time the answer was sent at, in seconds
 * (tdesktop: the reply's message id time); tgcalls resyncs to it after
 * NOT_READY and RESYNC_NEEDED.
 */
QC_EXPORT void qc_group_answer_broadcast_part(qc_group *group, uint64_t request_id, int32_t status, int64_t timestamp_ms, double response_time, const uint8_t *data, size_t length);

/*
 * Stops the instance and frees it. Blocks until tgcalls has stopped, so no
 * callback runs after it returns; never call it from a callback. Requests
 * still open are dropped without request_cancelled.
 */
QC_EXPORT void qc_group_destroy(qc_group *group);

#ifdef __cplusplus
}
#endif

#endif /* QUILL_CALLS_H */

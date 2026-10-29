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
#define QC_API_VERSION 2

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
 *   "connect_timeout_ms": 30000,         option call_connect_timeout_ms
 *   "packet_timeout_ms": 10000,          option call_packet_timeout_ms
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

#ifdef __cplusplus
}
#endif

#endif /* QUILL_CALLS_H */

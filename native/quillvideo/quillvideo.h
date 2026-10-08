/*
 * quillvideo: a small, stable C ABI over FFmpeg's libavformat/libavcodec/
 * libswscale/libswresample, loaded at runtime by Quill (src/video_decode).
 *
 * The shim exists so the Rust side never touches FFmpeg structs: it is
 * compiled against the exact FFmpeg headers it ships with, and Quill only
 * sees the plain structs below. One context reads one local file; calls on a
 * context must not overlap (Quill drives each from a single decode thread).
 *
 * MIT licensed like Quill. FFmpeg itself is LGPL-2.1+ and linked dynamically
 * (see docs/decisions/codex-video-cross-platform.md).
 */
#ifndef QUILLVIDEO_H
#define QUILLVIDEO_H

#include <stdint.h>

#ifdef _WIN32
#define QV_API __declspec(dllexport)
#else
#define QV_API __attribute__((visibility("default")))
#endif

/* Bumped whenever a struct or signature below changes. */
#define QV_ABI_VERSION 1

typedef struct QvOptions {
  /* Scale video down (never up) to fit within this box, keeping the aspect
   * ratio. 0 means no bound on that axis. */
  int32_t max_width;
  int32_t max_height;
  /* Decoder threads; 0 lets FFmpeg pick. */
  int32_t threads;
  /* Decode the soundtrack too (otherwise audio packets are skipped). */
  int32_t want_audio;
} QvOptions;

typedef struct QvInfo {
  /* Seconds; <= 0 when unknown. */
  double duration;
  /* Average frame rate; 0 when unknown. */
  double frame_rate;
  /* Decoded (pre-rotation) output size of video frames. */
  int32_t width;
  int32_t height;
  /* Clockwise rotation the picture must be shown with: 0, 90, 180, 270. */
  int32_t rotation;
  int32_t has_video;
  int32_t has_audio;
  /* Audio is delivered as interleaved f32 at this rate and channel count. */
  int32_t sample_rate;
  int32_t channels;
  /* NUL-terminated short codec names, e.g. "h264", "aac". */
  char video_codec[24];
  char audio_codec[24];
} QvInfo;

#define QV_ITEM_VIDEO 1
#define QV_ITEM_AUDIO 2

typedef struct QvItem {
  int32_t kind;
  /* Presentation time in seconds from the start of the file. */
  double pts;
  /* Video: BGRA rows, `stride` bytes apart. Audio: interleaved f32.
   * Owned by the context; valid until the next call on it. */
  const uint8_t *data;
  int32_t stride;
  int32_t width;
  int32_t height;
  /* Audio: sample frames (per channel) in `data`. */
  int32_t samples;
} QvItem;

#ifdef __cplusplus
extern "C" {
#endif

QV_API int32_t qv_abi_version(void);

/* Open a local file (UTF-8 path). NULL on failure, with a message in `err`. */
QV_API void *qv_open(const char *path, const QvOptions *options, char *err,
                     int32_t err_len);

QV_API void qv_info(void *ctx, QvInfo *out);

/* Decode the next picture or audio chunk in presentation order per stream.
 * Returns 1 with `out` filled, 0 at the end of the file, < 0 on error. */
QV_API int32_t qv_next(void *ctx, QvItem *out);

/* Seek so the next items start at `seconds` (frames before it are decoded
 * but not returned). Returns 0 on success. */
QV_API int32_t qv_seek(void *ctx, double seconds);

QV_API void qv_close(void *ctx);

#ifdef __cplusplus
}
#endif

#endif

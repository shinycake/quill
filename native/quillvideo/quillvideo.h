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
#define QV_ABI_VERSION 4

typedef struct QvOptions {
  /* Scale video down (never up) to fit within this box, keeping the aspect
   * ratio. 0 means no bound on that axis. */
  int32_t max_width;
  int32_t max_height;
  /* Decoder threads; 0 lets FFmpeg pick. */
  int32_t threads;
  /* Decode the soundtrack too (otherwise audio packets are skipped). */
  int32_t want_audio;
  /* ABI 3: QV_HW_*. With QV_HW_ALLOWED the video decodes on the GPU when
   * the platform and codec allow it, and in software otherwise. */
  int32_t hw;
} QvOptions;

#define QV_HW_OFF 0
#define QV_HW_ALLOWED 1
/* Tests: as QV_HW_ALLOWED, with a stand-in hardware decoder whose third
 * picture fails like a GPU transfer does, so the software fallback runs on
 * machines without a GPU. */
#define QV_HW_FAIL_THIRD_PICTURE 2
/* Tests: as QV_HW_FAIL_THIRD_PICTURE, and the software decoder then fails to
 * open, so the hardware decoder has to stay in charge. */
#define QV_HW_FAIL_THIRD_PICTURE_AND_REOPEN 3
/* Tests: as QV_HW_ALLOWED, with a stand-in hardware decoder that fails once
 * as it drains at the end of the file, after every picture came out. */
#define QV_HW_FAIL_AT_END 4

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
  /* ABI 3: 1 while the video decodes in hardware. It drops to 0, and stays
   * there, when the decoder hands out software pictures or the hardware
   * fails (the shim then reopens the decoder in software and carries on). */
  int32_t hw;
} QvInfo;

#define QV_ITEM_VIDEO 1
#define QV_ITEM_AUDIO 2

/* Picture layouts (QvItem.format, qv_set_output). */
#define QV_PIXEL_BGRA 0
/* Bi-planar 4:2:0: `height` luma rows of `width` bytes at `data`, `stride`
 * apart, then `height / 2` rows of `width / 2` interleaved (Cb, Cr) byte
 * pairs at `chroma`, `chroma_stride` apart. Width and height are even. */
#define QV_PIXEL_NV12 1

/* YUV matrices (QvItem.matrix). */
#define QV_MATRIX_BT601 1
#define QV_MATRIX_BT709 2

typedef struct QvItem {
  int32_t kind;
  /* Presentation time in seconds from the start of the file. */
  double pts;
  /* Video: BGRA rows, `stride` bytes apart, or the NV12 luma plane (see
   * QV_PIXEL_NV12). Audio: interleaved f32.
   * Owned by the context; valid until the next call on it. */
  const uint8_t *data;
  int32_t stride;
  int32_t width;
  int32_t height;
  /* Audio: sample frames (per channel) in `data`. */
  int32_t samples;
  /* Video (ABI 2): QV_PIXEL_BGRA or QV_PIXEL_NV12. */
  int32_t format;
  /* NV12: the interleaved chroma plane and its row stride in bytes. */
  const uint8_t *chroma;
  int32_t chroma_stride;
  /* NV12: QV_MATRIX_BT601 or QV_MATRIX_BT709; 1 for full range samples
   * (0-255), 0 for limited (16-235). */
  int32_t matrix;
  int32_t full_range;
  /* Video: microseconds spent turning the decoded picture into this layout
   * (sws_scale, or the plane copy for NV12). */
  int32_t convert_us;
} QvItem;

#ifdef __cplusplus
extern "C" {
#endif

QV_API int32_t qv_abi_version(void);

/* Open a local file (UTF-8 path). NULL on failure, with a message in `err`. */
QV_API void *qv_open(const char *path, const QvOptions *options, char *err,
                     int32_t err_len);

QV_API void qv_info(void *ctx, QvInfo *out);

/* The picture layout wanted from now on: QV_PIXEL_BGRA (the default) or
 * QV_PIXEL_NV12. NV12 is a request: pictures that are 8-bit 4:2:0 YUV
 * without alpha (yuv420p, yuvj420p, nv12), BT.601 or BT.709, with even
 * output sides come out as NV12 without a colour conversion; the rest
 * still come out as BGRA, so check QvItem.format. Returns 0, or < 0 for an
 * unknown layout. */
QV_API int32_t qv_set_output(void *ctx, int32_t format);

/* Decode the next picture or audio chunk in presentation order per stream.
 * Returns 1 with `out` filled, 0 at the end of the file, < 0 on error. Too
 * many corrupt video packets in a row are an error. */
QV_API int32_t qv_next(void *ctx, QvItem *out);

/* ABI 4: decode the next audio chunk while the caller has pictures enough.
 * Video packets read on the way wait undecoded for qv_next, which goes on
 * from them. Returns 1 with `out` filled, 0 when no sound is in reach (it
 * ended, or too many packets wait: a qv_next has to take some first), < 0 on
 * error. */
QV_API int32_t qv_next_sound(void *ctx, QvItem *out);

/* Seek so the next items start at `seconds` (frames before it are decoded
 * but not returned). Past the last picture, that picture is returned at the
 * end. Returns 0 on success. */
QV_API int32_t qv_seek(void *ctx, double seconds);

QV_API void qv_close(void *ctx);

#ifdef __cplusplus
}
#endif

#endif

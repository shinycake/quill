/*
 * quillsticker: decode a WebM video sticker (VP9 or VP8, optionally with an
 * alpha channel) to straight-alpha BGRA frames, in process. Built into the
 * same library as quillvideo (scripts/build-ffmpeg.sh) and loaded by Quill's
 * src/sticker_playback/webm_ffi.rs.
 *
 * WebM stores a sticker's alpha as a second VP9/VP8 stream in each block's
 * BlockAdditional (id 1), whose luma plane is the alpha plane. FFmpeg's
 * native decoders ignore it, and libvpx (which reads it, and which Telegram
 * Desktop links) is not part of Quill's LGPL decoder-only build. This shim
 * runs a second native decoder over that side stream instead and merges its
 * luma into the picture as the alpha plane, which is what libavcodec's
 * libvpxdec.c does with two libvpx decoders.
 *
 * One context reads one local file; calls on a context must not overlap.
 * MIT licensed like Quill.
 */
#ifndef QUILLSTICKER_H
#define QUILLSTICKER_H

#include <stdint.h>

#ifdef _WIN32
#define QS_API __declspec(dllexport)
#else
#define QS_API __attribute__((visibility("default")))
#endif

/* Bumped whenever a struct or signature below changes. */
#define QS_ABI_VERSION 1

typedef struct QsInfo {
  /* Seconds; <= 0 when unknown. */
  double duration;
  /* Average frame rate; 0 when unknown. */
  double frame_rate;
  /* Output size: the picture fit into the caller's box (enlarged or
   * shrunk, aspect kept). */
  int32_t width;
  int32_t height;
  /* The stream is tagged as carrying an alpha channel (alpha_mode). */
  int32_t has_alpha;
} QsInfo;

typedef struct QsFrame {
  /* Presentation time in seconds from the start of the file. */
  double pts;
  /* Straight (not premultiplied) BGRA rows, `stride` bytes apart. Owned by
   * the context; valid until the next call on it. */
  const uint8_t *data;
  int32_t stride;
  int32_t width;
  int32_t height;
} QsFrame;

#ifdef __cplusplus
extern "C" {
#endif

QS_API int32_t qs_abi_version(void);

/* Open a local file (UTF-8 path), fitting pictures into box_w x box_h
 * (1..4096 each). NULL on failure, with a message in `err`. */
QS_API void *qs_open(const char *path, int32_t box_w, int32_t box_h, char *err,
                     int32_t err_len);

QS_API void qs_info(void *ctx, QsInfo *out);

/* Decode the next shown picture and report its time in `*pts`; the picture
 * is converted only if qs_pixels is called. Returns 1 for a picture, 0 at
 * the end of the file, < 0 on error. */
QS_API int32_t qs_next(void *ctx, double *pts);

/* Convert the picture qs_next last returned. Returns 0 with `out` filled,
 * < 0 on error. */
QS_API int32_t qs_pixels(void *ctx, QsFrame *out);

QS_API void qs_close(void *ctx);

#ifdef __cplusplus
}
#endif

#endif

/*
 * Where quillvideo.c and quillsticker.c have swscale write a BGRA picture.
 *
 * swscale's x86 unscaled YUV->BGRA kernels (picked when the size does not
 * change and SWS_ACCURATE_RND is off) write whole 8 and 16 pixel blocks:
 * past the end of a row and past the last row, and they skip the right
 * columns when a block would not fit in the stride. Rows padded to 64 bytes
 * and 64 spare bytes after the last row leave them room, the layout Telegram
 * Desktop uses (kAlignImageBy, ComputeAlignedFrameStorageLayout).
 * native/quillvideo/tests/bgra_layout_test.c checks it against swscale.
 * MIT licensed like Quill.
 */
#ifndef QUILL_BGRA_LAYOUT_H
#define QUILL_BGRA_LAYOUT_H

#include <libavutil/macros.h>
#include <stddef.h>

#define QV_BGRA_ALIGN 64

/* Bytes from one row to the next. */
static inline int qv_bgra_stride(int width) {
  return FFALIGN(width * 4, QV_BGRA_ALIGN);
}

/* Bytes to allocate for `height` rows. */
static inline size_t qv_bgra_size(int width, int height) {
  return (size_t)qv_bgra_stride(width) * (size_t)height + QV_BGRA_ALIGN;
}

#endif

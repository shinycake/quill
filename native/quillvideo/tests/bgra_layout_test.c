/*
 * Checks bgra_layout.h against the swscale it is built with: for clip sizes
 * whose width is not a multiple of 16, converting YUV to BGRA at the same
 * size (swscale's unscaled fast path, which quillvideo.c and quillsticker.c
 * take) writes every column and nothing past qv_bgra_size. Run by
 * scripts/test-bgra-layout.sh.
 */
#include "../bgra_layout.h"

#include <libavutil/mem.h>
#include <libswscale/swscale.h>
#include <stdio.h>
#include <string.h>

#define GUARD 256
#define FILL 0xAB

/* 0 when the conversion stayed inside the layout and wrote every pixel. */
static int check(int w, int h, enum AVPixelFormat format, int flags,
                 const char *name) {
  struct SwsContext *sws = sws_getContext(w, h, format, w, h, AV_PIX_FMT_BGRA,
                                          flags, NULL, NULL, NULL);
  if (!sws) {
    printf("FAIL %s %dx%d: no swscale context\n", name, w, h);
    return 1;
  }
  int cw = (w + 1) / 2, ch = (h + 1) / 2;
  int linesize[4] = {FFALIGN(w, 64), FFALIGN(cw, 64), FFALIGN(cw, 64),
                     FFALIGN(w, 64)};
  int rows[4] = {h, ch, ch, h};
  int planes = format == AV_PIX_FMT_YUVA420P ? 4 : 3;
  uint8_t *src[4] = {NULL, NULL, NULL, NULL};
  for (int p = 0; p < planes; p++) {
    src[p] = av_malloc((size_t)linesize[p] * rows[p] + 64);
    memset(src[p], p == 3 ? 255 : 128, (size_t)linesize[p] * rows[p] + 64);
  }
  int stride = qv_bgra_stride(w);
  size_t size = qv_bgra_size(w, h);
  uint8_t *dst = av_malloc(size + GUARD);
  memset(dst, FILL, size + GUARD);
  uint8_t *dst_planes[4] = {dst, NULL, NULL, NULL};
  int dst_stride[4] = {stride, 0, 0, 0};
  sws_scale(sws, (const uint8_t *const *)src, linesize, 0, h, dst_planes,
            dst_stride);

  int past = 0;
  for (int i = 0; i < GUARD; i++)
    if (dst[size + i] != FILL)
      past = i + 1;
  int unwritten = 0;
  for (int y = 0; y < h; y++)
    for (int x = 0; x < w; x++)
      if (dst[(size_t)y * stride + (size_t)x * 4 + 3] != 255)
        unwritten++;
  int failed = past != 0 || unwritten != 0;
  printf("%s %s %dx%d: %d bytes past the buffer, %d pixels unwritten\n",
         failed ? "FAIL" : "ok  ", name, w, h, past, unwritten);
  sws_freeContext(sws);
  av_free(dst);
  for (int p = 0; p < planes; p++)
    av_free(src[p]);
  return failed;
}

int main(void) {
  /* Portrait and odd-width clips; the first ten broke the tight layout. */
  static const int sizes[][2] = {
      {1080, 1920}, {360, 640},  {360, 360}, {426, 240},  {540, 960},
      {590, 1280},  {1278, 720}, {854, 480}, {853, 480},  {498, 280},
      {1920, 1080}, {1280, 720}, {848, 480}, {512, 360},  {361, 512},
      {2, 2},       {3, 3},      {17, 9},    {4095, 33},
  };
  int failures = 0;
  for (size_t i = 0; i < sizeof sizes / sizeof sizes[0]; i++) {
    int w = sizes[i][0], h = sizes[i][1];
    /* quillvideo.c's BGRA path, and quillsticker.c's with and without alpha. */
    failures += check(w, h, AV_PIX_FMT_YUV420P, SWS_BILINEAR, "video");
    failures += check(w, h, AV_PIX_FMT_YUV420P, SWS_BICUBIC, "sticker");
    failures += check(w, h, AV_PIX_FMT_YUVA420P, SWS_BICUBIC, "sticker+alpha");
  }
  if (failures) {
    printf("%d conversions broke the layout\n", failures);
    return 1;
  }
  return 0;
}

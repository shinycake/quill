/*
 * quillsticker: see quillsticker.h. Demuxes one local WebM with
 * libavformat, decodes the picture with the native VP9/VP8 decoder and,
 * when blocks carry an alpha side stream (Matroska BlockAdditional id 1), a
 * second native decoder for that stream; converts to straight-alpha BGRA
 * with libswscale, fit into the caller's box.
 */
#include "quillsticker.h"
#include "bgra_layout.h"

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/intreadwrite.h>
#include <libavutil/pixdesc.h>
#include <libswscale/swscale.h>

#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Telegram's video stickers are 512 px on their long side; anything far
 * past that is not a sticker. */
#define QS_MAX_SIDE 4096
/* BlockAddID of the alpha stream (Matroska/WebM "alpha_mode"). */
#define QS_ALPHA_ID 1

typedef struct QsCtx {
  AVFormatContext *fmt;
  AVCodecContext *dec;
  /* The alpha side-stream decoder, opened on the first alpha block. */
  AVCodecContext *adec;
  int vidx;
  AVPacket *pkt;
  AVPacket *apkt;
  AVFrame *frame;
  AVFrame *aframe;
  AVFrame *atmp;
  /* qs_next returned `frame`; `aframe` holds the alpha for it. */
  int have_frame;
  int have_alpha;
  int eof_sent;
  double start;
  /* Time of the picture qs_next returned, and of the one after it. */
  double cur_pts;
  double next_pts;
  int box_w;
  int box_h;
  struct SwsContext *sws;
  int sws_w, sws_h, sws_fmt, sws_ow, sws_oh, sws_cs, sws_range;
  uint8_t *rgb;
  size_t rgb_size;
  QsInfo info;
} QsCtx;

int32_t qs_abi_version(void) { return QS_ABI_VERSION; }

static void qs_set_err(char *err, int32_t len, const char *what, int code) {
  if (!err || len <= 0)
    return;
  if (code < 0) {
    char buf[128];
    av_strerror(code, buf, sizeof buf);
    snprintf(err, (size_t)len, "%s: %s", what, buf);
  } else {
    snprintf(err, (size_t)len, "%s", what);
  }
}

/* A single-threaded native decoder: one frame out per frame in, so the
 * picture and alpha decoders stay in step. */
static AVCodecContext *qs_open_decoder(const AVStream *st, int *code) {
  const AVCodec *codec = avcodec_find_decoder(st->codecpar->codec_id);
  if (!codec) {
    *code = AVERROR_DECODER_NOT_FOUND;
    return NULL;
  }
  AVCodecContext *dec = avcodec_alloc_context3(codec);
  if (!dec) {
    *code = AVERROR(ENOMEM);
    return NULL;
  }
  *code = avcodec_parameters_to_context(dec, st->codecpar);
  if (*code < 0) {
    avcodec_free_context(&dec);
    return NULL;
  }
  dec->pkt_timebase = st->time_base;
  dec->thread_count = 1;
  *code = avcodec_open2(dec, codec, NULL);
  if (*code < 0) {
    avcodec_free_context(&dec);
    return NULL;
  }
  return dec;
}

/* `w` x `h` with sample aspect `sar`, fit into the box (enlarged or
 * shrunk, like ffmpeg's scale=W:H:force_original_aspect_ratio=decrease). */
static void qs_output_size(const QsCtx *c, int w, int h, AVRational sar,
                           int *ow, int *oh) {
  double dw = w, dh = h;
  if (sar.num > 0 && sar.den > 0 && sar.num != sar.den)
    dw = dw * sar.num / sar.den;
  double scale = fmin(c->box_w / dw, c->box_h / dh);
  if (!isfinite(scale) || scale <= 0)
    scale = 1.0;
  *ow = (int)lround(fmin(dw * scale, c->box_w));
  *oh = (int)lround(fmin(dh * scale, c->box_h));
  if (*ow < 1)
    *ow = 1;
  if (*oh < 1)
    *oh = 1;
}

void *qs_open(const char *path, int32_t box_w, int32_t box_h, char *err,
              int32_t err_len) {
  /* Errors only (FFmpeg logs to stderr); the same level quillvideo sets. */
  av_log_set_level(AV_LOG_ERROR);
  if (!path || box_w < 1 || box_h < 1 || box_w > QS_MAX_SIDE ||
      box_h > QS_MAX_SIDE) {
    qs_set_err(err, err_len, "invalid arguments", 0);
    return NULL;
  }
  QsCtx *c = calloc(1, sizeof *c);
  if (!c) {
    qs_set_err(err, err_len, "out of memory", 0);
    return NULL;
  }
  c->vidx = -1;
  c->box_w = box_w;
  c->box_h = box_h;
  c->sws_fmt = -1;

  /* Local files only: no network or other protocols, ever. */
  AVDictionary *opts = NULL;
  av_dict_set(&opts, "protocol_whitelist", "file", 0);
  int r = avformat_open_input(&c->fmt, path, NULL, &opts);
  av_dict_free(&opts);
  if (r < 0) {
    qs_set_err(err, err_len, "can't open the file", r);
    free(c);
    return NULL;
  }
  r = avformat_find_stream_info(c->fmt, NULL);
  if (r < 0) {
    qs_set_err(err, err_len, "can't read the file", r);
    qs_close(c);
    return NULL;
  }
  c->start = c->fmt->start_time != AV_NOPTS_VALUE
                 ? (double)c->fmt->start_time / AV_TIME_BASE
                 : 0.0;
  int v = av_find_best_stream(c->fmt, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
  if (v < 0) {
    qs_set_err(err, err_len, "the file has no picture", v);
    qs_close(c);
    return NULL;
  }
  c->vidx = v;
  for (unsigned i = 0; i < c->fmt->nb_streams; i++) {
    if ((int)i != c->vidx)
      c->fmt->streams[i]->discard = AVDISCARD_ALL;
  }
  AVStream *st = c->fmt->streams[c->vidx];
  enum AVCodecID id = st->codecpar->codec_id;
  if (id != AV_CODEC_ID_VP9 && id != AV_CODEC_ID_VP8) {
    qs_set_err(err, err_len, "not a VP9 or VP8 sticker", 0);
    qs_close(c);
    return NULL;
  }
  int w = st->codecpar->width, h = st->codecpar->height;
  if (w < 1 || h < 1 || w > QS_MAX_SIDE || h > QS_MAX_SIDE) {
    qs_set_err(err, err_len, "unsupported sticker size", 0);
    qs_close(c);
    return NULL;
  }
  int code = 0;
  c->dec = qs_open_decoder(st, &code);
  if (!c->dec) {
    qs_set_err(err, err_len, "can't decode the sticker", code);
    qs_close(c);
    return NULL;
  }
  c->pkt = av_packet_alloc();
  c->apkt = av_packet_alloc();
  c->frame = av_frame_alloc();
  c->aframe = av_frame_alloc();
  c->atmp = av_frame_alloc();
  if (!c->pkt || !c->apkt || !c->frame || !c->aframe || !c->atmp) {
    qs_set_err(err, err_len, "out of memory", 0);
    qs_close(c);
    return NULL;
  }
  int ow, oh;
  qs_output_size(c, w, h, st->codecpar->sample_aspect_ratio, &ow, &oh);
  c->info.width = ow;
  c->info.height = oh;
  c->info.has_alpha = av_dict_get(st->metadata, "alpha_mode", NULL, 0) != NULL;
  AVRational rate = av_guess_frame_rate(c->fmt, st, NULL);
  c->info.frame_rate = rate.num > 0 && rate.den > 0 ? av_q2d(rate) : 0.0;
  if (c->fmt->duration != AV_NOPTS_VALUE && c->fmt->duration > 0)
    c->info.duration = (double)c->fmt->duration / AV_TIME_BASE;
  else if (st->duration != AV_NOPTS_VALUE && st->duration > 0)
    c->info.duration = st->duration * av_q2d(st->time_base);
  return c;
}

void qs_info(void *ctx, QsInfo *out) {
  if (ctx && out)
    *out = ((QsCtx *)ctx)->info;
}

/* Feed the block's alpha side stream, if it has one, to the alpha decoder.
 * A missing or broken alpha block leaves that picture opaque. */
static void qs_send_alpha(QsCtx *c, const AVPacket *pkt) {
  size_t size = 0;
  const uint8_t *side =
      av_packet_get_side_data(pkt, AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL, &size);
  if (!side || size <= 8 || size - 8 > INT_MAX)
    return;
  if (AV_RB64(side) != QS_ALPHA_ID)
    return;
  if (!c->adec) {
    int code = 0;
    c->adec = qs_open_decoder(c->fmt->streams[c->vidx], &code);
    if (!c->adec)
      return;
  }
  av_packet_unref(c->apkt);
  if (av_new_packet(c->apkt, (int)(size - 8)) < 0)
    return;
  memcpy(c->apkt->data, side + 8, size - 8);
  c->apkt->pts = pkt->pts;
  c->apkt->dts = pkt->dts;
  c->apkt->flags = pkt->flags;
  avcodec_send_packet(c->adec, c->apkt);
  av_packet_unref(c->apkt);
}

/* The newest alpha picture the alpha decoder has ready, into `aframe`. */
static void qs_take_alpha(QsCtx *c) {
  c->have_alpha = 0;
  if (!c->adec)
    return;
  while (avcodec_receive_frame(c->adec, c->atmp) == 0) {
    av_frame_unref(c->aframe);
    av_frame_move_ref(c->aframe, c->atmp);
    c->have_alpha = 1;
  }
}

int32_t qs_next(void *ctx, double *pts) {
  QsCtx *c = ctx;
  if (!c || !pts)
    return AVERROR(EINVAL);
  c->have_frame = 0;
  for (;;) {
    int r = avcodec_receive_frame(c->dec, c->frame);
    if (r == 0) {
      qs_take_alpha(c);
      AVStream *st = c->fmt->streams[c->vidx];
      int64_t ts = c->frame->best_effort_timestamp;
      if (ts == AV_NOPTS_VALUE)
        ts = c->frame->pts;
      double t = ts != AV_NOPTS_VALUE ? ts * av_q2d(st->time_base) - c->start
                                      : c->next_pts;
      if (!isfinite(t))
        t = c->next_pts;
      double dur = c->frame->duration > 0
                       ? c->frame->duration * av_q2d(st->time_base)
                       : (c->info.frame_rate > 0 ? 1.0 / c->info.frame_rate
                                                 : 1.0 / 30.0);
      c->cur_pts = t;
      c->next_pts = t + dur;
      c->have_frame = 1;
      *pts = t;
      return 1;
    }
    if (r == AVERROR_EOF)
      return 0;
    if (r != AVERROR(EAGAIN))
      return r;
    if (c->eof_sent)
      return 0;
    r = av_read_frame(c->fmt, c->pkt);
    if (r == AVERROR(EAGAIN))
      continue;
    if (r < 0) {
      /* The end, or a truncated file: flush what the decoders hold. */
      avcodec_send_packet(c->dec, NULL);
      if (c->adec)
        avcodec_send_packet(c->adec, NULL);
      c->eof_sent = 1;
      continue;
    }
    if (c->pkt->stream_index == c->vidx) {
      /* A corrupt packet is dropped; decoding carries on with the next. */
      if (avcodec_send_packet(c->dec, c->pkt) == 0)
        qs_send_alpha(c, c->pkt);
    }
    av_packet_unref(c->pkt);
  }
}

static int qs_colorspace(const AVFrame *f) {
  switch (f->colorspace) {
  case AVCOL_SPC_BT709:
    return SWS_CS_ITU709;
  case AVCOL_SPC_BT2020_NCL:
  case AVCOL_SPC_BT2020_CL:
    return SWS_CS_BT2020;
  default:
    /* Stickers are SD-sized: untagged means BT.601. */
    return f->height >= 720 ? SWS_CS_ITU709 : SWS_CS_ITU601;
  }
}

/* The alpha picture can stand in as the A plane of `f`: same size, 8-bit
 * luma, and `f` is 8-bit 4:2:0 (the only layout stickers use). */
static int qs_alpha_fits(const QsCtx *c, const AVFrame *f) {
  if (!c->have_alpha || f->format != AV_PIX_FMT_YUV420P)
    return 0;
  const AVFrame *a = c->aframe;
  if (a->width != f->width || a->height != f->height || !a->data[0])
    return 0;
  const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(a->format);
  return desc && !(desc->flags & AV_PIX_FMT_FLAG_RGB) &&
         desc->comp[0].depth == 8 && desc->comp[0].plane == 0 &&
         desc->comp[0].step == 1;
}

int32_t qs_pixels(void *ctx, QsFrame *out) {
  QsCtx *c = ctx;
  if (!c || !out || !c->have_frame)
    return AVERROR(EINVAL);
  AVFrame *f = c->frame;
  if (f->width < 1 || f->height < 1 || f->width > QS_MAX_SIDE ||
      f->height > QS_MAX_SIDE)
    return AVERROR_INVALIDDATA;
  const uint8_t *src[4] = {f->data[0], f->data[1], f->data[2], f->data[3]};
  int src_stride[4] = {f->linesize[0], f->linesize[1], f->linesize[2],
                       f->linesize[3]};
  int format = f->format;
  if (qs_alpha_fits(c, f)) {
    format = AV_PIX_FMT_YUVA420P;
    src[3] = c->aframe->data[0];
    src_stride[3] = c->aframe->linesize[0];
  }
  int ow, oh;
  qs_output_size(c, f->width, f->height, f->sample_aspect_ratio, &ow, &oh);
  int cs = qs_colorspace(f);
  int range = f->color_range == AVCOL_RANGE_JPEG;
  if (!c->sws || c->sws_w != f->width || c->sws_h != f->height ||
      c->sws_fmt != format || c->sws_ow != ow || c->sws_oh != oh ||
      c->sws_cs != cs || c->sws_range != range) {
    sws_freeContext(c->sws);
    /* Bicubic, as ffmpeg's scale filter (the subprocess path) uses. */
    c->sws = sws_getContext(f->width, f->height, (enum AVPixelFormat)format,
                            ow, oh, AV_PIX_FMT_BGRA,
                            qv_bgra_sws_flags(SWS_BICUBIC, ow), NULL, NULL,
                            NULL);
    if (!c->sws)
      return AVERROR(EINVAL);
    sws_setColorspaceDetails(c->sws, sws_getCoefficients(cs), range,
                             sws_getCoefficients(SWS_CS_DEFAULT), 1, 0,
                             1 << 16, 1 << 16);
    c->sws_w = f->width;
    c->sws_h = f->height;
    c->sws_fmt = format;
    c->sws_ow = ow;
    c->sws_oh = oh;
    c->sws_cs = cs;
    c->sws_range = range;
  }
  int stride = qv_bgra_stride(ow);
  size_t need = qv_bgra_size(ow, oh);
  if (need > c->rgb_size) {
    av_free(c->rgb);
    c->rgb = av_malloc(need);
    c->rgb_size = c->rgb ? need : 0;
    if (!c->rgb)
      return AVERROR(ENOMEM);
  }
  uint8_t *dst[4] = {c->rgb, NULL, NULL, NULL};
  int dst_stride[4] = {stride, 0, 0, 0};
  int rows = sws_scale(c->sws, src, src_stride, 0, f->height, dst, dst_stride);
  if (rows <= 0)
    return AVERROR(EINVAL);
  out->pts = c->cur_pts;
  out->data = c->rgb;
  out->stride = stride;
  out->width = ow;
  out->height = oh;
  return 0;
}

void qs_close(void *ctx) {
  QsCtx *c = ctx;
  if (!c)
    return;
  avcodec_free_context(&c->dec);
  avcodec_free_context(&c->adec);
  avformat_close_input(&c->fmt);
  av_packet_free(&c->pkt);
  av_packet_free(&c->apkt);
  av_frame_free(&c->frame);
  av_frame_free(&c->aframe);
  av_frame_free(&c->atmp);
  sws_freeContext(c->sws);
  av_free(c->rgb);
  free(c);
}

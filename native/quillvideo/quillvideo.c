/*
 * quillvideo: see quillvideo.h. Demuxes one local file with libavformat,
 * decodes its best video and audio streams with libavcodec, converts
 * pictures to BGRA (libswscale, scaled into the caller's box) or, when asked
 * and the picture allows, hands them over as NV12 without a colour
 * conversion, and sound to interleaved f32 (libswresample). With QV_HW_ALLOWED
 * the video decodes on the GPU where it can (tdesktop's platform order) and
 * pictures are copied back to memory. Quill runs one
 * context per player on a background thread and does all queueing, timing
 * and A/V sync itself.
 */
#include "quillvideo.h"
#include "bgra_layout.h"

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/display.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libavutil/time.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Quill's limits. tdesktop's kMaxQueuedPackets is a flush batch, not a limit. */
#define HELD_MAX_PACKETS 1024
#define HELD_MAX_BYTES (64 * 1024 * 1024)
/* tdesktop's kSkipInvalidDataPackets. */
#define SKIP_INVALID_PACKETS 10

typedef struct Ctx {
  AVFormatContext *fmt;
  AVCodecContext *vdec;
  AVCodecContext *adec;
  int vidx;
  int aidx;
  AVPacket *pkt;
  AVFrame *frame;
  int threads;
  enum AVPixelFormat hw_fmt;
  AVFrame *sw_frame;
  int stand_in_pictures_left;
  int stand_in_reopen_fails;
  /* A test stand-in for a hardware decoder: keeps QvInfo.hw set even where
   * no GPU decodes, so the fallback paths test the same on every machine. */
  int stand_in;
  /* File start time (seconds): pts are reported relative to it. */
  double start;
  int max_w;
  int max_h;
  int read_done;
  int vflushed;
  int vdone;
  int adone;
  /* Seek in progress: drop items that end before these times (< 0: none). */
  double vskip;
  double askip;
  int keep_skipped;
  AVFrame *skipped;
  int bad_packets_in_a_row;
  AVPacket *held[HELD_MAX_PACKETS];
  int held_first;
  int held_count;
  size_t held_bytes;
  /* Where the next frame starts when a frame carries no timestamp. */
  double vnext;
  double anext;
  /* Picture conversion. */
  struct SwsContext *sws;
  int sws_w, sws_h, sws_fmt, sws_ow, sws_oh, sws_cs, sws_range;
  uint8_t *rgb;
  size_t rgb_size;
  /* The layout asked for (qv_set_output). */
  int out_fmt;
  /* NV12 pictures: a scaler for those that need resizing (no colour
   * conversion), and the tightly packed output (luma, then chroma). */
  struct SwsContext *nsws;
  int nsws_w, nsws_h, nsws_fmt, nsws_ow, nsws_oh, nsws_cs, nsws_range;
  uint8_t *nv12;
  size_t nv12_size;
  /* Sound conversion. */
  SwrContext *swr;
  int swr_rate, swr_fmt, swr_channels;
  int out_channels;
  float *pcm;
  size_t pcm_cap; /* floats */
  QvInfo info;
} Ctx;

int32_t qv_abi_version(void) { return QV_ABI_VERSION; }

static void set_err(char *err, int32_t len, const char *what, int code) {
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

/* Clockwise degrees (0/90/180/270) from the stream's display matrix. */
static int stream_rotation(const AVStream *st) {
  const AVPacketSideData *sd =
      av_packet_side_data_get(st->codecpar->coded_side_data,
                              st->codecpar->nb_coded_side_data,
                              AV_PKT_DATA_DISPLAYMATRIX);
  if (!sd || sd->size < 9 * 4)
    return 0;
  double theta = -av_display_rotation_get((const int32_t *)sd->data);
  if (isnan(theta))
    return 0;
  theta -= 360.0 * floor(theta / 360.0 + 0.9 / 360.0);
  int quarter = (int)lround(theta / 90.0) % 4;
  return quarter * 90;
}

/* tdesktop's order (ffmpeg_utility.cpp GetHwFormat). */
static const enum AVHWDeviceType HW_ORDER[] = {
#if defined(_WIN32)
    AV_HWDEVICE_TYPE_D3D11VA,
    AV_HWDEVICE_TYPE_DXVA2,
#elif defined(__APPLE__)
    AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
#else
    AV_HWDEVICE_TYPE_VAAPI,
    AV_HWDEVICE_TYPE_VDPAU,
#endif
};

static enum AVPixelFormat hw_format(const AVCodec *codec,
                                    enum AVHWDeviceType type) {
  for (int i = 0;; i++) {
    const AVCodecHWConfig *config = avcodec_get_hw_config(codec, i);
    if (!config)
      return AV_PIX_FMT_NONE;
    if ((config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) &&
        config->device_type == type)
      return config->pix_fmt;
  }
}

static enum AVPixelFormat pick_format(AVCodecContext *dec,
                                      const enum AVPixelFormat *formats) {
  const Ctx *c = dec->opaque;
  enum AVPixelFormat software = AV_PIX_FMT_NONE;
  for (const enum AVPixelFormat *p = formats; *p != AV_PIX_FMT_NONE; p++) {
    if (*p == c->hw_fmt)
      return *p;
    const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(*p);
    if (desc && !(desc->flags & AV_PIX_FMT_FLAG_HWACCEL))
      software = *p;
  }
  return software;
}

static void attach_hw(Ctx *c, AVCodecContext *dec, const AVCodec *codec) {
  for (size_t i = 0; i < sizeof HW_ORDER / sizeof HW_ORDER[0]; i++) {
    enum AVPixelFormat format = hw_format(codec, HW_ORDER[i]);
    if (format == AV_PIX_FMT_NONE)
      continue;
    AVBufferRef *device = NULL;
    if (av_hwdevice_ctx_create(&device, HW_ORDER[i], NULL, NULL, 0) < 0)
      continue;
    dec->hw_device_ctx = device;
    dec->opaque = c;
    dec->get_format = pick_format;
    c->hw_fmt = format;
    return;
  }
}

static AVCodecContext *open_decoder(Ctx *c, AVStream *st, int threads, int hw,
                                    int *code) {
  const AVCodec *codec = NULL;
  /* VP8/VP9 with an alpha plane: only libvpx decodes it (when built in). */
  if (st->codecpar->codec_id == AV_CODEC_ID_VP9 &&
      av_dict_get(st->metadata, "alpha_mode", NULL, 0))
    codec = avcodec_find_decoder_by_name("libvpx-vp9");
  if (!codec)
    codec = avcodec_find_decoder(st->codecpar->codec_id);
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
  dec->thread_count = threads > 0 ? threads : 0;
  dec->thread_type = FF_THREAD_FRAME | FF_THREAD_SLICE;
  if (hw)
    attach_hw(c, dec, codec);
  *code = avcodec_open2(dec, codec, NULL);
  if (*code < 0) {
    avcodec_free_context(&dec);
    return NULL;
  }
  return dec;
}

/* The display size of a `w` x `h` picture with sample aspect `sar`, fit
 * (never enlarged) into the context's box. */
static void output_size(const Ctx *c, int w, int h, AVRational sar, int *ow,
                        int *oh) {
  double dw = w, dh = h;
  if (sar.num > 0 && sar.den > 0 && sar.num != sar.den)
    dw = dw * sar.num / sar.den;
  double scale = 1.0;
  if (c->max_w > 0 && dw > c->max_w)
    scale = fmin(scale, c->max_w / dw);
  if (c->max_h > 0 && dh > c->max_h)
    scale = fmin(scale, c->max_h / dh);
  *ow = (int)lround(dw * scale);
  *oh = (int)lround(dh * scale);
  if (*ow < 1)
    *ow = 1;
  if (*oh < 1)
    *oh = 1;
}

void *qv_open(const char *path, const QvOptions *options, char *err,
              int32_t err_len) {
  static int quiet = 0;
  if (!quiet) {
    av_log_set_level(AV_LOG_ERROR);
    quiet = 1;
  }
  if (!path || !options) {
    set_err(err, err_len, "invalid arguments", 0);
    return NULL;
  }
  Ctx *c = calloc(1, sizeof *c);
  if (!c) {
    set_err(err, err_len, "out of memory", 0);
    return NULL;
  }
  c->vidx = c->aidx = -1;
  c->vskip = c->askip = -1.0;
  c->max_w = options->max_width;
  c->max_h = options->max_height;
  c->sws_fmt = -1;
  c->nsws_fmt = -1;
  c->swr_fmt = -1;
  c->out_fmt = QV_PIXEL_BGRA;
  c->threads = options->threads;
  c->hw_fmt = AV_PIX_FMT_NONE;

  /* Local files only: no network or other protocols, ever. */
  AVDictionary *opts = NULL;
  av_dict_set(&opts, "protocol_whitelist", "file", 0);
  int r = avformat_open_input(&c->fmt, path, NULL, &opts);
  av_dict_free(&opts);
  if (r < 0) {
    set_err(err, err_len, "can't open the file", r);
    free(c);
    return NULL;
  }
  r = avformat_find_stream_info(c->fmt, NULL);
  if (r < 0) {
    set_err(err, err_len, "can't read the file", r);
    qv_close(c);
    return NULL;
  }
  c->start = c->fmt->start_time != AV_NOPTS_VALUE
                 ? (double)c->fmt->start_time / AV_TIME_BASE
                 : 0.0;

  int v = av_find_best_stream(c->fmt, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
  if (v >= 0 && !(c->fmt->streams[v]->disposition & AV_DISPOSITION_ATTACHED_PIC))
    c->vidx = v;
  if (options->want_audio) {
    int a = av_find_best_stream(c->fmt, AVMEDIA_TYPE_AUDIO, -1, c->vidx, NULL, 0);
    if (a >= 0)
      c->aidx = a;
  }
  for (unsigned i = 0; i < c->fmt->nb_streams; i++) {
    if ((int)i != c->vidx && (int)i != c->aidx)
      c->fmt->streams[i]->discard = AVDISCARD_ALL;
  }

  int code = 0;
  if (c->vidx >= 0) {
    AVStream *st = c->fmt->streams[c->vidx];
    int hw = options->hw != QV_HW_OFF;
    c->vdec = open_decoder(c, st, options->threads, hw, &code);
    if (!c->vdec && hw) {
      c->hw_fmt = AV_PIX_FMT_NONE;
      c->vdec = open_decoder(c, st, options->threads, 0, &code);
    }
    if (!c->vdec) {
      set_err(err, err_len, "can't decode the video", code);
      qv_close(c);
      return NULL;
    }
    int ow, oh;
    output_size(c, st->codecpar->width, st->codecpar->height,
                st->codecpar->sample_aspect_ratio, &ow, &oh);
    c->info.width = ow;
    c->info.height = oh;
    c->info.rotation = stream_rotation(st);
    AVRational rate = av_guess_frame_rate(c->fmt, st, NULL);
    c->info.frame_rate = rate.num > 0 && rate.den > 0 ? av_q2d(rate) : 0.0;
    c->info.has_video = 1;
    c->info.hw = c->vdec->hw_device_ctx != NULL;
    if (options->hw == QV_HW_FAIL_THIRD_PICTURE ||
        options->hw == QV_HW_FAIL_THIRD_PICTURE_AND_REOPEN) {
      c->stand_in = 1;
      c->stand_in_pictures_left = 3;
      c->stand_in_reopen_fails =
          options->hw == QV_HW_FAIL_THIRD_PICTURE_AND_REOPEN;
      c->info.hw = 1;
    }
    snprintf(c->info.video_codec, sizeof c->info.video_codec, "%s",
             avcodec_get_name(st->codecpar->codec_id));
  }
  if (c->aidx >= 0) {
    AVStream *st = c->fmt->streams[c->aidx];
    c->adec = open_decoder(c, st, 1, 0, &code);
    if (c->adec) {
      int channels = st->codecpar->ch_layout.nb_channels;
      c->out_channels = channels == 1 ? 1 : 2;
      c->info.has_audio = 1;
      c->info.sample_rate = st->codecpar->sample_rate;
      c->info.channels = c->out_channels;
      snprintf(c->info.audio_codec, sizeof c->info.audio_codec, "%s",
               avcodec_get_name(st->codecpar->codec_id));
    } else {
      /* A soundtrack we can't decode: play the picture silently. */
      st->discard = AVDISCARD_ALL;
      c->aidx = -1;
    }
  }
  if (!c->vdec && !c->adec) {
    set_err(err, err_len, "the file has no playable streams", 0);
    qv_close(c);
    return NULL;
  }
  if (c->fmt->duration != AV_NOPTS_VALUE && c->fmt->duration > 0) {
    c->info.duration = (double)c->fmt->duration / AV_TIME_BASE;
  } else if (c->vidx >= 0 &&
             c->fmt->streams[c->vidx]->duration != AV_NOPTS_VALUE) {
    AVStream *st = c->fmt->streams[c->vidx];
    c->info.duration = st->duration * av_q2d(st->time_base);
  }
  c->pkt = av_packet_alloc();
  c->frame = av_frame_alloc();
  if (!c->pkt || !c->frame) {
    set_err(err, err_len, "out of memory", 0);
    qv_close(c);
    return NULL;
  }
  return c;
}

void qv_info(void *ctx, QvInfo *out) {
  if (ctx && out)
    *out = ((Ctx *)ctx)->info;
}

int32_t qv_set_output(void *ctx, int32_t format) {
  Ctx *c = ctx;
  if (!c || (format != QV_PIXEL_BGRA && format != QV_PIXEL_NV12))
    return AVERROR(EINVAL);
  c->out_fmt = format;
  return 0;
}

static double frame_duration(const Ctx *c, const AVStream *st,
                             const AVFrame *f) {
  if (f->duration > 0)
    return f->duration * av_q2d(st->time_base);
  if (c->info.frame_rate > 0)
    return 1.0 / c->info.frame_rate;
  return 1.0 / 30.0;
}

static double frame_pts(const Ctx *c, const AVStream *st, const AVFrame *f,
                        double fallback) {
  int64_t ts = f->best_effort_timestamp;
  if (ts == AV_NOPTS_VALUE)
    ts = f->pts;
  if (ts == AV_NOPTS_VALUE)
    return fallback;
  return ts * av_q2d(st->time_base) - c->start;
}

static int colorspace_of(const AVFrame *f) {
  switch (f->colorspace) {
  case AVCOL_SPC_BT709:
    return SWS_CS_ITU709;
  case AVCOL_SPC_BT2020_NCL:
  case AVCOL_SPC_BT2020_CL:
    return SWS_CS_BT2020;
  case AVCOL_SPC_SMPTE170M:
  case AVCOL_SPC_BT470BG:
    return SWS_CS_ITU601;
  default:
    /* Untagged: HD is almost always BT.709, SD BT.601 (as most players). */
    return f->height >= 720 ? SWS_CS_ITU709 : SWS_CS_ITU601;
  }
}

static int full_range(const AVFrame *f) {
  return f->color_range == AVCOL_RANGE_JPEG || f->format == AV_PIX_FMT_YUVJ420P ||
         f->format == AV_PIX_FMT_YUVJ422P || f->format == AV_PIX_FMT_YUVJ444P;
}

static int32_t elapsed_us(int64_t since) {
  int64_t us = av_gettime_relative() - since;
  if (us < 0)
    return 0;
  return us > INT32_MAX ? INT32_MAX : (int32_t)us;
}

/* Whether the picture can be handed over as NV12 at `ow` x `oh`: 8-bit
 * 4:2:0 without alpha, a BT.601 or BT.709 matrix, even sides. */
static int nv12_eligible(const AVFrame *f, int cs, int ow, int oh) {
  if (f->format != AV_PIX_FMT_YUV420P && f->format != AV_PIX_FMT_YUVJ420P &&
      f->format != AV_PIX_FMT_NV12)
    return 0;
  if (cs != SWS_CS_ITU601 && cs != SWS_CS_ITU709)
    return 0;
  return ow > 0 && oh > 0 && ow % 2 == 0 && oh % 2 == 0 && f->width % 2 == 0 &&
         f->height % 2 == 0;
}

/* Copy the decoded planes into the packed NV12 buffer as they are
 * (interleaving Cb and Cr for yuv420p). 0 when the planes are laid out in
 * a way this doesn't copy (the caller scales instead). */
static int copy_nv12(const AVFrame *f, uint8_t *dst, int w, int h) {
  if (!f->data[0] || f->linesize[0] < w)
    return 0;
  int cw = w / 2, ch = h / 2;
  uint8_t *uv = dst + (size_t)w * (size_t)h;
  if (f->format == AV_PIX_FMT_NV12) {
    if (!f->data[1] || f->linesize[1] < w)
      return 0;
  } else if (!f->data[1] || !f->data[2] || f->linesize[1] < cw ||
             f->linesize[2] < cw) {
    return 0;
  }
  for (int y = 0; y < h; y++)
    memcpy(dst + (size_t)y * (size_t)w,
           f->data[0] + (size_t)y * (size_t)f->linesize[0], (size_t)w);
  for (int y = 0; y < ch; y++) {
    uint8_t *row = uv + (size_t)y * (size_t)w;
    if (f->format == AV_PIX_FMT_NV12) {
      memcpy(row, f->data[1] + (size_t)y * (size_t)f->linesize[1], (size_t)w);
      continue;
    }
    const uint8_t *cb = f->data[1] + (size_t)y * (size_t)f->linesize[1];
    const uint8_t *cr = f->data[2] + (size_t)y * (size_t)f->linesize[2];
    for (int x = 0; x < cw; x++) {
      row[2 * x] = cb[x];
      row[2 * x + 1] = cr[x];
    }
  }
  return 1;
}

/* Hand the picture over as NV12 (scaled into `ow` x `oh` when it must be,
 * with no colour conversion either way): 1 done, 0 not possible (the caller
 * converts to BGRA), < 0 error. */
static int emit_nv12(Ctx *c, const AVFrame *f, int ow, int oh, int cs,
                     int range, QvItem *out) {
  int64_t started = av_gettime_relative();
  size_t need = (size_t)ow * (size_t)oh * 3 / 2;
  if (need > c->nv12_size) {
    av_free(c->nv12);
    c->nv12 = av_malloc(need);
    c->nv12_size = c->nv12 ? need : 0;
    if (!c->nv12)
      return AVERROR(ENOMEM);
  }
  int copied = ow == f->width && oh == f->height && copy_nv12(f, c->nv12, ow, oh);
  if (!copied) {
    if (!c->nsws || c->nsws_w != f->width || c->nsws_h != f->height ||
        c->nsws_fmt != f->format || c->nsws_ow != ow || c->nsws_oh != oh ||
        c->nsws_cs != cs || c->nsws_range != range) {
      sws_freeContext(c->nsws);
      /* No SWS_ACCURATE_RND: it selects swscale's slower exact-rounding
       * scalers, and tdesktop passes no such flag either. */
      c->nsws = sws_getContext(f->width, f->height,
                               (enum AVPixelFormat)f->format, ow, oh,
                               AV_PIX_FMT_NV12, SWS_BILINEAR, NULL, NULL,
                               NULL);
      c->nsws_fmt = -1;
      if (!c->nsws)
        return 0;
      /* Same matrix and range in and out: a resize, not a conversion. */
      sws_setColorspaceDetails(c->nsws, sws_getCoefficients(cs), range,
                               sws_getCoefficients(cs), range, 0, 1 << 16,
                               1 << 16);
      c->nsws_w = f->width;
      c->nsws_h = f->height;
      c->nsws_fmt = f->format;
      c->nsws_ow = ow;
      c->nsws_oh = oh;
      c->nsws_cs = cs;
      c->nsws_range = range;
    }
    uint8_t *dst[4] = {c->nv12, c->nv12 + (size_t)ow * (size_t)oh, NULL, NULL};
    int dst_stride[4] = {ow, ow, 0, 0};
    int rows = sws_scale(c->nsws, (const uint8_t *const *)f->data, f->linesize,
                         0, f->height, dst, dst_stride);
    if (rows <= 0)
      return 0;
  }
  out->data = c->nv12;
  out->stride = ow;
  out->width = ow;
  out->height = oh;
  out->format = QV_PIXEL_NV12;
  out->chroma = c->nv12 + (size_t)ow * (size_t)oh;
  out->chroma_stride = ow;
  out->matrix = cs == SWS_CS_ITU709 ? QV_MATRIX_BT709 : QV_MATRIX_BT601;
  out->full_range = range ? 1 : 0;
  out->convert_us = elapsed_us(started);
  return 1;
}

static void save_skipped(Ctx *c, const AVFrame *f) {
  if (!c->skipped && !(c->skipped = av_frame_alloc()))
    return;
  av_frame_unref(c->skipped);
  av_frame_ref(c->skipped, f);
}

/* Convert the decoded picture; 0 when it is skipped (seeking past it). */
static int emit_video(Ctx *c, const AVFrame *f, QvItem *out) {
  AVStream *st = c->fmt->streams[c->vidx];
  double dur = frame_duration(c, st, f);
  double pts = frame_pts(c, st, f, c->vnext);
  c->vnext = pts + dur;
  if (c->vskip >= 0) {
    if (pts + dur <= c->vskip + 1e-6) {
      if (c->keep_skipped)
        save_skipped(c, f);
      return 0;
    }
    c->vskip = -1.0;
    av_frame_unref(c->skipped);
  }
  if (f->width <= 0 || f->height <= 0)
    return 0;
  int ow, oh;
  output_size(c, f->width, f->height, f->sample_aspect_ratio, &ow, &oh);
  int cs = colorspace_of(f);
  int range = full_range(f);
  out->kind = QV_ITEM_VIDEO;
  out->pts = pts;
  out->samples = 0;
  if (c->out_fmt == QV_PIXEL_NV12 && nv12_eligible(f, cs, ow, oh)) {
    int r = emit_nv12(c, f, ow, oh, cs, range, out);
    if (r != 0)
      return r;
  }
  int64_t started = av_gettime_relative();
  if (!c->sws || c->sws_w != f->width || c->sws_h != f->height ||
      c->sws_fmt != f->format || c->sws_ow != ow || c->sws_oh != oh ||
      c->sws_cs != cs || c->sws_range != range) {
    sws_freeContext(c->sws);
    c->sws = sws_getContext(f->width, f->height, (enum AVPixelFormat)f->format,
                            ow, oh, AV_PIX_FMT_BGRA,
                            qv_bgra_sws_flags(SWS_BILINEAR, ow), NULL, NULL,
                            NULL);
    if (!c->sws)
      return AVERROR(EINVAL);
    /* Ignored (harmlessly fails) for RGB sources such as GIF palettes. */
    sws_setColorspaceDetails(c->sws, sws_getCoefficients(cs), range,
                             sws_getCoefficients(SWS_CS_DEFAULT), 1, 0,
                             1 << 16, 1 << 16);
    c->sws_w = f->width;
    c->sws_h = f->height;
    c->sws_fmt = f->format;
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
  int rows = sws_scale(c->sws, (const uint8_t *const *)f->data, f->linesize, 0,
                       f->height, dst, dst_stride);
  if (rows <= 0)
    return AVERROR(EINVAL);
  out->data = c->rgb;
  out->stride = stride;
  out->width = ow;
  out->height = oh;
  out->format = QV_PIXEL_BGRA;
  out->chroma = NULL;
  out->chroma_stride = 0;
  out->matrix = 0;
  out->full_range = 0;
  out->convert_us = elapsed_us(started);
  return 1;
}

/* Convert the decoded sound; 0 when it is skipped (seeking past it). */
static int emit_audio(Ctx *c, QvItem *out) {
  AVFrame *f = c->frame;
  AVStream *st = c->fmt->streams[c->aidx];
  int rate = f->sample_rate > 0 ? f->sample_rate : c->info.sample_rate;
  if (rate <= 0 || f->nb_samples <= 0)
    return 0;
  double pts = frame_pts(c, st, f, c->anext);
  c->anext = pts + (double)f->nb_samples / rate;
  int in_channels = f->ch_layout.nb_channels;
  /* The output rate is fixed at what qv_info reported (a mid-stream rate
   * change is resampled), so the caller's audio format never changes. */
  if (c->info.sample_rate <= 0)
    c->info.sample_rate = rate;
  int out_rate = c->info.sample_rate;
  if (!c->swr || c->swr_rate != rate || c->swr_fmt != f->format ||
      c->swr_channels != in_channels) {
    swr_free(&c->swr);
    AVChannelLayout in_layout = {0};
    if (f->ch_layout.order == AV_CHANNEL_ORDER_UNSPEC || in_channels <= 0)
      av_channel_layout_default(&in_layout, in_channels > 0 ? in_channels : 1);
    else
      av_channel_layout_copy(&in_layout, &f->ch_layout);
    AVChannelLayout out_layout = {0};
    av_channel_layout_default(&out_layout, c->out_channels);
    int r = swr_alloc_set_opts2(&c->swr, &out_layout, AV_SAMPLE_FMT_FLT, out_rate,
                                &in_layout, (enum AVSampleFormat)f->format,
                                rate, 0, NULL);
    av_channel_layout_uninit(&in_layout);
    av_channel_layout_uninit(&out_layout);
    if (r < 0 || swr_init(c->swr) < 0) {
      swr_free(&c->swr);
      return AVERROR(EINVAL);
    }
    c->swr_rate = rate;
    c->swr_fmt = f->format;
    c->swr_channels = in_channels;
  }
  int cap = swr_get_out_samples(c->swr, f->nb_samples);
  if (cap <= 0)
    return 0;
  size_t need = (size_t)cap * (size_t)c->out_channels;
  if (need > c->pcm_cap) {
    av_free(c->pcm);
    c->pcm = av_malloc(need * sizeof(float));
    c->pcm_cap = c->pcm ? need : 0;
    if (!c->pcm)
      return AVERROR(ENOMEM);
  }
  uint8_t *dst = (uint8_t *)c->pcm;
  int n = swr_convert(c->swr, &dst, cap, (const uint8_t **)f->extended_data,
                      f->nb_samples);
  if (n <= 0)
    return 0;
  if (c->askip >= 0) {
    double end = pts + (double)n / out_rate;
    if (end <= c->askip + 1e-6)
      return 0;
    if (pts < c->askip) {
      int drop = (int)((c->askip - pts) * out_rate);
      if (drop > 0 && drop < n) {
        memmove(c->pcm, c->pcm + (size_t)drop * c->out_channels,
                (size_t)(n - drop) * c->out_channels * sizeof(float));
        n -= drop;
        pts += (double)drop / out_rate;
      }
    }
    c->askip = -1.0;
  }
  out->kind = QV_ITEM_AUDIO;
  out->pts = pts;
  out->data = (const uint8_t *)c->pcm;
  out->stride = 0;
  out->width = 0;
  out->height = 0;
  out->samples = n;
  out->format = QV_PIXEL_BGRA;
  out->chroma = NULL;
  out->chroma_stride = 0;
  out->matrix = 0;
  out->full_range = 0;
  out->convert_us = 0;
  return 1;
}

static AVPacket *take_held(Ctx *c) {
  AVPacket *pkt = c->held[c->held_first];
  c->held[c->held_first] = NULL;
  c->held_first = (c->held_first + 1) % HELD_MAX_PACKETS;
  c->held_count--;
  c->held_bytes -= (size_t)pkt->size;
  return pkt;
}

static void drop_held(Ctx *c) {
  while (c->held_count > 0) {
    AVPacket *pkt = take_held(c);
    av_packet_free(&pkt);
  }
}

static int restart(Ctx *c, double target, double vskip, double askip) {
  int64_t ts = (int64_t)((target + c->start) * AV_TIME_BASE);
  int r = av_seek_frame(c->fmt, -1, ts, AVSEEK_FLAG_BACKWARD);
  if (r < 0)
    r = av_seek_frame(c->fmt, -1, ts, AVSEEK_FLAG_BACKWARD | AVSEEK_FLAG_ANY);
  if (r < 0)
    return r;
  if (c->vdec)
    avcodec_flush_buffers(c->vdec);
  if (c->adec)
    avcodec_flush_buffers(c->adec);
  if (c->swr) {
    /* Drop any samples the resampler held from before the seek. */
    swr_free(&c->swr);
  }
  drop_held(c);
  c->read_done = 0;
  c->vflushed = 0;
  c->vdone = 0;
  c->adone = 0;
  c->bad_packets_in_a_row = 0;
  av_frame_unref(c->skipped);
  c->vskip = c->vdec ? vskip : -1.0;
  c->askip = c->adec ? askip : -1.0;
  c->vnext = vskip;
  c->anext = askip;
  return 0;
}

static int fall_back_to_software(Ctx *c) {
  int code = AVERROR_DECODER_NOT_FOUND;
  AVCodecContext *dec =
      c->stand_in_reopen_fails
          ? NULL
          : open_decoder(c, c->fmt->streams[c->vidx], c->threads, 0, &code);
  if (!dec)
    return code;
  avcodec_free_context(&c->vdec);
  c->vdec = dec;
  c->hw_fmt = AV_PIX_FMT_NONE;
  c->stand_in = 0;
  c->stand_in_pictures_left = 0;
  c->info.hw = 0;
  double after_picture = c->vskip > c->vnext ? c->vskip : c->vnext;
  double after_sound = c->askip > c->anext ? c->askip : c->anext;
  double target =
      c->adec && after_sound < after_picture ? after_sound : after_picture;
  /* If this seek fails the software decoder stays: the error ends playback,
   * and the next qv_seek restarts it from a clean state. */
  return restart(c, target, after_picture, after_sound);
}

static int picture_in_memory(Ctx *c, const AVFrame **pic) {
  *pic = c->frame;
  if (c->stand_in_pictures_left > 0 && --c->stand_in_pictures_left == 0)
    return AVERROR_EXTERNAL;
  if (!c->frame->hw_frames_ctx) {
    if (!c->stand_in)
      c->info.hw = 0;
    return 0;
  }
  if (!c->sw_frame && !(c->sw_frame = av_frame_alloc()))
    return AVERROR(ENOMEM);
  av_frame_unref(c->sw_frame);
  int r = av_hwframe_transfer_data(c->sw_frame, c->frame, 0);
  if (r >= 0)
    r = av_frame_copy_props(c->sw_frame, c->frame);
  if (r < 0)
    return r;
  *pic = c->sw_frame;
  return 0;
}

static int emit_skipped(Ctx *c, QvItem *out) {
  if (c->vskip < 0 || !c->skipped || !c->skipped->buf[0])
    return 0;
  c->vskip = -1.0;
  int k = emit_video(c, c->skipped, out);
  av_frame_unref(c->skipped);
  return k;
}

/* Pull one frame from a decoder: 1 emitted, 0 none ready (or skipped), < 0
 * error. Marks the decoder done once it is drained after the end. */
static int drain(Ctx *c, int video, QvItem *out) {
  int *done = video ? &c->vdone : &c->adone;
  int flushed = video ? c->vflushed : c->read_done;
  for (;;) {
    int r = avcodec_receive_frame(video ? c->vdec : c->adec, c->frame);
    if (r == AVERROR(EAGAIN) && !flushed)
      return 0;
    if (r == AVERROR(EAGAIN) || r == AVERROR_EOF) {
      *done = 1;
      return video ? emit_skipped(c, out) : 0;
    }
    if (r < 0)
      return video && c->info.hw ? fall_back_to_software(c) : r;
    if (!video) {
      int k = emit_audio(c, out);
      av_frame_unref(c->frame);
      if (k != 0)
        return k;
      continue;
    }
    const AVFrame *pic = NULL;
    int k = picture_in_memory(c, &pic);
    if (k < 0) {
      av_frame_unref(c->frame);
      return c->info.hw ? fall_back_to_software(c) : k;
    }
    k = emit_video(c, pic, out);
    av_frame_unref(c->frame);
    av_frame_unref(c->sw_frame);
    if (k != 0)
      return k;
  }
}

static int send_video(Ctx *c, const AVPacket *pkt) {
  int sent = avcodec_send_packet(c->vdec, pkt);
  if (sent >= 0) {
    c->bad_packets_in_a_row = 0;
    return 0;
  }
  if (sent == AVERROR(EAGAIN))
    return 0;
  if (c->info.hw)
    return fall_back_to_software(c);
  if (sent == AVERROR_INVALIDDATA && ++c->bad_packets_in_a_row < SKIP_INVALID_PACKETS)
    return 0;
  return sent;
}

static void end_reading(Ctx *c) {
  c->read_done = 1;
  if (c->adec)
    avcodec_send_packet(c->adec, NULL);
}

int32_t qv_next(void *ctx, QvItem *out) {
  Ctx *c = ctx;
  if (!c || !out)
    return AVERROR(EINVAL);
  for (;;) {
    if (c->vdec && !c->vdone) {
      int r = drain(c, 1, out);
      if (r != 0)
        return r;
    }
    if (c->adec && !c->adone) {
      int r = drain(c, 0, out);
      if (r != 0)
        return r;
    }
    if (c->held_count > 0) {
      AVPacket *pkt = take_held(c);
      int r = send_video(c, pkt);
      av_packet_free(&pkt);
      if (r < 0)
        return r;
      continue;
    }
    if (c->read_done) {
      if (c->vdec && !c->vflushed) {
        avcodec_send_packet(c->vdec, NULL);
        c->vflushed = 1;
      } else if ((!c->vdec || c->vdone) && (!c->adec || c->adone)) {
        return 0;
      }
      continue;
    }
    int r = av_read_frame(c->fmt, c->pkt);
    if (r == AVERROR(EAGAIN))
      continue;
    if (r < 0) {
      end_reading(c);
      continue;
    }
    if (c->pkt->stream_index == c->vidx && c->vdec) {
      r = send_video(c, c->pkt);
      av_packet_unref(c->pkt);
      if (r < 0)
        return r;
      continue;
    }
    if (c->pkt->stream_index == c->aidx && c->adec)
      avcodec_send_packet(c->adec, c->pkt);
    av_packet_unref(c->pkt);
  }
}

int32_t qv_next_sound(void *ctx, QvItem *out) {
  Ctx *c = ctx;
  if (!c || !out)
    return AVERROR(EINVAL);
  for (;;) {
    if (!c->adec || c->adone)
      return 0;
    int r = drain(c, 0, out);
    if (r != 0)
      return r;
    if (c->read_done)
      continue;
    if (c->held_count >= HELD_MAX_PACKETS || c->held_bytes >= HELD_MAX_BYTES)
      return 0;
    r = av_read_frame(c->fmt, c->pkt);
    if (r == AVERROR(EAGAIN))
      continue;
    if (r < 0) {
      end_reading(c);
      continue;
    }
    if (c->pkt->stream_index == c->vidx && c->vdec) {
      AVPacket *pkt = av_packet_alloc();
      if (!pkt) {
        av_packet_unref(c->pkt);
        return AVERROR(ENOMEM);
      }
      av_packet_move_ref(pkt, c->pkt);
      c->held[(c->held_first + c->held_count) % HELD_MAX_PACKETS] = pkt;
      c->held_count++;
      c->held_bytes += (size_t)pkt->size;
      continue;
    }
    if (c->pkt->stream_index == c->aidx)
      avcodec_send_packet(c->adec, c->pkt);
    av_packet_unref(c->pkt);
  }
}

int32_t qv_seek(void *ctx, double seconds) {
  Ctx *c = ctx;
  if (!c)
    return AVERROR(EINVAL);
  if (seconds < 0)
    seconds = 0;
  c->keep_skipped = 1;
  return restart(c, seconds, seconds, seconds);
}

void qv_close(void *ctx) {
  Ctx *c = ctx;
  if (!c)
    return;
  avcodec_free_context(&c->vdec);
  avcodec_free_context(&c->adec);
  avformat_close_input(&c->fmt);
  drop_held(c);
  av_packet_free(&c->pkt);
  av_frame_free(&c->frame);
  av_frame_free(&c->sw_frame);
  av_frame_free(&c->skipped);
  sws_freeContext(c->sws);
  swr_free(&c->swr);
  av_free(c->rgb);
  sws_freeContext(c->nsws);
  av_free(c->nv12);
  av_free(c->pcm);
  free(c);
}

/*
 * quillvideo: see quillvideo.h. Demuxes one local file with libavformat,
 * decodes its best video and audio streams with libavcodec, converts
 * pictures to BGRA (libswscale, scaled into the caller's box) and sound to
 * interleaved f32 (libswresample). Quill runs one context per player on a
 * background thread and does all queueing, timing and A/V sync itself.
 */
#include "quillvideo.h"

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/display.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Ctx {
  AVFormatContext *fmt;
  AVCodecContext *vdec;
  AVCodecContext *adec;
  int vidx;
  int aidx;
  AVPacket *pkt;
  AVFrame *frame;
  /* File start time (seconds): pts are reported relative to it. */
  double start;
  int max_w;
  int max_h;
  /* Reading finished and both decoders were sent the flush packet. */
  int eof_sent;
  int vdone;
  int adone;
  /* Seek in progress: drop items that end before these times (< 0: none). */
  double vskip;
  double askip;
  /* Where the next frame starts when a frame carries no timestamp. */
  double vnext;
  double anext;
  /* Picture conversion. */
  struct SwsContext *sws;
  int sws_w, sws_h, sws_fmt, sws_ow, sws_oh, sws_cs, sws_range;
  uint8_t *rgb;
  size_t rgb_size;
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

static AVCodecContext *open_decoder(AVStream *st, int threads, int *code) {
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
  c->swr_fmt = -1;

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
    c->vdec = open_decoder(st, options->threads, &code);
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
    snprintf(c->info.video_codec, sizeof c->info.video_codec, "%s",
             avcodec_get_name(st->codecpar->codec_id));
  }
  if (c->aidx >= 0) {
    AVStream *st = c->fmt->streams[c->aidx];
    c->adec = open_decoder(st, 1, &code);
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

/* Convert the decoded picture; 0 when it is skipped (seeking past it). */
static int emit_video(Ctx *c, QvItem *out) {
  AVFrame *f = c->frame;
  AVStream *st = c->fmt->streams[c->vidx];
  double dur = frame_duration(c, st, f);
  double pts = frame_pts(c, st, f, c->vnext);
  c->vnext = pts + dur;
  if (c->vskip >= 0) {
    if (pts + dur <= c->vskip + 1e-6)
      return 0;
    c->vskip = -1.0;
  }
  if (f->width <= 0 || f->height <= 0)
    return 0;
  int ow, oh;
  output_size(c, f->width, f->height, f->sample_aspect_ratio, &ow, &oh);
  int cs = colorspace_of(f);
  int range = full_range(f);
  if (!c->sws || c->sws_w != f->width || c->sws_h != f->height ||
      c->sws_fmt != f->format || c->sws_ow != ow || c->sws_oh != oh ||
      c->sws_cs != cs || c->sws_range != range) {
    sws_freeContext(c->sws);
    c->sws = sws_getContext(f->width, f->height, (enum AVPixelFormat)f->format,
                            ow, oh, AV_PIX_FMT_BGRA,
                            SWS_BILINEAR | SWS_ACCURATE_RND, NULL, NULL, NULL);
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
  size_t need = (size_t)ow * (size_t)oh * 4;
  if (need > c->rgb_size) {
    av_free(c->rgb);
    c->rgb = av_malloc(need);
    c->rgb_size = c->rgb ? need : 0;
    if (!c->rgb)
      return AVERROR(ENOMEM);
  }
  uint8_t *dst[4] = {c->rgb, NULL, NULL, NULL};
  int dst_stride[4] = {ow * 4, 0, 0, 0};
  int rows = sws_scale(c->sws, (const uint8_t *const *)f->data, f->linesize, 0,
                       f->height, dst, dst_stride);
  if (rows <= 0)
    return AVERROR(EINVAL);
  out->kind = QV_ITEM_VIDEO;
  out->pts = pts;
  out->data = c->rgb;
  out->stride = ow * 4;
  out->width = ow;
  out->height = oh;
  out->samples = 0;
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
  return 1;
}

/* Pull one frame from a decoder: 1 emitted, 0 none ready (or skipped), < 0
 * error. Marks the decoder done once it is drained after the end. */
static int drain(Ctx *c, AVCodecContext *dec, int *done, int video,
                 QvItem *out) {
  for (;;) {
    int r = avcodec_receive_frame(dec, c->frame);
    if (r == AVERROR(EAGAIN)) {
      if (c->eof_sent)
        *done = 1;
      return 0;
    }
    if (r == AVERROR_EOF) {
      *done = 1;
      return 0;
    }
    if (r < 0)
      return r;
    int k = video ? emit_video(c, out) : emit_audio(c, out);
    av_frame_unref(c->frame);
    if (k != 0)
      return k;
  }
}

int32_t qv_next(void *ctx, QvItem *out) {
  Ctx *c = ctx;
  if (!c || !out)
    return AVERROR(EINVAL);
  for (;;) {
    if (c->vdec && !c->vdone) {
      int r = drain(c, c->vdec, &c->vdone, 1, out);
      if (r != 0)
        return r;
    }
    if (c->adec && !c->adone) {
      int r = drain(c, c->adec, &c->adone, 0, out);
      if (r != 0)
        return r;
    }
    if (c->eof_sent) {
      if ((!c->vdec || c->vdone) && (!c->adec || c->adone))
        return 0;
      continue;
    }
    int r = av_read_frame(c->fmt, c->pkt);
    if (r == AVERROR(EAGAIN))
      continue;
    if (r < 0) {
      /* The end, or a truncated file: flush what the decoders hold. */
      if (c->vdec)
        avcodec_send_packet(c->vdec, NULL);
      if (c->adec)
        avcodec_send_packet(c->adec, NULL);
      c->eof_sent = 1;
      continue;
    }
    /* A corrupt packet is dropped; decoding carries on with the next. */
    if (c->pkt->stream_index == c->vidx && c->vdec)
      avcodec_send_packet(c->vdec, c->pkt);
    else if (c->pkt->stream_index == c->aidx && c->adec)
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
  int64_t ts = (int64_t)((seconds + c->start) * AV_TIME_BASE);
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
  c->eof_sent = 0;
  c->vdone = 0;
  c->adone = 0;
  c->vskip = c->vdec ? seconds : -1.0;
  c->askip = c->adec ? seconds : -1.0;
  c->vnext = seconds;
  c->anext = seconds;
  return 0;
}

void qv_close(void *ctx) {
  Ctx *c = ctx;
  if (!c)
    return;
  avcodec_free_context(&c->vdec);
  avcodec_free_context(&c->adec);
  avformat_close_input(&c->fmt);
  av_packet_free(&c->pkt);
  av_frame_free(&c->frame);
  sws_freeContext(c->sws);
  swr_free(&c->swr);
  av_free(c->rgb);
  av_free(c->pcm);
  free(c);
}

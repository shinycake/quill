#include "video_frame.h"

#include "api/video/i420_buffer.h"
#include "rtc_base/time_utils.h"

namespace quill_calls {

void WithI420(const webrtc::VideoFrame &frame, const std::function<void(const qc_video_frame &)> &callback) {
	const auto buffer = frame.video_frame_buffer()->ToI420();
	if (!buffer) {
		return;
	}
	const auto result = qc_video_frame{
		.width = buffer->width(),
		.height = buffer->height(),
		.rotation = int32_t(frame.rotation()),
		.timestamp_us = frame.timestamp_us(),
		.y = buffer->DataY(),
		.u = buffer->DataU(),
		.v = buffer->DataV(),
		.stride_y = buffer->StrideY(),
		.stride_u = buffer->StrideU(),
		.stride_v = buffer->StrideV(),
	};
	callback(result);
}

webrtc::VideoFrame FrameFromI420(const qc_video_frame &frame) {
	const auto buffer = webrtc::I420Buffer::Copy(
		frame.width,
		frame.height,
		frame.y,
		frame.stride_y,
		frame.u,
		frame.stride_u,
		frame.v,
		frame.stride_v);
	const auto rotation = [&] {
		switch (frame.rotation) {
		case 90: return webrtc::kVideoRotation_90;
		case 180: return webrtc::kVideoRotation_180;
		case 270: return webrtc::kVideoRotation_270;
		default: return webrtc::kVideoRotation_0;
		}
	}();
	return webrtc::VideoFrame::Builder()
		.set_video_frame_buffer(buffer)
		.set_rotation(rotation)
		.set_timestamp_us(frame.timestamp_us ? frame.timestamp_us : rtc::TimeMicros())
		.build();
}

CallbackSink::CallbackSink(Callback callback)
: _callback(std::move(callback)) {
}

void CallbackSink::OnFrame(const webrtc::VideoFrame &frame) {
	WithI420(frame, _callback);
}

} // namespace quill_calls

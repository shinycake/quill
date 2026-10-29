// Conversions between webrtc::VideoFrame and the C API's qc_video_frame.
#pragma once

#include "quill_calls.h"

#include "api/video/video_frame.h"
#include "api/video/video_sink_interface.h"

#include <functional>

namespace quill_calls {

// Converts `frame` to I420 and hands it to `callback` for the call's duration.
void WithI420(const webrtc::VideoFrame &frame, const std::function<void(const qc_video_frame &)> &callback);

// Copies a C picture into a frame tgcalls can send.
webrtc::VideoFrame FrameFromI420(const qc_video_frame &frame);

// A sink that hands every picture, in I420, to `callback`.
class CallbackSink final : public rtc::VideoSinkInterface<webrtc::VideoFrame> {
public:
	using Callback = std::function<void(const qc_video_frame &)>;

	explicit CallbackSink(Callback callback);

	void OnFrame(const webrtc::VideoFrame &frame) override;

private:
	Callback _callback;
};

} // namespace quill_calls

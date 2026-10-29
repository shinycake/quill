// platform/darwin: capturers hand pictures to the DarwinVideoTrackSource
// behind the proxy (tgcalls platform/darwin/DarwinInterface.mm).
#include "platform.h"

#include "platform/darwin/DarwinVideoSource.h"

#include "pc/video_track_source_proxy.h"

namespace quill_calls {

void PushToSource(webrtc::VideoTrackSourceInterface *source, const webrtc::VideoFrame &frame) {
	const auto proxy = static_cast<webrtc::VideoTrackSourceProxy *>(source);
	const auto internal = static_cast<tgcalls::DarwinVideoTrackSource *>(proxy->internal());
	internal->OnCapturedFrame(frame);
}

} // namespace quill_calls

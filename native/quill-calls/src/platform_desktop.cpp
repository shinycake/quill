// platform/tdesktop: Telegram Desktop's capturers write into the source's
// broadcaster (tgcalls platform/tdesktop/VideoCapturerInterfaceImpl.cpp).
#include "platform.h"

#include "platform/tdesktop/VideoCapturerTrackSource.h"

#include "pc/video_track_source_proxy.h"

namespace quill_calls {

void PushToSource(webrtc::VideoTrackSourceInterface *source, const webrtc::VideoFrame &frame) {
	const auto proxy = static_cast<webrtc::VideoTrackSourceProxy *>(source);
	const auto internal = static_cast<tgcalls::VideoCapturerTrackSource *>(proxy->internal());
	internal->sink()->OnFrame(frame);
}

} // namespace quill_calls

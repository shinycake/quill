// Cameras as tgcalls' platform/tdesktop capturer enumerates them
// (platform/tdesktop/VideoCameraCapturer.cpp), so every id opens.
#include "video.h"

#include "modules/video_capture/video_capture_factory.h"

#ifdef WEBRTC_LINUX
#include "modules/video_capture/video_capture_options.h"
#endif // WEBRTC_LINUX

namespace quill_calls {

std::vector<std::pair<std::string, std::string>> CameraDevices() {
#ifdef WEBRTC_LINUX
	auto options = webrtc::VideoCaptureOptions();
	options.set_allow_v4l2(true);
	const auto info = std::unique_ptr<webrtc::VideoCaptureModule::DeviceInfo>(
		webrtc::VideoCaptureFactory::CreateDeviceInfo(&options));
#else // WEBRTC_LINUX
	const auto info = std::unique_ptr<webrtc::VideoCaptureModule::DeviceInfo>(
		webrtc::VideoCaptureFactory::CreateDeviceInfo());
#endif // WEBRTC_LINUX
	auto result = std::vector<std::pair<std::string, std::string>>();
	if (!info) {
		return result;
	}
	const auto count = info->NumberOfDevices();
	for (auto i = 0; i < int(count); ++i) {
		constexpr auto kLengthLimit = 256;
		char name[kLengthLimit] = { 0 };
		char id[kLengthLimit] = { 0 };
		if (info->GetDeviceName(i, name, kLengthLimit, id, kLengthLimit) == 0) {
			result.emplace_back(id, name);
		}
	}
	return result;
}

} // namespace quill_calls

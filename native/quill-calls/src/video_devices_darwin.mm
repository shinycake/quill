// Cameras as tgcalls' platform/darwin capturer picks them: by uniqueID from
// VideoCameraCapturer's own device list (VideoCapturerInterfaceImpl.mm).
#include "video.h"

#import "platform/darwin/VideoCameraCapturerMac.h"

#import <AVFoundation/AVFoundation.h>

namespace quill_calls {

std::vector<std::pair<std::string, std::string>> CameraDevices() {
	auto result = std::vector<std::pair<std::string, std::string>>();
	@autoreleasepool {
		for (AVCaptureDevice *device in [VideoCameraCapturer captureDevices]) {
			result.emplace_back(
				std::string(device.uniqueID.UTF8String ?: ""),
				std::string(device.localizedName.UTF8String ?: ""));
		}
	}
	return result;
}

} // namespace quill_calls

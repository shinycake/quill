#include "video.h"

#include "guard.h"
#include "platform.h"
#include "video_frame.h"

#include "StaticThreads.h"
#include "VideoCaptureInterface.h"
#include "third-party/json11.hpp"

#include <cstdlib>
#include <cstring>
#include <mutex>

struct qc_video_capture {
	std::shared_ptr<tgcalls::VideoCaptureInterface> capture;
	// Only for an external source.
	std::shared_ptr<quill_calls::ExternalFeed> feed;
	std::mutex previewMutex;
	std::shared_ptr<quill_calls::CallbackGuard> previewGuard;
};

namespace quill_calls {
namespace {

tgcalls::VideoState StateFromC(int32_t state) {
	switch (state) {
	case QC_VIDEO_ACTIVE: return tgcalls::VideoState::Active;
	case QC_VIDEO_PAUSED: return tgcalls::VideoState::Paused;
	default: return tgcalls::VideoState::Inactive;
	}
}

qc_video_capture *Wrap(std::unique_ptr<tgcalls::VideoCaptureInterface> capture, std::shared_ptr<ExternalFeed> feed) {
	if (!capture) {
		return nullptr;
	}
	auto result = new qc_video_capture();
	result->capture = std::move(capture);
	result->feed = std::move(feed);
	return result;
}

void ClosePreview(qc_video_capture *capture) {
	std::lock_guard lock(capture->previewMutex);
	if (capture->previewGuard) {
		capture->previewGuard->close();
		capture->previewGuard = nullptr;
	}
}

} // namespace

std::shared_ptr<tgcalls::VideoCaptureInterface> CaptureOf(qc_video_capture *capture) {
	return capture ? capture->capture : nullptr;
}

char *CopyForCaller(const std::string &value) {
	const auto result = static_cast<char *>(std::malloc(value.size() + 1));
	if (result) {
		std::memcpy(result, value.c_str(), value.size() + 1);
	}
	return result;
}

} // namespace quill_calls

extern "C" {

char *qc_video_input_devices(void) {
	auto list = json11::Json::array();
	for (const auto &[id, name] : quill_calls::CameraDevices()) {
		list.push_back(json11::Json::object{ { "id", id }, { "name", name } });
	}
	return quill_calls::CopyForCaller(json11::Json(list).dump());
}

qc_video_capture *qc_video_capture_camera(const char *device_id) {
	return quill_calls::Wrap(
		tgcalls::VideoCaptureInterface::Create(
			tgcalls::StaticThreads::getThreads(),
			device_id ? device_id : ""),
		nullptr);
}

qc_video_capture *qc_video_capture_external(int32_t is_screencast) {
	auto feed = quill_calls::ExternalFeed::Create();
	auto capture = tgcalls::VideoCaptureInterface::Create(
		tgcalls::StaticThreads::getThreads(),
		feed->deviceId(),
		is_screencast != 0);
	return quill_calls::Wrap(std::move(capture), std::move(feed));
}

void qc_video_capture_push_i420(qc_video_capture *capture, const qc_video_frame *frame) {
	if (!capture || !capture->feed || !frame || frame->width <= 0 || frame->height <= 0) {
		return;
	}
	capture->feed->push(quill_calls::FrameFromI420(*frame));
}

void qc_video_capture_switch_camera(qc_video_capture *capture, const char *device_id) {
	if (!capture || capture->feed) {
		return;
	}
	capture->capture->switchToDevice(device_id ? device_id : "", false);
}

void qc_video_capture_set_state(qc_video_capture *capture, int32_t state) {
	if (capture) {
		capture->capture->setState(quill_calls::StateFromC(state));
	}
}

void qc_video_capture_set_preview(qc_video_capture *capture, qc_video_frame_fn preview, void *user) {
	if (!capture) {
		return;
	}
	quill_calls::ClosePreview(capture);
	if (!preview) {
		capture->capture->setOutput(nullptr);
		return;
	}
	auto guard = std::make_shared<quill_calls::CallbackGuard>();
	{
		std::lock_guard lock(capture->previewMutex);
		capture->previewGuard = guard;
	}
	capture->capture->setOutput(std::make_shared<quill_calls::CallbackSink>(
		[=](const qc_video_frame &frame) {
			guard->run([&] {
				preview(user, &frame);
			});
		}));
}

void qc_video_capture_free(qc_video_capture *capture) {
	if (!capture) {
		return;
	}
	quill_calls::ClosePreview(capture);
	capture->capture->setOutput(nullptr);
	delete capture;
}

} // extern "C"

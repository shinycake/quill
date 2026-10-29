// Quill's tgcalls platform layer: tgcalls' own (platform/tdesktop or
// platform/darwin, as Telegram Desktop builds it) plus external video
// sources, the pictures Quill captures itself and pushes in.
#pragma once

#include "api/media_stream_interface.h"
#include "api/scoped_refptr.h"
#include "api/video/video_frame.h"

#include <memory>
#include <mutex>
#include <string>

namespace quill_calls {

// One external source. tgcalls builds the capturer for it on its media
// thread, some time after the source is created, and may rebuild it.
class ExternalFeed final {
public:
	// The tgcalls device id that makes the platform layer pick this feed.
	const std::string &deviceId() const;

	void push(const webrtc::VideoFrame &frame);
	void setActive(bool active);

	// Called by the capturer tgcalls builds; nullptr when it goes away.
	void attach(webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> source);

	static std::shared_ptr<ExternalFeed> Create();
	static std::shared_ptr<ExternalFeed> Find(const std::string &deviceId);
	~ExternalFeed();

private:
	explicit ExternalFeed(std::string deviceId);

	const std::string _deviceId;
	std::mutex _mutex;
	webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> _source;
	bool _active = true;
};

// Delivers a picture to a source made by tgcalls' platform makeVideoSource.
// Defined per platform, next to how that platform's capturers deliver theirs.
void PushToSource(webrtc::VideoTrackSourceInterface *source, const webrtc::VideoFrame &frame);

} // namespace quill_calls

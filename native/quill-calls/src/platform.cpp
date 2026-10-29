#include "platform.h"

#include "VideoCapturerInterface.h"
#include "VideoCaptureInterface.h"
#include "platform/PlatformInterface.h"

#include <atomic>
#include <map>

namespace tgcalls {

// tgcalls' own platform layer. Its CreatePlatformInterface is renamed to
// this at compile time (CMakeLists.txt) so the one below can wrap it; neither
// of those files uses PlatformInterface::SharedInstance, so the inline
// function keeps a single definition.
std::unique_ptr<PlatformInterface> CreateTgcallsPlatformInterface();

} // namespace tgcalls

namespace quill_calls {
namespace {

constexpr auto kExternalPrefix = "quill:external:";

std::mutex FeedsMutex;
std::map<std::string, std::weak_ptr<ExternalFeed>> Feeds;

class ExternalCapturer final : public tgcalls::VideoCapturerInterface {
public:
	ExternalCapturer(
		webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> source,
		std::shared_ptr<ExternalFeed> feed,
		std::function<void(tgcalls::VideoState)> stateUpdated)
	: _source(source)
	, _feed(std::move(feed))
	, _stateUpdated(std::move(stateUpdated)) {
		_feed->attach(_source);
	}

	~ExternalCapturer() override {
		setUncroppedOutput(nullptr);
		_feed->attach(nullptr);
	}

	void setState(tgcalls::VideoState state) override {
		_feed->setActive(state == tgcalls::VideoState::Active);
		if (_stateUpdated) {
			_stateUpdated(state);
		}
	}

	void setPreferredCaptureAspectRatio(float aspectRatio) override {
	}

	// The local preview, as Telegram Desktop's capturer gives it.
	void setUncroppedOutput(std::shared_ptr<rtc::VideoSinkInterface<webrtc::VideoFrame>> sink) override {
		if (_uncroppedSink) {
			_source->RemoveSink(_uncroppedSink.get());
		}
		_uncroppedSink = std::move(sink);
		if (_uncroppedSink) {
			_source->AddOrUpdateSink(_uncroppedSink.get(), rtc::VideoSinkWants());
		}
	}

	int getRotation() override {
		return 0;
	}

private:
	webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> _source;
	std::shared_ptr<ExternalFeed> _feed;
	std::function<void(tgcalls::VideoState)> _stateUpdated;
	std::shared_ptr<rtc::VideoSinkInterface<webrtc::VideoFrame>> _uncroppedSink;
};

class QuillPlatformInterface final : public tgcalls::PlatformInterface {
public:
	QuillPlatformInterface() : _base(tgcalls::CreateTgcallsPlatformInterface()) {
		preferX264 = _base->preferX264;
	}

	void configurePlatformAudio(int numChannels) override {
		_base->configurePlatformAudio(numChannels);
	}

	std::unique_ptr<rtc::NetworkMonitorFactory> createNetworkMonitorFactory() override {
		return _base->createNetworkMonitorFactory();
	}

	std::unique_ptr<webrtc::VideoEncoderFactory> makeVideoEncoderFactory(bool preferHardwareEncoding, bool isScreencast) override {
		return _base->makeVideoEncoderFactory(preferHardwareEncoding, isScreencast);
	}

	std::unique_ptr<webrtc::VideoDecoderFactory> makeVideoDecoderFactory() override {
		return _base->makeVideoDecoderFactory();
	}

	bool supportsEncoding(const std::string &codecName) override {
		return _base->supportsEncoding(codecName);
	}

	webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> makeVideoSource(rtc::Thread *signalingThread, rtc::Thread *workerThread) override {
		return _base->makeVideoSource(signalingThread, workerThread);
	}

	void adaptVideoSource(webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> videoSource, int width, int height, int fps) override {
		_base->adaptVideoSource(videoSource, width, height, fps);
	}

	std::unique_ptr<tgcalls::VideoCapturerInterface> makeVideoCapturer(
			webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> source,
			std::string deviceId,
			std::function<void(tgcalls::VideoState)> stateUpdated,
			std::function<void(tgcalls::PlatformCaptureInfo)> captureInfoUpdated,
			std::shared_ptr<tgcalls::PlatformContext> platformContext,
			std::pair<int, int> &outResolution) override {
		if (deviceId.rfind(kExternalPrefix, 0) == 0) {
			if (auto feed = ExternalFeed::Find(deviceId)) {
				// Unknown until the first picture; tgcalls only uses it to
				// crop to the receiver's aspect ratio, which a screen skips.
				outResolution = { 0, 0 };
				return std::make_unique<ExternalCapturer>(source, std::move(feed), std::move(stateUpdated));
			}
			return nullptr;
		}
		return _base->makeVideoCapturer(source, deviceId, std::move(stateUpdated), std::move(captureInfoUpdated), platformContext, outResolution);
	}

	webrtc::scoped_refptr<tgcalls::WrappedAudioDeviceModule> wrapAudioDeviceModule(webrtc::scoped_refptr<webrtc::AudioDeviceModule> module) override {
		return _base->wrapAudioDeviceModule(module);
	}

	void setupVideoDecoding(AVCodecContext *codecContext) override {
		_base->setupVideoDecoding(codecContext);
	}

	webrtc::scoped_refptr<webrtc::VideoFrameBuffer> createPlatformFrameFromData(AVFrame const *frame) override {
		return _base->createPlatformFrameFromData(frame);
	}

private:
	const std::unique_ptr<tgcalls::PlatformInterface> _base;
};

} // namespace

ExternalFeed::ExternalFeed(std::string deviceId) : _deviceId(std::move(deviceId)) {
}

ExternalFeed::~ExternalFeed() {
	std::lock_guard lock(FeedsMutex);
	Feeds.erase(_deviceId);
}

std::shared_ptr<ExternalFeed> ExternalFeed::Create() {
	static std::atomic<uint64_t> counter;
	auto result = std::shared_ptr<ExternalFeed>(new ExternalFeed(kExternalPrefix + std::to_string(++counter)));
	std::lock_guard lock(FeedsMutex);
	Feeds.emplace(result->_deviceId, result);
	return result;
}

std::shared_ptr<ExternalFeed> ExternalFeed::Find(const std::string &deviceId) {
	std::lock_guard lock(FeedsMutex);
	const auto i = Feeds.find(deviceId);
	return (i != Feeds.end()) ? i->second.lock() : nullptr;
}

const std::string &ExternalFeed::deviceId() const {
	return _deviceId;
}

void ExternalFeed::attach(webrtc::scoped_refptr<webrtc::VideoTrackSourceInterface> source) {
	std::lock_guard lock(_mutex);
	_source = std::move(source);
}

void ExternalFeed::setActive(bool active) {
	std::lock_guard lock(_mutex);
	_active = active;
}

void ExternalFeed::push(const webrtc::VideoFrame &frame) {
	std::lock_guard lock(_mutex);
	if (_source && _active) {
		PushToSource(_source.get(), frame);
	}
}

} // namespace quill_calls

namespace tgcalls {

std::unique_ptr<PlatformInterface> CreatePlatformInterface() {
	return std::make_unique<quill_calls::QuillPlatformInterface>();
}

} // namespace tgcalls

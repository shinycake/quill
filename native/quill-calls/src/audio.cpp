#include "audio.h"

#include "api/scoped_refptr.h"
#include "modules/audio_device/include/audio_device.h"

#include <chrono>
#include <thread>

namespace quill_calls {
namespace {

// The most microphone sound waiting for the call; older samples are dropped
// so a stalled call never plays the past back late.
constexpr auto kMaxQueuedSamples = size_t(QC_AUDIO_FRAME_SAMPLES * 10);

// FakeAudioDeviceModule's own scheduler sleeps a full tick after each frame,
// so it drifts slower than real time and the microphone queue would grow.
// This one keeps an absolute clock and catches up instead.
void ScheduleOnClock(tgcalls::FakeAudioDeviceModule::Task task) {
	std::thread([task = std::move(task)] {
		using Clock = std::chrono::steady_clock;
		auto next = Clock::now();
		while (true) {
			const auto wait = task();
			if (wait < 0) {
				return;
			}
			next += std::chrono::microseconds(int64_t(wait * 1000000));
			const auto now = Clock::now();
			if (next < now - std::chrono::milliseconds(100)) {
				next = now;
			}
			std::this_thread::sleep_until(next);
		}
	}).detach();
}

} // namespace

class AudioBridge::Recorder final : public tgcalls::FakeAudioDeviceModule::Recorder {
public:
	void push(const int16_t *samples, size_t count) {
		std::lock_guard lock(_mutex);
		_queue.insert(_queue.end(), samples, samples + count);
		if (_queue.size() > kMaxQueuedSamples) {
			_queue.erase(_queue.begin(), _queue.begin() + (_queue.size() - kMaxQueuedSamples));
		}
	}

	tgcalls::AudioFrame Record() override {
		{
			std::lock_guard lock(_mutex);
			const auto available = std::min(_queue.size(), _frame.size());
			std::copy_n(_queue.begin(), available, _frame.begin());
			std::fill(_frame.begin() + available, _frame.end(), int16_t(0));
			_queue.erase(_queue.begin(), _queue.begin() + available);
		}
		return tgcalls::AudioFrame{
			.audio_samples = _frame.data(),
			.num_samples = _frame.size(),
			.bytes_per_sample = sizeof(int16_t),
			.num_channels = 1,
			.samples_per_sec = QC_AUDIO_RATE,
			.elapsed_time_ms = 0,
			.ntp_time_ms = 0,
		};
	}

private:
	std::mutex _mutex;
	std::deque<int16_t> _queue;
	std::vector<int16_t> _frame = std::vector<int16_t>(QC_AUDIO_FRAME_SAMPLES);
};

class AudioBridge::Renderer final : public tgcalls::FakeAudioDeviceModule::Renderer {
public:
	explicit Renderer(Output output) : _output(std::move(output)) {
	}

	bool Render(const tgcalls::AudioFrame &frame) override {
		if (frame.num_channels == 1) {
			_output(frame.audio_samples, frame.num_samples);
			return true;
		}
		// Stereo playout: Quill plays the call in mono.
		_mono.resize(frame.num_samples);
		for (size_t i = 0; i != frame.num_samples; ++i) {
			auto sum = 0;
			for (size_t channel = 0; channel != frame.num_channels; ++channel) {
				sum += frame.audio_samples[i * frame.num_channels + channel];
			}
			_mono[i] = int16_t(sum / int(frame.num_channels));
		}
		_output(_mono.data(), _mono.size());
		return true;
	}

private:
	Output _output;
	std::vector<int16_t> _mono;
};

AudioBridge::AudioBridge(Output output)
: _recorder(std::make_shared<Recorder>())
, _renderer(std::make_shared<Renderer>(std::move(output))) {
}

void AudioBridge::push(const int16_t *samples, size_t count) {
	_recorder->push(samples, count);
}

std::function<webrtc::scoped_refptr<webrtc::AudioDeviceModule>(webrtc::TaskQueueFactory *)> AudioBridge::creator() {
	return tgcalls::FakeAudioDeviceModule::Creator(
		_renderer,
		_recorder,
		tgcalls::FakeAudioDeviceModule::Options{
			.samples_per_sec = QC_AUDIO_RATE,
			.num_channels = 1,
			.scheduler_ = ScheduleOnClock,
		});
}

} // namespace quill_calls

// Quill's sound in and out of a call. tg_owt is built without audio
// backends, so every call gets tgcalls' FakeAudioDeviceModule wired to an
// AudioBridge: Quill pushes microphone frames in and receives the call's
// mixed sound out, 10 ms at a time, 48 kHz mono.
#pragma once

#include "quill_calls.h"

#include "FakeAudioDeviceModule.h"

#include <deque>
#include <functional>
#include <memory>
#include <mutex>
#include <vector>

namespace webrtc {
class AudioDeviceModule;
class TaskQueueFactory;
template <class T>
class scoped_refptr;
} // namespace webrtc

namespace quill_calls {

class AudioBridge final {
public:
	using Output = std::function<void(const int16_t *samples, size_t count)>;

	explicit AudioBridge(Output output);

	// Queues microphone samples for the call to pick up.
	void push(const int16_t *samples, size_t count);

	// For tgcalls::Descriptor::createAudioDeviceModule; call once.
	std::function<webrtc::scoped_refptr<webrtc::AudioDeviceModule>(webrtc::TaskQueueFactory *)> creator();

private:
	class Recorder;
	class Renderer;

	std::shared_ptr<Recorder> _recorder;
	std::shared_ptr<Renderer> _renderer;
};

} // namespace quill_calls

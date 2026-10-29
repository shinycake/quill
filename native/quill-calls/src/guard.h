// A call's callbacks outlive nothing: CallbackGuard lets the call's destroy
// function wait out any callback in flight and refuse every later one, since
// tgcalls finishes tearing an instance down on its own threads.
#pragma once

#include <mutex>
#include <shared_mutex>

namespace quill_calls {

class CallbackGuard {
public:
	// Runs `callback` unless the guard is closed. Callbacks run concurrently.
	template <typename Callback>
	void run(Callback &&callback) {
		std::shared_lock lock(_mutex);
		if (_open) {
			callback();
		}
	}

	// Returns once no callback is running; none runs afterwards.
	void close() {
		std::unique_lock lock(_mutex);
		_open = false;
	}

private:
	std::shared_mutex _mutex;
	bool _open = true;
};

} // namespace quill_calls

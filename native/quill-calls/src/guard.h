// A call's callbacks outlive nothing: CallbackGuard lets the call's destroy
// function wait out any callback in flight and refuse every later one, since
// tgcalls finishes tearing an instance down on its own threads.
#pragma once

#include <atomic>
#include <mutex>
#include <shared_mutex>

namespace quill_calls {

class CallbackGuard {
public:
	// Runs `callback` unless the guard is closing or closed. Callbacks run
	// concurrently.
	template <typename Callback>
	void run(Callback &&callback) {
		// Checked before the shared lock: glibc's rwlock prefers readers,
		// and a conference call's e2e callbacks hold it across a TDLib
		// round trip on several media threads at once, so close() could
		// wait for a gap that never comes. Once closing, new callbacks
		// are no-ops and only the ones already in flight are waited out.
		if (_closing.load(std::memory_order_acquire)) {
			return;
		}
		std::shared_lock lock(_mutex);
		if (_open) {
			callback();
		}
	}

	// Returns once no callback is running; none runs afterwards.
	void close() {
		_closing.store(true, std::memory_order_release);
		std::unique_lock lock(_mutex);
		_open = false;
	}

private:
	std::atomic<bool> _closing = false;
	std::shared_mutex _mutex;
	bool _open = true;
};

} // namespace quill_calls

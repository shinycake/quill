// libquill_calls: the C API in include/quill_calls.h over tgcalls. This file
// holds the library-wide parts; calls, video and audio have their own files.
#include "quill_calls.h"

#include "Instance.h"

#include "rtc_base/logging.h"

#include <algorithm>
#include <cstdlib>
#include <mutex>
#include <string>

namespace tgcalls {
class InstanceImpl;
class InstanceV2Impl;
class InstanceV2ReferenceImpl;
} // namespace tgcalls

namespace {

// The same three implementations Telegram Desktop registers
// (calls_call.cpp), so Quill offers and accepts the same protocol versions.
const auto kRegisterLegacy = tgcalls::Register<tgcalls::InstanceImpl>();
const auto kRegisterV2 = tgcalls::Register<tgcalls::InstanceV2Impl>();
const auto kRegisterV2Reference = tgcalls::Register<tgcalls::InstanceV2ReferenceImpl>();

std::string JoinedVersionsAsTdesktopSends() {
	auto versions = tgcalls::Meta::Versions();
	auto result = std::string();
	for (auto i = versions.rbegin(); i != versions.rend(); ++i) {
		if (!result.empty()) {
			result += '\n';
		}
		result += *i;
	}
	return result;
}

class CallbackLogSink final : public rtc::LogSink {
public:
	CallbackLogSink(qc_log_fn log, void *user) : _log(log), _user(user) {
	}

	using rtc::LogSink::OnLogMessage;

	void OnLogMessage(const std::string &message, rtc::LoggingSeverity severity) override {
		_log(_user, int32_t(severity), TrimmedLine(message).c_str());
	}

	void OnLogMessage(const std::string &message) override {
		_log(_user, QC_LOG_INFO, TrimmedLine(message).c_str());
	}

private:
	static std::string TrimmedLine(const std::string &message) {
		auto result = message;
		while (!result.empty() && (result.back() == '\n' || result.back() == '\r')) {
			result.pop_back();
		}
		return result;
	}

	qc_log_fn _log = nullptr;
	void *_user = nullptr;
};

std::mutex LogMutex;
std::unique_ptr<CallbackLogSink> LogSink;

} // namespace

extern "C" {

uint32_t qc_api_version(void) {
	return QC_API_VERSION;
}

const char *qc_library_versions(void) {
	static const auto versions = JoinedVersionsAsTdesktopSends();
	return versions.c_str();
}

int32_t qc_max_layer(void) {
	return tgcalls::Meta::MaxLayer();
}

void qc_string_free(char *string) {
	std::free(string);
}

void qc_set_log(qc_log_fn log, void *user, int32_t min_level) {
	std::lock_guard lock(LogMutex);
	if (LogSink) {
		// Waits out a line being delivered, so the old sink can go.
		rtc::LogMessage::RemoveLogToStream(LogSink.get());
		LogSink = nullptr;
	}
	if (!log) {
		return;
	}
	// Quill's sink is the only place WebRTC writes to.
	rtc::LogMessage::LogToDebug(rtc::LS_NONE);
	rtc::LogMessage::SetLogToStderr(false);
	const auto severity = rtc::LoggingSeverity(std::clamp(min_level, int32_t(QC_LOG_VERBOSE), int32_t(QC_LOG_ERROR)));
	LogSink = std::make_unique<CallbackLogSink>(log, user);
	rtc::LogMessage::AddLogToStream(LogSink.get(), severity);
}

} // extern "C"

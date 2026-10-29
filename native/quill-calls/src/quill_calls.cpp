// libquill_calls: the C API in include/quill_calls.h over tgcalls.
#include "quill_calls.h"

#include "Instance.h"

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

} // extern "C"

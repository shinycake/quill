#pragma once

#include "quill_calls.h"

#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace tgcalls {
class VideoCaptureInterface;
} // namespace tgcalls

namespace quill_calls {

// The cameras tgcalls can open on this platform, as {id, name}.
std::vector<std::pair<std::string, std::string>> CameraDevices();

std::shared_ptr<tgcalls::VideoCaptureInterface> CaptureOf(qc_video_capture *capture);

// Copies a string for the caller to free with qc_string_free.
char *CopyForCaller(const std::string &value);

} // namespace quill_calls

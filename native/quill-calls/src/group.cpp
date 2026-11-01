// Group calls: tgcalls::GroupInstanceCustomImpl configured and driven the way
// Telegram Desktop drives it (calls_group_call.cpp: GroupCall::tryCreateController
// for the call, tryCreateScreencast for its presentation).
#include "audio.h"
#include "guard.h"
#include "video.h"
#include "video_frame.h"

#include "StaticThreads.h"
#include "VideoCaptureInterface.h"
#include "group/GroupInstanceCustomImpl.h"
#include "third-party/json11.hpp"

#include "rtc_base/logging.h"
#include "rtc_base/thread.h"

#include <chrono>
#include <cstdlib>
#include <future>
#include <map>
#include <mutex>
#include <optional>
#include <variant>

namespace quill_calls {
namespace {

// tgcalls answers in milliseconds; this only ends a wait that went wrong.
constexpr auto kJoinPayloadTimeout = std::chrono::seconds(2);

// The answers tgcalls waits for, by the request id Quill was given. A
// request is answered, cancelled by tgcalls, or dropped at destroy, once.
class Requests final : public std::enable_shared_from_this<Requests> {
public:
	using MediaChannels = std::function<void(std::vector<tgcalls::MediaChannelDescription> &&)>;
	using CurrentTime = std::function<void(int64_t)>;
	using Part = std::function<void(tgcalls::BroadcastPart &&)>;
	using Answer = std::variant<MediaChannels, CurrentTime, Part>;

	Requests(std::shared_ptr<CallbackGuard> guard, qc_group_callbacks callbacks)
	: _guard(std::move(guard))
	, _callbacks(callbacks) {
	}

	template <typename Kind>
	uint64_t add(Kind answer) {
		std::lock_guard lock(_mutex);
		const auto id = ++_lastId;
		_pending.emplace(id, Answer(std::in_place_type<Kind>, std::move(answer)));
		return id;
	}

	template <typename Kind>
	std::optional<Kind> take(uint64_t id) {
		std::lock_guard lock(_mutex);
		const auto i = _pending.find(id);
		if (i == _pending.end() || !std::holds_alternative<Kind>(i->second)) {
			return std::nullopt;
		}
		auto result = std::get<Kind>(std::move(i->second));
		_pending.erase(i);
		return result;
	}

	void cancel(uint64_t id) {
		{
			std::lock_guard lock(_mutex);
			if (!_pending.erase(id)) {
				return;
			}
		}
		const auto c = _callbacks;
		_guard->run([&] {
			if (c.request_cancelled) {
				c.request_cancelled(c.user, id);
			}
		});
	}

	// Drops every open request without telling Quill, at destroy.
	void clear() {
		auto dropped = std::map<uint64_t, Answer>();
		{
			std::lock_guard lock(_mutex);
			dropped.swap(_pending);
		}
	}

private:
	const std::shared_ptr<CallbackGuard> _guard;
	const qc_group_callbacks _callbacks;
	std::mutex _mutex;
	uint64_t _lastId = 0;
	std::map<uint64_t, Answer> _pending;
};

// What tgcalls holds for an open request; cancel() is tdesktop's
// LoadPartTask::cancel and MediaChannelDescriptionsTask::cancel.
template <typename Base>
class Task final : public Base {
public:
	Task(std::weak_ptr<Requests> requests, uint64_t id)
	: _requests(std::move(requests))
	, _id(id) {
	}

	void cancel() override {
		if (const auto requests = _requests.lock()) {
			requests->cancel(_id);
		}
	}

private:
	const std::weak_ptr<Requests> _requests;
	const uint64_t _id;
};

tgcalls::VideoChannelDescription::Quality QualityFromC(int32_t quality) {
	switch (quality) {
	case QC_GROUP_QUALITY_FULL: return tgcalls::VideoChannelDescription::Quality::Full;
	case QC_GROUP_QUALITY_MEDIUM: return tgcalls::VideoChannelDescription::Quality::Medium;
	default: return tgcalls::VideoChannelDescription::Quality::Thumbnail;
	}
}

int32_t QualityToC(tgcalls::VideoChannelDescription::Quality quality) {
	switch (quality) {
	case tgcalls::VideoChannelDescription::Quality::Full: return QC_GROUP_QUALITY_FULL;
	case tgcalls::VideoChannelDescription::Quality::Medium: return QC_GROUP_QUALITY_MEDIUM;
	case tgcalls::VideoChannelDescription::Quality::Thumbnail: return QC_GROUP_QUALITY_THUMBNAIL;
	}
	return QC_GROUP_QUALITY_THUMBNAIL;
}

tgcalls::BroadcastPart::Status StatusFromC(int32_t status) {
	switch (status) {
	case QC_BROADCAST_PART_SUCCESS: return tgcalls::BroadcastPart::Status::Success;
	case QC_BROADCAST_PART_NOT_READY: return tgcalls::BroadcastPart::Status::NotReady;
	default: return tgcalls::BroadcastPart::Status::ResyncNeeded;
	}
}

tgcalls::GroupConnectionMode ModeFromC(int32_t mode) {
	switch (mode) {
	case QC_GROUP_MODE_RTC: return tgcalls::GroupConnectionMode::GroupConnectionModeRtc;
	case QC_GROUP_MODE_BROADCAST: return tgcalls::GroupConnectionMode::GroupConnectionModeBroadcast;
	default: return tgcalls::GroupConnectionMode::GroupConnectionModeNone;
	}
}

// An ssrc or a user id: a JSON number, or a string for ids past 2^53.
int64_t IntegerOf(const json11::Json &value) {
	if (value.is_string()) {
		return std::strtoll(value.string_value().c_str(), nullptr, 10);
	}
	return int64_t(value.number_value());
}

std::optional<std::vector<tgcalls::VideoChannelDescription>> ParseRequestedVideo(const std::string &text) {
	auto error = std::string();
	const auto json = json11::Json::parse(text, error);
	if (!json.is_array()) {
		RTC_LOG(LS_ERROR) << "quill_calls: bad requested video channels: " << error;
		return std::nullopt;
	}
	auto result = std::vector<tgcalls::VideoChannelDescription>();
	for (const auto &item : json.array_items()) {
		if (!item.is_object() || item["endpoint"].string_value().empty()) {
			RTC_LOG(LS_ERROR) << "quill_calls: a requested video channel has no endpoint";
			return std::nullopt;
		}
		auto channel = tgcalls::VideoChannelDescription{
			.audioSsrc = uint32_t(IntegerOf(item["audio_ssrc"])),
			.userId = IntegerOf(item["user_id"]),
			.endpointId = item["endpoint"].string_value(),
			.minQuality = QualityFromC(item["min_quality"].int_value()),
			.maxQuality = QualityFromC(item["max_quality"].int_value()),
		};
		for (const auto &group : item["ssrc_groups"].array_items()) {
			auto ssrcGroup = tgcalls::MediaSsrcGroup{
				.semantics = group["semantics"].string_value(),
			};
			for (const auto &ssrc : group["ssrcs"].array_items()) {
				ssrcGroup.ssrcs.push_back(uint32_t(IntegerOf(ssrc)));
			}
			channel.ssrcGroups.push_back(std::move(ssrcGroup));
		}
		result.push_back(std::move(channel));
	}
	return result;
}

// qc_group_e2e_result_fn: `result` is the frame tgcalls gets back.
void SetE2eResult(void *result, const uint8_t *data, size_t length) {
	auto &frame = *static_cast<std::vector<uint8_t>*>(result);
	if (data && length) {
		frame.assign(data, data + length);
	} else {
		frame.clear();
	}
}

} // namespace
} // namespace quill_calls

struct qc_group {
	std::shared_ptr<quill_calls::CallbackGuard> guard;
	qc_group_callbacks callbacks = {};
	std::shared_ptr<quill_calls::Requests> requests;
	std::unique_ptr<quill_calls::AudioBridge> audio;
	std::mutex sinksMutex;
	// tgcalls holds weak pointers to these: dropping one stops its video.
	std::map<std::string, std::shared_ptr<quill_calls::CallbackSink>> sinks;
	std::unique_ptr<tgcalls::GroupInstanceCustomImpl> instance;
};

namespace quill_calls {
namespace {

qc_group *Create(const std::string &configJson, const qc_group_callbacks &callbacks, qc_video_capture *video) {
	auto error = std::string();
	const auto json = configJson.empty()
		? json11::Json(json11::Json::object())
		: json11::Json::parse(configJson, error);
	if (!json.is_object()) {
		RTC_LOG(LS_ERROR) << "quill_calls: bad group call config: " << error;
		return nullptr;
	}
	const auto presentation = json["presentation"].bool_value();
	// A conference call's frames are never sent in the clear: without the
	// callback that encrypts them there is no instance at all.
	if (json["e2e"].bool_value() && !callbacks.e2e) {
		RTC_LOG(LS_ERROR) << "quill_calls: an e2e group call needs callbacks.e2e";
		return nullptr;
	}

	auto result = std::make_unique<qc_group>();
	const auto guard = result->guard = std::make_shared<CallbackGuard>();
	const auto c = result->callbacks = callbacks;
	const auto requests = result->requests = std::make_shared<Requests>(guard, c);
	const auto weakRequests = std::weak_ptr<Requests>(requests);
	result->audio = std::make_unique<AudioBridge>([=](const int16_t *samples, size_t count) {
		if (presentation) {
			// tdesktop's screen instance plays nothing.
			return;
		}
		guard->run([&] {
			if (c.audio_out) {
				c.audio_out(c.user, samples, count);
			}
		});
	});

	auto codecs = std::vector<tgcalls::VideoCodecName>();
	if (json["prioritize_vp8"].bool_value()) {
		codecs.push_back(tgcalls::VideoCodecName::VP8);
	}

	auto descriptor = tgcalls::GroupInstanceDescriptor{
		.threads = tgcalls::StaticThreads::getThreads(),
		// Logging goes to qc_set_log's sink, not to tgcalls' own file.
		.config = tgcalls::GroupConfig{ .need_log = false },
		.networkStateUpdated = [=](tgcalls::GroupNetworkState state) {
			guard->run([&] {
				if (c.network_state) {
					c.network_state(c.user, state.isConnected ? 1 : 0, state.isTransitioningFromBroadcastToRtc ? 1 : 0);
				}
			});
		},
		.createAudioDeviceModule = result->audio->creator(),
		.videoCapture = CaptureOf(video),
		// tdesktop leaves these out of its screen instance; they are wired
		// in both so that a presentation in broadcast mode can't call an
		// empty function. A request with no Quill callback stays open
		// until tgcalls cancels it.
		.requestCurrentTime = [=](std::function<void(int64_t)> done) {
			const auto id = requests->add<Requests::CurrentTime>(std::move(done));
			guard->run([&] {
				if (c.request_current_time) {
					c.request_current_time(c.user, id);
				}
			});
			return std::make_shared<Task<tgcalls::BroadcastPartTask>>(weakRequests, id);
		},
		.requestAudioBroadcastPart = [=](int64_t time, int64_t period, std::function<void(tgcalls::BroadcastPart &&)> done) {
			const auto id = requests->add<Requests::Part>(std::move(done));
			const auto part = qc_group_broadcast_part{
				.timestamp_ms = time,
				.duration_ms = period,
				.is_video = 0,
				.video_channel = 0,
				.video_quality = 0,
			};
			guard->run([&] {
				if (c.request_broadcast_part) {
					c.request_broadcast_part(c.user, id, &part);
				}
			});
			return std::make_shared<Task<tgcalls::BroadcastPartTask>>(weakRequests, id);
		},
		.requestVideoBroadcastPart = [=](int64_t time, int64_t period, int32_t channel, tgcalls::VideoChannelDescription::Quality quality, std::function<void(tgcalls::BroadcastPart &&)> done) {
			const auto id = requests->add<Requests::Part>(std::move(done));
			const auto part = qc_group_broadcast_part{
				.timestamp_ms = time,
				.duration_ms = period,
				.is_video = 1,
				.video_channel = channel,
				.video_quality = QualityToC(quality),
			};
			guard->run([&] {
				if (c.request_broadcast_part) {
					c.request_broadcast_part(c.user, id, &part);
				}
			});
			return std::make_shared<Task<tgcalls::BroadcastPartTask>>(weakRequests, id);
		},
		.videoContentType = presentation
			? tgcalls::VideoContentType::Screencast
			: tgcalls::VideoContentType::Generic,
		// Quill denoises the microphone itself (RNNoise) before pushing it.
		.initialEnableNoiseSuppression = false,
		.videoCodecPreferences = std::move(codecs),
	};
	if (!presentation) {
		// GroupCall::tryCreateController's audioLevelsUpdated: a batch of
		// only our own level is dropped while it repeats, so a muted
		// microphone doesn't send a stream of zeros.
		const auto myLevel = std::make_shared<tgcalls::GroupLevelValue>();
		descriptor.audioLevelsUpdated = [=](const tgcalls::GroupLevelsUpdate &data) {
			const auto &updates = data.updates;
			if (updates.empty()) {
				return;
			} else if (updates.size() == 1 && !updates.front().ssrc) {
				const auto &value = updates.front().value;
				if (myLevel->level == value.level && myLevel->voice == value.voice) {
					return;
				}
				*myLevel = value;
			}
			auto levels = std::vector<qc_group_level>();
			levels.reserve(updates.size());
			for (const auto &update : updates) {
				levels.push_back(qc_group_level{
					.ssrc = update.ssrc,
					.level = update.value.level,
					.voice = update.value.voice ? 1 : 0,
				});
			}
			guard->run([&] {
				if (c.audio_levels) {
					c.audio_levels(c.user, levels.data(), levels.size());
				}
			});
		};
		if (c.request_media_channels) {
			descriptor.requestMediaChannelDescriptions = [=](const std::vector<uint32_t> &ssrcs, std::function<void(std::vector<tgcalls::MediaChannelDescription> &&)> done) {
				const auto id = requests->add<Requests::MediaChannels>(std::move(done));
				guard->run([&] {
					c.request_media_channels(c.user, id, ssrcs.data(), ssrcs.size());
				});
				return std::make_shared<Task<tgcalls::RequestMediaChannelDescriptionTask>>(weakRequests, id);
			};
		}
	}

	if (json["e2e"].bool_value()) {
		// tdesktop's TdE2E::EncryptDecrypt::callback: an empty frame is
		// one tgcalls drops, which is also what a closed guard gives.
		descriptor.e2eEncryptDecrypt = [=](const std::vector<uint8_t> &data, int64_t userId, bool isEncrypt, int32_t unencryptedPrefixSize) {
			auto result = std::vector<uint8_t>();
			const auto frame = qc_group_e2e_frame{
				.data = data.data(),
				.length = data.size(),
				.is_encrypt = isEncrypt ? 1 : 0,
				.user_id = userId,
				.unencrypted_prefix_size = unencryptedPrefixSize,
			};
			guard->run([&] {
				c.e2e(c.user, &frame, SetE2eResult, &result);
			});
			return result;
		};
	}

	result->instance = std::make_unique<tgcalls::GroupInstanceCustomImpl>(std::move(descriptor));
	return result.release();
}

} // namespace
} // namespace quill_calls

extern "C" {

qc_group *qc_group_create(const char *config_json, const qc_group_callbacks *callbacks, qc_video_capture *video) {
	if (!callbacks) {
		return nullptr;
	}
	return quill_calls::Create(config_json ? config_json : "", *callbacks, video);
}

char *qc_group_join_payload(qc_group *group, uint32_t *audio_ssrc) {
	if (!group) {
		return nullptr;
	}
	// tgcalls makes the payload on its network thread; the promise outlives
	// this call in case the completion is still running there, or runs late.
	const auto ready = std::make_shared<std::promise<tgcalls::GroupJoinPayload>>();
	auto future = ready->get_future();
	group->instance->emitJoinPayload([ready](const tgcalls::GroupJoinPayload &payload) {
		ready->set_value(payload);
	});
	// The caller is the UI thread: a completion that never comes fails the
	// join, and one tgcalls dropped must not throw across the C boundary.
	if (future.wait_for(quill_calls::kJoinPayloadTimeout) != std::future_status::ready) {
		return nullptr;
	}
	try {
		const auto payload = future.get();
		if (audio_ssrc) {
			*audio_ssrc = payload.audioSsrc;
		}
		return quill_calls::CopyForCaller(payload.json);
	} catch (const std::future_error &) {
		return nullptr;
	}
}

void qc_group_set_join_response(qc_group *group, const char *payload_json) {
	if (group && payload_json) {
		group->instance->setJoinResponsePayload(payload_json);
	}
}

void qc_group_set_connection_mode(qc_group *group, int32_t mode, int32_t is_unified_broadcast) {
	if (group) {
		// keepBroadcastIfWasEnabled is always true in tdesktop.
		group->instance->setConnectionMode(quill_calls::ModeFromC(mode), true, is_unified_broadcast != 0);
	}
}

void qc_group_set_muted(qc_group *group, int32_t muted) {
	if (group) {
		group->instance->setIsMuted(muted != 0);
	}
}

void qc_group_push_audio(qc_group *group, const int16_t *samples, size_t count) {
	if (group && samples) {
		group->audio->push(samples, count);
	}
}

void qc_group_set_volume(qc_group *group, uint32_t ssrc, double volume) {
	if (group) {
		group->instance->setVolume(ssrc, volume);
	}
}

void qc_group_remove_ssrcs(qc_group *group, const uint32_t *ssrcs, size_t count) {
	if (group && ssrcs && count) {
		group->instance->removeSsrcs(std::vector<uint32_t>(ssrcs, ssrcs + count));
	}
}

int32_t qc_group_set_requested_video(qc_group *group, const char *channels_json) {
	if (!group || !channels_json) {
		return 0;
	}
	auto channels = quill_calls::ParseRequestedVideo(channels_json);
	if (!channels) {
		return 0;
	}
	group->instance->setRequestedVideoChannels(std::move(*channels));
	return 1;
}

void qc_group_add_incoming_video(qc_group *group, const char *endpoint) {
	if (!group || !endpoint || !*endpoint) {
		return;
	}
	const auto id = std::string(endpoint);
	std::lock_guard lock(group->sinksMutex);
	if (group->sinks.contains(id)) {
		return;
	}
	const auto guard = group->guard;
	const auto c = group->callbacks;
	auto sink = std::make_shared<quill_calls::CallbackSink>([=](const qc_video_frame &frame) {
		guard->run([&] {
			if (c.incoming_video) {
				c.incoming_video(c.user, id.c_str(), &frame);
			}
		});
	});
	group->instance->addIncomingVideoOutput(id, sink);
	group->sinks.emplace(id, std::move(sink));
}

void qc_group_remove_incoming_video(qc_group *group, const char *endpoint) {
	if (!group || !endpoint) {
		return;
	}
	auto removed = std::shared_ptr<quill_calls::CallbackSink>();
	std::lock_guard lock(group->sinksMutex);
	if (const auto i = group->sinks.find(endpoint); i != group->sinks.end()) {
		removed = std::move(i->second);
		group->sinks.erase(i);
	}
}

void qc_group_set_video_capture(qc_group *group, qc_video_capture *video) {
	if (group) {
		group->instance->setVideoCapture(quill_calls::CaptureOf(video));
	}
}

void qc_group_answer_media_channels(qc_group *group, uint64_t request_id, const qc_group_media_channel *channels, size_t count) {
	if (!group) {
		return;
	}
	auto done = group->requests->take<quill_calls::Requests::MediaChannels>(request_id);
	if (!done) {
		return;
	}
	auto descriptions = std::vector<tgcalls::MediaChannelDescription>();
	for (size_t i = 0; channels && i != count; ++i) {
		descriptions.push_back(tgcalls::MediaChannelDescription{
			.type = tgcalls::MediaChannelDescription::Type::Audio,
			.audioSsrc = channels[i].audio_ssrc,
			.userId = channels[i].user_id,
		});
	}
	(*done)(std::move(descriptions));
}

void qc_group_answer_current_time(qc_group *group, uint64_t request_id, int64_t timestamp_ms) {
	if (!group) {
		return;
	}
	if (auto done = group->requests->take<quill_calls::Requests::CurrentTime>(request_id)) {
		(*done)(timestamp_ms);
	}
}

void qc_group_answer_broadcast_part(qc_group *group, uint64_t request_id, int32_t status, int64_t timestamp_ms, double response_time, const uint8_t *data, size_t length) {
	if (!group) {
		return;
	}
	auto done = group->requests->take<quill_calls::Requests::Part>(request_id);
	if (!done) {
		return;
	}
	auto part = tgcalls::BroadcastPart{
		.timestampMilliseconds = timestamp_ms,
		.responseTimestamp = response_time,
		.status = quill_calls::StatusFromC(status),
	};
	if (part.status == tgcalls::BroadcastPart::Status::Success && data) {
		part.data.assign(data, data + length);
	}
	(*done)(std::move(part));
}

void qc_group_destroy(qc_group *group) {
	if (!group) {
		return;
	}
	group->guard->close();
	group->requests->clear();
	// As tdesktop's GroupCall::destroyController (stop, then drop the
	// instance), except that this waits for tgcalls to finish, as
	// qc_p2p_destroy does.
	std::promise<void> stopped;
	group->instance->stop([&stopped] {
		stopped.set_value();
	});
	stopped.get_future().wait();
	// Waits for the instance's internals to go on the media thread.
	group->instance = nullptr;
	const auto threads = tgcalls::StaticThreads::getThreads();
	for (auto pass = 0; pass != 2; ++pass) {
		for (const auto thread : {threads->getMediaThread(), threads->getNetworkThread(), threads->getWorkerThread()}) {
			thread->BlockingCall([] {
			});
		}
	}
	delete group;
}

} // extern "C"

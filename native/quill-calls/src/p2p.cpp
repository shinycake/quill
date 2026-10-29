// 1:1 calls: tgcalls::Instance built from TDLib's callStateReady the way
// Telegram Desktop builds it from phoneCall (calls_call.cpp, Call::createAndStartController).
#include "audio.h"
#include "guard.h"
#include "video.h"
#include "video_frame.h"

#include "Instance.h"
#include "StaticThreads.h"
#include "VideoCaptureInterface.h"
#include "third-party/json11.hpp"

#include "rtc_base/logging.h"
#include "rtc_base/thread.h"

#include <array>
#include <cstdlib>
#include <cstring>
#include <future>
#include <iterator>
#include <optional>
#include <set>

struct qc_p2p {
	std::shared_ptr<quill_calls::CallbackGuard> guard;
	std::unique_ptr<quill_calls::AudioBridge> audio;
	std::shared_ptr<quill_calls::CallbackSink> remoteVideo;
	std::unique_ptr<tgcalls::Instance> instance;
};

namespace quill_calls {
namespace {

std::optional<std::string> DecodeBase64(const std::string &value) {
	auto result = std::string();
	auto buffer = uint32_t(0);
	auto bits = 0;
	for (const auto c : value) {
		int digit;
		if (c >= 'A' && c <= 'Z') {
			digit = c - 'A';
		} else if (c >= 'a' && c <= 'z') {
			digit = c - 'a' + 26;
		} else if (c >= '0' && c <= '9') {
			digit = c - '0' + 52;
		} else if (c == '+' || c == '-') {
			digit = 62;
		} else if (c == '/' || c == '_') {
			digit = 63;
		} else if (c == '=') {
			break;
		} else {
			return std::nullopt;
		}
		buffer = (buffer << 6) | uint32_t(digit);
		bits += 6;
		if (bits >= 8) {
			bits -= 8;
			result.push_back(char((buffer >> bits) & 0xFF));
		}
	}
	return result;
}

std::string Hex(const std::string &value) {
	const auto digit = [](unsigned char c) {
		return char((c < 10) ? ('0' + c) : ('a' + c - 10));
	};
	auto result = std::string();
	result.reserve(value.size() * 2);
	for (const auto ch : value) {
		result += digit((unsigned char)(ch) / 16);
		result += digit((unsigned char)(ch) % 16);
	}
	return result;
}

struct Server {
	int64_t id = 0;
	std::string ipv4;
	std::string ipv6;
	uint16_t port = 0;
	bool reflector = false;
	// Reflector.
	std::string peerTag;
	bool tcp = false;
	// WebRTC.
	std::string username;
	std::string password;
	bool turn = false;
	bool stun = false;
};

std::optional<Server> ParseServer(const json11::Json &json) {
	auto result = Server();
	const auto id = json["id"].string_value();
	if (id.empty()) {
		return std::nullopt;
	}
	result.id = std::strtoll(id.c_str(), nullptr, 10);
	result.ipv4 = json["ipv4"].string_value();
	result.ipv6 = json["ipv6"].string_value();
	result.port = uint16_t(json["port"].int_value());
	if (const auto &reflector = json["reflector"]; reflector.is_object()) {
		const auto tag = DecodeBase64(reflector["peer_tag"].string_value());
		if (!tag) {
			return std::nullopt;
		}
		result.reflector = true;
		result.peerTag = *tag;
		result.tcp = reflector["tcp"].bool_value();
	} else if (const auto &webrtc = json["webrtc"]; webrtc.is_object()) {
		result.username = webrtc["username"].string_value();
		result.password = webrtc["password"].string_value();
		result.turn = webrtc["turn"].bool_value();
		result.stun = webrtc["stun"].bool_value();
	} else {
		return std::nullopt;
	}
	return result;
}

// calls_call.cpp AppendEndpoint.
void AppendEndpoint(std::vector<tgcalls::Endpoint> &list, const Server &server) {
	if (!server.reflector || server.peerTag.size() != 16 || server.tcp) {
		return;
	}
	auto endpoint = tgcalls::Endpoint{
		.endpointId = server.id,
		.host = tgcalls::EndpointHost{ .ipv4 = server.ipv4, .ipv6 = server.ipv6 },
		.port = server.port,
		.type = tgcalls::EndpointType::UdpRelay,
	};
	std::memcpy(endpoint.peerTag, server.peerTag.data(), 16);
	list.push_back(std::move(endpoint));
}

// calls_call.cpp AppendServer: a reflector is also a TURN server whose id is
// its rank among the reflector ids.
void AppendServer(std::vector<tgcalls::RtcServer> &list, const Server &server, const std::set<int64_t> &ids) {
	if (server.reflector) {
		const auto i = ids.find(server.id);
		const auto id = uint8_t(std::distance(ids.begin(), i) + 1);
		const auto pushTurn = [&](const std::string &host) {
			list.push_back(tgcalls::RtcServer{
				.id = id,
				.host = host,
				.port = server.port,
				.login = "reflector",
				.password = Hex(server.peerTag),
				.isTurn = true,
				.isTcp = server.tcp,
			});
		};
		pushTurn(server.ipv4);
		pushTurn(server.ipv6);
		return;
	}
	if (server.stun) {
		const auto pushStun = [&](const std::string &host) {
			if (host.empty()) {
				return;
			}
			list.push_back(tgcalls::RtcServer{
				.host = host,
				.port = server.port,
				.isTurn = false,
			});
		};
		pushStun(server.ipv4);
		pushStun(server.ipv6);
	}
	if (server.turn && !server.username.empty() && !server.password.empty()) {
		const auto pushTurn = [&](const std::string &host) {
			list.push_back(tgcalls::RtcServer{
				.host = host,
				.port = server.port,
				.login = server.username,
				.password = server.password,
				.isTurn = true,
			});
		};
		pushTurn(server.ipv4);
		pushTurn(server.ipv6);
	}
}

int32_t VideoStateToC(tgcalls::VideoState state) {
	switch (state) {
	case tgcalls::VideoState::Active: return QC_VIDEO_ACTIVE;
	case tgcalls::VideoState::Paused: return QC_VIDEO_PAUSED;
	case tgcalls::VideoState::Inactive: return QC_VIDEO_INACTIVE;
	}
	return QC_VIDEO_INACTIVE;
}

int32_t StateToC(tgcalls::State state) {
	switch (state) {
	case tgcalls::State::WaitInit: return QC_P2P_WAIT_INIT;
	case tgcalls::State::WaitInitAck: return QC_P2P_WAIT_INIT_ACK;
	case tgcalls::State::Established: return QC_P2P_ESTABLISHED;
	case tgcalls::State::Failed: return QC_P2P_FAILED;
	case tgcalls::State::Reconnecting: return QC_P2P_RECONNECTING;
	}
	return QC_P2P_FAILED;
}

qc_p2p *Create(const std::string &configJson, const qc_p2p_callbacks &callbacks, qc_video_capture *video) {
	auto error = std::string();
	const auto json = json11::Json::parse(configJson, error);
	if (!json.is_object()) {
		RTC_LOG(LS_ERROR) << "quill_calls: bad call config: " << error;
		return nullptr;
	}
	const auto version = json["version"].string_value();
	const auto key = DecodeBase64(json["encryption_key"].string_value());
	if (!key || key->size() != tgcalls::EncryptionKey::kSize) {
		RTC_LOG(LS_ERROR) << "quill_calls: the encryption key must be " << tgcalls::EncryptionKey::kSize << " bytes";
		return nullptr;
	}
	auto keyValue = std::make_shared<std::array<uint8_t, tgcalls::EncryptionKey::kSize>>();
	std::memcpy(keyValue->data(), key->data(), key->size());

	auto servers = std::vector<Server>();
	for (const auto &item : json["servers"].array_items()) {
		if (auto server = ParseServer(item)) {
			servers.push_back(std::move(*server));
		} else {
			RTC_LOG(LS_WARNING) << "quill_calls: skipping a malformed call server";
		}
	}

	auto result = std::make_unique<qc_p2p>();
	const auto guard = result->guard = std::make_shared<CallbackGuard>();
	const auto c = callbacks;
	result->audio = std::make_unique<AudioBridge>([=](const int16_t *samples, size_t count) {
		guard->run([&] {
			if (c.audio_out) {
				c.audio_out(c.user, samples, count);
			}
		});
	});
	result->remoteVideo = std::make_shared<CallbackSink>([=](const qc_video_frame &frame) {
		guard->run([&] {
			if (c.remote_video) {
				c.remote_video(c.user, &frame);
			}
		});
	});

	auto descriptor = tgcalls::Descriptor{
		.version = version,
		.config = tgcalls::Config{
			.initializationTimeout = json["connect_timeout_ms"].number_value() / 1000.,
			.receiveTimeout = json["packet_timeout_ms"].number_value() / 1000.,
			.dataSaving = tgcalls::DataSaving::Never,
			.enableP2P = json["allow_p2p"].bool_value(),
			.enableAEC = false,
			.enableNS = true,
			.enableAGC = true,
			.enableVolumeControl = true,
			.maxApiLayer = json["max_layer"].int_value(),
			.customParameters = json["custom_parameters"].string_value(),
		},
		.encryptionKey = tgcalls::EncryptionKey(std::move(keyValue), json["outgoing"].bool_value()),
		.videoCapture = CaptureOf(video),
		.stateUpdated = [=](tgcalls::State state) {
			guard->run([&] {
				if (c.state) {
					c.state(c.user, StateToC(state));
				}
			});
		},
		.signalBarsUpdated = [=](int count) {
			guard->run([&] {
				if (c.signal_bars) {
					c.signal_bars(c.user, count);
				}
			});
		},
		.remoteBatteryLevelIsLowUpdated = [=](bool isLow) {
			guard->run([&] {
				if (c.remote_battery_low) {
					c.remote_battery_low(c.user, isLow ? 1 : 0);
				}
			});
		},
		.remoteMediaStateUpdated = [=](tgcalls::AudioState audio, tgcalls::VideoState video) {
			guard->run([&] {
				if (c.remote_media_state) {
					c.remote_media_state(c.user, (audio == tgcalls::AudioState::Active) ? 1 : 0, VideoStateToC(video));
				}
			});
		},
		.signalingDataEmitted = [=](const std::vector<uint8_t> &data) {
			guard->run([&] {
				if (c.signaling) {
					c.signaling(c.user, data.data(), data.size());
				}
			});
		},
		.createAudioDeviceModule = result->audio->creator(),
	};

	auto ids = std::set<int64_t>();
	for (const auto &server : servers) {
		if (server.reflector) {
			ids.insert(server.id);
		}
	}
	for (const auto &server : servers) {
		AppendEndpoint(descriptor.endpoints, server);
	}
	for (const auto &server : servers) {
		AppendServer(descriptor.rtcServers, server, ids);
	}

	if (const auto &proxy = json["proxy"]; proxy.is_object() && !proxy["host"].string_value().empty()) {
		descriptor.proxy = std::make_unique<tgcalls::Proxy>();
		descriptor.proxy->host = proxy["host"].string_value();
		descriptor.proxy->port = uint16_t(proxy["port"].int_value());
		descriptor.proxy->login = proxy["login"].string_value();
		descriptor.proxy->password = proxy["password"].string_value();
	}

	result->instance = tgcalls::Meta::Create(version, std::move(descriptor));
	if (!result->instance) {
		RTC_LOG(LS_ERROR) << "quill_calls: wrong library version: " << version;
		result->guard->close();
		return nullptr;
	}
	result->instance->setIncomingVideoOutput(result->remoteVideo);
	return result.release();
}

} // namespace
} // namespace quill_calls

extern "C" {

qc_p2p *qc_p2p_create(const char *config_json, const qc_p2p_callbacks *callbacks, qc_video_capture *video) {
	if (!config_json || !callbacks) {
		return nullptr;
	}
	return quill_calls::Create(config_json, *callbacks, video);
}

void qc_p2p_receive_signaling(qc_p2p *call, const uint8_t *data, size_t length) {
	if (call && data) {
		call->instance->receiveSignalingData(std::vector<uint8_t>(data, data + length));
	}
}

void qc_p2p_push_audio(qc_p2p *call, const int16_t *samples, size_t count) {
	if (call && samples) {
		call->audio->push(samples, count);
	}
}

void qc_p2p_set_muted(qc_p2p *call, int32_t muted) {
	if (call) {
		call->instance->setMuteMicrophone(muted != 0);
	}
}

void qc_p2p_set_video_capture(qc_p2p *call, qc_video_capture *video) {
	if (call) {
		call->instance->setVideoCapture(quill_calls::CaptureOf(video));
	}
}

void qc_p2p_set_requested_aspect(qc_p2p *call, float aspect) {
	if (call) {
		call->instance->setRequestedVideoAspect(aspect);
	}
}

void qc_p2p_set_low_battery(qc_p2p *call, int32_t is_low) {
	if (call) {
		call->instance->setIsLowBatteryLevel(is_low != 0);
	}
}

char *qc_p2p_debug_info(qc_p2p *call) {
	return call ? quill_calls::CopyForCaller(call->instance->getDebugInfo()) : nullptr;
}

char *qc_p2p_last_error(qc_p2p *call) {
	return call ? quill_calls::CopyForCaller(call->instance->getLastError()) : nullptr;
}

void qc_p2p_destroy(qc_p2p *call) {
	if (!call) {
		return;
	}
	call->guard->close();
	// As Telegram Desktop's Call::destroyController, except that this waits
	// for tgcalls to finish: it tears the call down on its shared threads,
	// and a process that exits meanwhile has those threads touch OpenSSL
	// and WebRTC statics already destroyed by exit handlers.
	std::promise<void> stopped;
	call->instance->stop([&stopped](tgcalls::FinalState) {
		stopped.set_value();
	});
	stopped.get_future().wait();
	call->instance = nullptr;
	// The instance's parts are released by tasks it posts to tgcalls'
	// threads, each of which may post to another; two passes over all
	// three run every such task through.
	const auto threads = tgcalls::StaticThreads::getThreads();
	for (auto pass = 0; pass != 2; ++pass) {
		for (const auto thread : {threads->getMediaThread(), threads->getNetworkThread(), threads->getWorkerThread()}) {
			thread->BlockingCall([] {
			});
		}
	}
	delete call;
}

} // extern "C"

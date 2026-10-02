# Basic P2P audio source setup

The native v3.0.0 adapter passed NULL audio device input for the default microphone and output. The native BaseDeviceModule parses input as JSON and rejects this with a media-device error; macOS additionally requires a UID. Resolve default macOS devices with valid metadata and an empty UID, which leaves AudioQueue on the current system default. Other platforms use actual enumerated metadata. Explicit selections keep their existing metadata.

P2PCall attaches incoming audio to the playback Microphone receiver, not the separate Speaker receiver. Put the output device description on that receiver and configure sources before connect: P2PCall's initial optimize_sources enables incoming audio only when that receiver already has a writer. Device changes and reconnects reuse this setup.

Validation: the actual macOS sidecar default-audio regression failed before the fix with ntg_set_stream_sources code -404 and passed afterward. It constructs readers before connecting, so it does not open capture devices or contact Telegram. The opt-in native integration also creates, destroys and recreates local P2P transports. These checks do not establish end-to-end call quality; a designated second account is still needed for that. No parity completion is declared.

Upstream contract: [BaseDeviceModule](https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/ntgcalls/src/media/devices/base_device_module.cpp), [MacAudioDeviceModule](https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/ntgcalls/src/media/devices/mac_audio_device_module.cpp), [P2PCall](https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/ntgcalls/src/instances/p2p_call.cpp), [StreamManager](https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/ntgcalls/src/media/stream_manager.cpp).

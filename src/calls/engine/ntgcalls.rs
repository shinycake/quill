//! `NtgcallsEngine`: runtime-loaded ntgcalls adapter implementing `CallEngine`.
use super::*;
use ntgcalls_sys::{
    Loader, NTG_ERR_INVALID_PARAMS, NTG_MEDIA_SOURCE_DESKTOP, NTG_MEDIA_SOURCE_DEVICE,
    NTG_MEDIA_SOURCE_EXTERNAL, NTG_OK, NTG_STREAM_MODE_CAPTURE, NTG_STREAM_MODE_PLAYBACK,
    ntg_instance, ntg_media_description, ntg_media_devices, ntg_protocol, ntg_ssrc_group,
    ntg_video_description,
};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::marker::PhantomData;
use std::ptr::{NonNull, null_mut};
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

/// Phase C2b: safe driver-thread adapter over the runtime-loaded C ABI.
///
/// `Rc` in the marker intentionally keeps this type `!Send`: the native
/// instance is created, used, and destroyed on the connect-driver thread.
pub struct NtgcallsEngine {
    api: Loader,
    instance: Option<NonNull<ntg_instance>>,
    protocol: EngineProtocol,
    call_to_user: HashMap<i32, i64>,
    call_media: HashMap<i32, CallMediaConfig>,
    /// Phase C2g: native group transport per TDLib group call id.
    group_calls: HashMap<i32, GroupCallMedia>,
    callback: Arc<CallbackShared>,
    _driver_thread_only: PhantomData<Rc<()>>,
}

impl NtgcallsEngine {
    /// Phase C2b: load the sidecar and cache its reported call protocol.
    pub fn load() -> Result<Self, EngineError> {
        let api = Loader::load_default().map_err(|_| EngineError::Unavailable)?;
        let mut raw: ntg_protocol = unsafe { std::mem::zeroed() };
        // SAFETY: `raw` is a valid writable C output and the loaded function
        // pointer remains live through `api`.
        let rc = unsafe { (api.ntg_get_protocol)(&mut raw) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_get_protocol",
                code: rc,
            });
        }
        let library_versions =
            c_string_pointer_array(raw.library_versions, raw.library_versions_len);
        let protocol = EngineProtocol {
            udp_p2p: raw.udp_p2p,
            udp_reflector: raw.udp_reflector,
            min_layer: raw.min_layer,
            max_layer: raw.max_layer,
            library_versions,
        };
        // SAFETY: ntgcalls allocated the successful protocol output and its
        // matching free function accepts the same stack wrapper by pointer.
        unsafe { (api.ntg_protocol_free)(&mut raw) };

        Ok(Self {
            api,
            instance: None,
            protocol,
            call_to_user: HashMap::new(),
            call_media: HashMap::new(),
            group_calls: HashMap::new(),
            callback: Arc::new(CallbackShared {
                user_to_call: Mutex::new(HashMap::new()),
                hook: Mutex::new(None),
                transport_hook: Mutex::new(None),
                frame_hook: Mutex::new(None),
                remote_video_hook: Mutex::new(None),
                remote_screen_hook: Mutex::new(None),
                remote_audio_hook: Mutex::new(None),
                frame_seq: AtomicU64::new(0),
                group_chat_to_call: Mutex::new(HashMap::new()),
                group_video_ssrc_to_user: Mutex::new(HashMap::new()),
            }),
            _driver_thread_only: PhantomData,
        })
    }

    fn ensure_instance(&mut self) -> Result<NonNull<ntg_instance>, EngineError> {
        if let Some(instance) = self.instance {
            return Ok(instance);
        }
        // SAFETY: the loaded constructor takes no arguments and ownership is
        // retained here until `Drop` calls the matching destroy function.
        let instance = unsafe { (self.api.ntg_instance_create)() };
        let instance = NonNull::new(instance).ok_or(EngineError::NullInstance)?;
        let user_data = Arc::as_ptr(&self.callback).cast_mut().cast::<c_void>();
        // SAFETY: `user_data` points to `self.callback`, which outlives the
        // registered callback. Drop unregisters before releasing the Arc.
        let rc = unsafe {
            (self.api.ntg_on_signaling_data_callback)(
                instance.as_ptr(),
                Some(signaling_trampoline),
                user_data,
            )
        };
        if rc != NTG_OK {
            // SAFETY: registration failed, so no callback can reference the
            // engine; destroy the freshly-created instance immediately.
            unsafe { (self.api.ntg_instance_destroy)(instance.as_ptr()) };
            return Err(EngineError::Engine {
                op: "ntg_on_signaling_data_callback",
                code: rc,
            });
        }
        let rc = unsafe {
            (self.api.ntg_on_connection_change_callback)(
                instance.as_ptr(),
                Some(connection_trampoline),
                user_data,
            )
        };
        if rc != NTG_OK {
            unsafe {
                let _ =
                    (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
                (self.api.ntg_instance_destroy)(instance.as_ptr());
            }
            return Err(EngineError::Engine {
                op: "ntg_on_connection_change_callback",
                code: rc,
            });
        }
        let rc = unsafe {
            (self.api.ntg_on_frames_callback)(instance.as_ptr(), Some(frames_trampoline), user_data)
        };
        if rc != NTG_OK {
            unsafe {
                let _ =
                    (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
                let _ = (self.api.ntg_on_connection_change_callback)(
                    instance.as_ptr(),
                    None,
                    null_mut(),
                );
                (self.api.ntg_instance_destroy)(instance.as_ptr());
            }
            return Err(EngineError::Engine {
                op: "ntg_on_frames_callback",
                code: rc,
            });
        }
        let rc = unsafe {
            (self.api.ntg_on_remote_source_change_callback)(
                instance.as_ptr(),
                Some(remote_source_trampoline),
                user_data,
            )
        };
        if rc != NTG_OK {
            unsafe {
                let _ =
                    (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
                let _ = (self.api.ntg_on_connection_change_callback)(
                    instance.as_ptr(),
                    None,
                    null_mut(),
                );
                let _ = (self.api.ntg_on_frames_callback)(instance.as_ptr(), None, null_mut());
                (self.api.ntg_instance_destroy)(instance.as_ptr());
            }
            return Err(EngineError::Engine {
                op: "ntg_on_remote_source_change_callback",
                code: rc,
            });
        }
        self.instance = Some(instance);
        Ok(instance)
    }

    fn user_id(&self, call_id: i32) -> Result<i64, EngineError> {
        self.call_to_user
            .get(&call_id)
            .copied()
            .ok_or(EngineError::NoSuchCall(call_id))
    }

    /// Phase C2e: re-issue stream sources from the retained per-call media
    /// config. Capture carries microphone + camera (the camera description
    /// is present only while video is enabled; NULL removes the camera
    /// reader and ntgcalls tells the peer via media_state video_stopped).
    /// Playback carries the speaker only.
    /// Phase C2g: (re)issue the group call's capture sources from the
    /// retained config. Group calls carry no audio transport in this
    /// slice — only the camera, and only while it is enabled.
    /// Put the group transport's microphone in the tracked mute state.
    fn apply_group_mute(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        if !media.connected {
            return Ok(());
        }
        let (chat_id, muted) = (media.chat_id, media.muted);
        let instance = self.ensure_instance()?;
        let mut state = false;
        // SAFETY: the instance and output pointer are valid for the
        // duration of this synchronous C call.
        let rc = unsafe {
            if muted {
                (self.api.ntg_mute)(instance.as_ptr(), chat_id, &mut state)
            } else {
                (self.api.ntg_unmute)(instance.as_ptr(), chat_id, &mut state)
            }
        };
        if rc == NTG_OK {
            Ok(())
        } else {
            Err(EngineError::Engine {
                op: if muted { "ntg_mute" } else { "ntg_unmute" },
                code: rc,
            })
        }
    }

    fn issue_group_sources(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get(&group_call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        let instance = self.ensure_instance()?;
        let camera = native_input(media.camera.as_deref())?;
        // The system's default microphone and output, as for 1:1 calls:
        // without them a voice chat is silent both ways.
        let mic = self.device_input(None, MediaDeviceKind::Microphone)?;
        let speaker = self.device_input(None, MediaDeviceKind::Speaker)?;
        let mut mic_audio = audio_description(mic.as_ref());
        let mut camera_video = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_DEVICE,
            // Telegram's group video default.
            width: 640,
            height: 480,
            fps: 30,
            input: camera
                .as_ref()
                .map_or(null_mut(), |value| value.as_ptr().cast_mut()),
            keep_open: false,
        };
        let capture = ntg_media_description {
            microphone: &mut mic_audio,
            speaker: null_mut(),
            camera: if media.camera_enabled {
                &mut camera_video
            } else {
                null_mut()
            },
            screen: null_mut(),
        };
        // SAFETY: `instance` is live and the descriptions are stack-local
        // for the duration of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                media.chat_id,
                NTG_STREAM_MODE_CAPTURE,
                capture,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        // Everyone else's voices: the call mixes them into its playback
        // "microphone" receiver, which names the output device.
        let mut speaker_audio = audio_description(speaker.as_ref());
        let playback = ntg_media_description {
            microphone: &mut speaker_audio,
            speaker: null_mut(),
            camera: null_mut(),
            screen: null_mut(),
        };
        // SAFETY: as above.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                media.chat_id,
                NTG_STREAM_MODE_PLAYBACK,
                playback,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        self.apply_group_mute(group_call_id)
    }

    /// Phase C2i: desktop-capture video description for screen-share
    /// send (1:1 and group presentation alike). The input is enumerated
    /// display metadata; desktop capture itself is ntgcalls' job
    /// (libwebrtc capturer) — Quill carries no Linux capture code.
    fn screen_video_description(input: Option<&CString>) -> ntg_video_description {
        ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_DESKTOP,
            width: 1920,
            height: 1080,
            fps: 15,
            input: input.map_or(null_mut(), |value| value.as_ptr().cast_mut()),
            keep_open: false,
        }
    }

    /// Phase C2g: attach desktop capture to the presentation transport
    /// after `ntg_connect(..., is_presentation=true)`. The presentation
    /// carries only the screen track; the main group transport is
    /// untouched.
    fn issue_presentation_sources(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get(&group_call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        let instance = self.ensure_instance()?;
        let screen = self.device_input(None, MediaDeviceKind::Screen)?;
        let mut screen_video = Self::screen_video_description(screen.as_ref());
        let capture = ntg_media_description {
            microphone: null_mut(),
            speaker: null_mut(),
            camera: null_mut(),
            screen: &mut screen_video,
        };
        // SAFETY: `instance` is live and the descriptions are stack-local
        // for the duration of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                media.chat_id,
                NTG_STREAM_MODE_CAPTURE,
                capture,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        Ok(())
    }

    fn set_media_sources(&self, call_id: i32) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get(&call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(call_id))?;
        let user_id = self.user_id(call_id)?;
        let instance = self.instance.ok_or(EngineError::NullInstance)?;
        let mic = self.device_input(config.mic.as_deref(), MediaDeviceKind::Microphone)?;
        let speaker = self.device_input(config.speaker.as_deref(), MediaDeviceKind::Speaker)?;
        let camera = native_input(config.camera.as_deref())?;
        let screen = if config.screen_share_on {
            self.device_input(None, MediaDeviceKind::Screen)?
        } else {
            None
        };

        let mut mic_audio = audio_description(mic.as_ref());
        let mut camera_video = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_DEVICE,
            // Conventional tgvoip P2P default: 640x480@30.
            width: 640,
            height: 480,
            fps: 30,
            // NULL input selects the default device.
            input: camera
                .as_ref()
                .map_or(null_mut(), |value| value.as_ptr().cast_mut()),
            keep_open: false,
        };
        let mut screen_video = Self::screen_video_description(screen.as_ref());
        let capture = ntg_media_description {
            microphone: &mut mic_audio,
            speaker: null_mut(),
            // Phase C2i: screen share replaces the camera — ntgcalls
            // rejects camera+screen in Capture mode
            // (`stream_manager.cpp:69`, v3.0.0).
            camera: if config.camera_enabled && !config.screen_share_on {
                &mut camera_video
            } else {
                null_mut()
            },
            screen: if config.screen_share_on {
                &mut screen_video
            } else {
                null_mut()
            },
        };
        // SAFETY: `instance` is live and the descriptions are stack-local for
        // the duration of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                user_id,
                NTG_STREAM_MODE_CAPTURE,
                capture,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }

        let mut speaker_audio = audio_description(speaker.as_ref());
        // External playback delivers decoded peer video to our frame callback.
        // NULL descriptions disable receiving these tracks altogether.
        let mut peer_camera = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_EXTERNAL,
            input: null_mut(),
            ..camera_video
        };
        let mut peer_screen = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_EXTERNAL,
            ..Self::screen_video_description(None)
        };
        let playback = ntg_media_description {
            // P2PCall attaches incoming audio to its Microphone receiver.
            // The description still identifies an output device.
            microphone: &mut speaker_audio,
            speaker: null_mut(),
            camera: &mut peer_camera,
            screen: &mut peer_screen,
        };
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                user_id,
                NTG_STREAM_MODE_PLAYBACK,
                playback,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        Ok(())
    }

    fn device_input(
        &self,
        selected: Option<&str>,
        kind: MediaDeviceKind,
    ) -> Result<Option<CString>, EngineError> {
        if selected.is_some() {
            return native_input(selected);
        }
        // ntgcalls requires device JSON, even for the system default.
        // An empty macOS UID leaves AudioQueue on the current default device.
        #[cfg(target_os = "macos")]
        if matches!(kind, MediaDeviceKind::Microphone | MediaDeviceKind::Speaker) {
            return native_input(Some(
                &serde_json::json!({
                    "is_microphone": kind == MediaDeviceKind::Microphone,
                    "uid": "",
                })
                .to_string(),
            ));
        }
        let default = self
            .media_devices()?
            .into_iter()
            .find(|device| device.kind == kind)
            .ok_or(EngineError::Engine {
                op: "ntg_get_media_devices",
                code: NTG_ERR_INVALID_PARAMS,
            })?
            .id;
        native_input(Some(&default))
    }
}

impl CallEngine for NtgcallsEngine {
    fn start_call(
        &mut self,
        call_id: i32,
        user_id: i64,
        _is_outgoing: bool,
    ) -> Result<(), EngineError> {
        if self.call_to_user.contains_key(&call_id) {
            return Ok(());
        }
        let instance = self.ensure_instance()?;
        // SAFETY: `instance` is live and ntgcalls keys P2P calls by user id.
        let rc = unsafe { (self.api.ntg_create_p2p_call)(instance.as_ptr(), user_id) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_create_p2p_call",
                code: rc,
            });
        }
        self.call_to_user.insert(call_id, user_id);
        self.callback
            .user_to_call
            .lock()
            .expect("ntgcalls callback map")
            .insert(user_id, call_id);
        Ok(())
    }

    fn accept_call(&mut self, call_id: i32) -> Result<(), EngineError> {
        self.user_id(call_id).map(|_| ())
    }

    fn send_signaling_data(&mut self, call_id: i32, data: &[u8]) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        if data.is_empty() {
            return Ok(());
        }
        let instance = self.ensure_instance()?;
        // SAFETY: `instance` is live and `data` remains valid for the duration
        // of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_send_signaling_data)(
                instance.as_ptr(),
                user_id,
                data.as_ptr(),
                data.len(),
            )
        };
        if rc == NTG_OK {
            Ok(())
        } else {
            Err(EngineError::Engine {
                op: "ntg_send_signaling_data",
                code: rc,
            })
        }
    }

    fn receive_signaling_data(&self, call_id: i32, data: &[u8]) {
        let hook = self
            .callback
            .hook
            .lock()
            .expect("ntgcalls callback hook")
            .clone();
        if let Some(hook) = hook {
            hook(call_id, data.to_vec());
        }
    }

    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback) {
        *self.callback.hook.lock().expect("ntgcalls callback hook") = Some(callback);
    }

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback) {
        *self
            .callback
            .transport_hook
            .lock()
            .expect("ntgcalls transport callback hook") = Some(callback);
    }

    fn set_video_frame_callback(&mut self, callback: VideoFrameCallback) {
        *self
            .callback
            .frame_hook
            .lock()
            .expect("ntgcalls video frame hook") = Some(callback);
    }

    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        *self
            .callback
            .remote_video_hook
            .lock()
            .expect("ntgcalls remote video state hook") = Some(callback);
    }

    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        *self
            .callback
            .remote_screen_hook
            .lock()
            .expect("ntgcalls remote screen state hook") = Some(callback);
    }

    fn set_remote_audio_state_callback(&mut self, callback: RemoteAudioStateCallback) {
        *self
            .callback
            .remote_audio_hook
            .lock()
            .expect("ntgcalls remote audio state hook") = Some(callback);
    }

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        let instance = self.ensure_instance()?;
        // Native P2P setup copies a fixed 256-byte Telegram auth key.
        if params.encryption_key.len() != 256 {
            return Err(EngineError::Engine {
                op: "ntg_skip_exchange",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        let servers = NativeRtcServers::new(&params.servers)?;
        let versions = c_strings(&params.library_versions)?;
        let custom_parameters = if params.custom_parameters.is_empty() {
            None
        } else {
            Some(native_string(&params.custom_parameters)?)
        };
        let version_ptrs: Vec<_> = versions.iter().map(|value| value.as_ptr()).collect();
        let previous = self.call_media.get(&call_id).cloned();
        if previous.is_some() {
            // Native P2P transports cannot connect or exchange keys twice.
            // Stop the failed transport and restore the same TDLib call mapping.
            self.hangup(call_id)?;
            self.start_call(call_id, user_id, params.is_outgoing)?;
        }
        // Retain current device/camera/screen choices even after a failed attempt.
        // Their presence also identifies a subsequent transport retry.
        let config = previous.unwrap_or_else(|| retained_call_media(params, None));
        self.call_media.insert(call_id, config);
        let rc = unsafe {
            (self.api.ntg_skip_exchange)(
                instance.as_ptr(),
                user_id,
                params.encryption_key.as_ptr(),
                params.encryption_key.len(),
                params.is_outgoing,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_skip_exchange",
                code: rc,
            });
        }
        // P2P connect enables incoming audio from the already configured
        // playback sources. Configuring them afterwards leaves it disabled.
        self.set_media_sources(call_id)?;
        let rc = unsafe {
            (self.api.ntg_connect_p2p)(
                instance.as_ptr(),
                user_id,
                servers.raw.as_ptr(),
                servers.raw.len(),
                version_ptrs.as_ptr(),
                version_ptrs.len(),
                params.p2p_allowed,
                custom_parameters
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_connect_p2p",
                code: rc,
            });
        }
        Ok(())
    }

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get_mut(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))?;
        config.mic = microphone.map(str::to_owned);
        config.speaker = speaker.map(str::to_owned);
        self.set_media_sources(call_id)
    }

    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError> {
        let Some(user_id) = self.call_to_user.remove(&call_id) else {
            return Ok(());
        };
        self.call_media.remove(&call_id);
        self.callback
            .user_to_call
            .lock()
            .expect("ntgcalls callback map")
            .remove(&user_id);
        if let Some(instance) = self.instance {
            // SAFETY: best-effort teardown on a live instance. The return
            // value is intentionally ignored so hangup remains idempotent.
            unsafe { (self.api.ntg_stop)(instance.as_ptr(), user_id) };
        }
        Ok(())
    }

    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        let instance = self.ensure_instance()?;
        let mut state = false;
        // SAFETY: the instance and output pointer are valid for the duration
        // of this synchronous C call.
        let rc = unsafe {
            if muted {
                (self.api.ntg_mute)(instance.as_ptr(), user_id, &mut state)
            } else {
                (self.api.ntg_unmute)(instance.as_ptr(), user_id, &mut state)
            }
        };
        if rc == NTG_OK {
            Ok(())
        } else {
            Err(EngineError::Engine {
                op: if muted { "ntg_mute" } else { "ntg_unmute" },
                code: rc,
            })
        }
    }

    fn set_camera_enabled(
        &mut self,
        call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get_mut(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))?;
        let previous_enabled = config.camera_enabled;
        let previous_camera = config.camera.clone();
        let previous_screen_share = config.screen_share_on;
        config.camera_enabled = enabled;
        config.camera = camera.map(str::to_owned);
        // Phase C2i: the camera replaces screen share — ntgcalls
        // rejects camera+screen in Capture mode
        // (`stream_manager.cpp:69`, v3.0.0).
        if enabled {
            config.screen_share_on = false;
        }
        if let Err(err) = self.set_media_sources(call_id) {
            // Phase C2e: restore the retained config so it can't desync
            // from ntgcalls when issuance fails.
            let config = self
                .call_media
                .get_mut(&call_id)
                .ok_or(EngineError::NoSuchCall(call_id))?;
            config.camera_enabled = previous_enabled;
            config.camera = previous_camera;
            config.screen_share_on = previous_screen_share;
            return Err(err);
        }
        Ok(())
    }

    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get_mut(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))?;
        let previous_enabled = config.screen_share_on;
        let previous_camera_enabled = config.camera_enabled;
        config.screen_share_on = enabled;
        if enabled {
            config.camera_enabled = false;
        }
        if let Err(err) = self.set_media_sources(call_id) {
            // Restore the retained config so it can't desync from
            // ntgcalls when issuance fails (mirrors
            // `set_camera_enabled`).
            let config = self
                .call_media
                .get_mut(&call_id)
                .ok_or(EngineError::NoSuchCall(call_id))?;
            config.screen_share_on = previous_enabled;
            config.camera_enabled = previous_camera_enabled;
            return Err(err);
        }
        Ok(())
    }

    fn create_group_call(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
    ) -> Result<String, EngineError> {
        let instance = self.ensure_instance()?;
        let mut out: *mut c_char = null_mut();
        // SAFETY: `out` is a valid writable C output; ntgcalls allocates the
        // returned join-payload string on success.
        let rc = unsafe { (self.api.ntg_create_call)(instance.as_ptr(), chat_id, &mut out) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_create_call",
                code: rc,
            });
        }
        if out.is_null() {
            return Err(EngineError::Engine {
                op: "ntg_create_call",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        // SAFETY: `out` points at a NUL-terminated string allocated by the
        // call above; copy it then free with the matching free function.
        let offer = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.api.ntg_string_free)(out) };
        self.group_calls.insert(
            group_call_id,
            GroupCallMedia {
                chat_id,
                ..GroupCallMedia::default()
            },
        );
        self.callback
            .group_chat_to_call
            .lock()
            .expect("ntgcalls group call map")
            .insert(chat_id, group_call_id);
        Ok(offer)
    }

    fn set_group_muted(&mut self, group_call_id: i32, muted: bool) -> Result<(), EngineError> {
        self.group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .muted = muted;
        self.apply_group_mute(group_call_id)
    }

    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        video_enabled: bool,
    ) -> Result<(), EngineError> {
        let (chat_id, already) = {
            let media = self
                .group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?;
            (media.chat_id, media.connected)
        };
        if already {
            return Ok(());
        }
        let instance = self.ensure_instance()?;
        let params = CString::new(answer_payload).map_err(|_| EngineError::Engine {
            op: "ntg_connect",
            code: NTG_ERR_INVALID_PARAMS,
        })?;
        // SAFETY: instance is live and `params` is a valid C string; the
        // TDLib answer is the `Text` from `joinVideoChat`.
        let rc =
            unsafe { (self.api.ntg_connect)(instance.as_ptr(), chat_id, params.as_ptr(), false) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_connect",
                code: rc,
            });
        }
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        media.connected = true;
        media.camera_enabled = video_enabled;
        self.issue_group_sources(group_call_id)
    }

    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        let instance = self.ensure_instance()?;
        let chat_id = self
            .group_calls
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .chat_id;
        // Add new endpoints first so the video never blips on reorder.
        let mut desired: HashMap<&str, &GroupVideoSource> = HashMap::new();
        for source in sources {
            desired.insert(source.endpoint.as_str(), source);
        }
        let stale: Vec<String> = {
            let media = self
                .group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?;
            media
                .endpoints
                .keys()
                .filter(|endpoint| !desired.contains_key(endpoint.as_str()))
                .cloned()
                .collect()
        };
        for source in sources {
            let stored = self
                .group_calls
                .get(&group_call_id)
                .and_then(|media| media.endpoints.get(&source.endpoint))
                .cloned();
            match stored {
                // Subscription matches; nothing to do.
                Some(existing) if existing == *source => continue,
                // Ssrc groups changed; drop the stale subscription so the
                // add below re-issues it with the fresh groups.
                Some(_) => {
                    self.remove_group_video_endpoint(group_call_id, chat_id, &source.endpoint)?;
                }
                None => {}
            }
            let endpoint =
                CString::new(source.endpoint.as_str()).map_err(|_| EngineError::Engine {
                    op: "ntg_add_incoming_video",
                    code: NTG_ERR_INVALID_PARAMS,
                })?;
            let groups: Vec<ntg_ssrc_group> = source
                .ssrc_groups
                .iter()
                .map(|group| ntg_ssrc_group {
                    semantics: CString::new(group.semantics.as_str())
                        .map(|s| s.into_raw())
                        .unwrap_or(null_mut()),
                    ssrcs: group.ssrcs.as_ptr().cast_mut(),
                    ssrcs_len: group.ssrcs.len(),
                })
                .collect();
            let mut sink: u32 = 0;
            // SAFETY: instance is live; `endpoint` and the group metadata
            // are valid for this call; `sink` is a valid C output.
            let rc = unsafe {
                (self.api.ntg_add_incoming_video)(
                    instance.as_ptr(),
                    chat_id,
                    source.user_id,
                    endpoint.as_ptr(),
                    groups.as_ptr(),
                    groups.len(),
                    &mut sink,
                )
            };
            // The semantics C strings were `into_raw` above; reclaim them.
            for group in &groups {
                if !group.semantics.is_null() {
                    let _ = unsafe { CString::from_raw(group.semantics) };
                }
            }
            if rc != NTG_OK {
                return Err(EngineError::Engine {
                    op: "ntg_add_incoming_video",
                    code: rc,
                });
            }
            {
                let media = self
                    .group_calls
                    .get_mut(&group_call_id)
                    .ok_or(EngineError::NoSuchCall(group_call_id))?;
                media
                    .endpoints
                    .insert(source.endpoint.clone(), source.clone());
                for group in &source.ssrc_groups {
                    for ssrc in &group.ssrcs {
                        media.ssrc_to_user.insert((chat_id, *ssrc), source.user_id);
                    }
                }
            }
            self.callback
                .group_video_ssrc_to_user
                .lock()
                .expect("ntgcalls group ssrc map")
                .extend(
                    source
                        .ssrc_groups
                        .iter()
                        .flat_map(|group| group.ssrcs.iter())
                        .map(|ssrc| ((chat_id, *ssrc), source.user_id)),
                );
        }
        for endpoint in stale {
            self.remove_group_video_endpoint(group_call_id, chat_id, &endpoint)?;
        }
        // Prune the callback ssrc map of entries for removed endpoints.
        // Keys are (chat_id, ssrc), so entries for other group calls
        // (other chat ids) are left untouched.
        {
            let media = self
                .group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?;
            let live = media.ssrc_to_user.clone();
            self.callback
                .group_video_ssrc_to_user
                .lock()
                .expect("ntgcalls group ssrc map")
                .retain(|key, user| key.0 != chat_id || live.get(key) == Some(user));
        }
        Ok(())
    }

    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        media.camera_enabled = enabled;
        media.camera = camera.map(str::to_owned);
        self.issue_group_sources(group_call_id)
    }

    fn start_screen_share(&mut self, group_call_id: i32) -> Result<String, EngineError> {
        let instance = self.ensure_instance()?;
        let chat_id = self
            .group_calls
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .chat_id;
        let mut out: *mut c_char = null_mut();
        // SAFETY: `out` is a valid writable C output; ntgcalls allocates
        // the returned presentation offer on success.
        let rc = unsafe { (self.api.ntg_init_presentation)(instance.as_ptr(), chat_id, &mut out) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_init_presentation",
                code: rc,
            });
        }
        if out.is_null() {
            return Err(EngineError::Engine {
                op: "ntg_init_presentation",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        // SAFETY: as in `create_group_call`.
        let offer = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.api.ntg_string_free)(out) };
        if let Some(media) = self.group_calls.get_mut(&group_call_id) {
            media.presentation_initialized = true;
        }
        Ok(offer)
    }

    fn connect_screen_share(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
    ) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        if media.screen_sharing {
            return Ok(());
        }
        let chat_id = media.chat_id;
        let instance = self.ensure_instance()?;
        let params = CString::new(answer_payload).map_err(|_| EngineError::Engine {
            op: "ntg_connect",
            code: NTG_ERR_INVALID_PARAMS,
        })?;
        // SAFETY: instance is live and `params` is valid; the answer is the
        // `Text` from `startGroupCallScreenSharing`.
        let rc =
            unsafe { (self.api.ntg_connect)(instance.as_ptr(), chat_id, params.as_ptr(), true) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_connect",
                code: rc,
            });
        }
        self.group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .screen_sharing = true;
        self.issue_presentation_sources(group_call_id)
    }

    fn stop_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let (instance, chat_id) = (
            self.ensure_instance()?,
            self.group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?
                .chat_id,
        );
        // SAFETY: instance is live; `chat_id` is this group's chat.
        let rc = unsafe { (self.api.ntg_stop_presentation)(instance.as_ptr(), chat_id) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_stop_presentation",
                code: rc,
            });
        }
        if let Some(media) = self.group_calls.get_mut(&group_call_id) {
            media.screen_sharing = false;
            media.presentation_initialized = false;
        }
        Ok(())
    }

    fn presentation_active(&self, group_call_id: i32) -> bool {
        self.group_calls
            .get(&group_call_id)
            .is_some_and(|media| media.screen_sharing || media.presentation_initialized)
    }

    fn leave_group_call(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        // Unknown group call ids succeed: leave is cleanup, never an error.
        let Some(media) = self.group_calls.remove(&group_call_id) else {
            return Ok(());
        };
        // Routing leaves with the call even when a native stop below fails
        // (the early returns would otherwise strand the ssrc map).
        self.callback.forget_group(media.chat_id);
        if let Some(instance) = self.instance {
            // Privacy: tear a live presentation down FIRST so screen
            // capture stops before the call itself.
            if media.screen_sharing || media.presentation_initialized {
                // SAFETY: instance is live; `media.chat_id` is this group's chat.
                let rc =
                    unsafe { (self.api.ntg_stop_presentation)(instance.as_ptr(), media.chat_id) };
                if rc != NTG_OK {
                    return Err(EngineError::Engine {
                        op: "ntg_stop_presentation",
                        code: rc,
                    });
                }
            }
            // SAFETY: instance is live; `media.chat_id` is this group's chat.
            let rc = unsafe { (self.api.ntg_stop)(instance.as_ptr(), media.chat_id) };
            if rc != NTG_OK {
                return Err(EngineError::Engine {
                    op: "ntg_stop",
                    code: rc,
                });
            }
        }
        Ok(())
    }

    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError> {
        let mut raw: ntg_media_devices = unsafe { std::mem::zeroed() };
        // SAFETY: device enumeration needs no instance and writes to `raw`.
        let rc = unsafe { (self.api.ntg_get_media_devices)(&mut raw) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_get_media_devices",
                code: rc,
            });
        }
        let mut devices = Vec::new();
        append_devices(
            &mut devices,
            raw.microphone,
            raw.microphone_len,
            MediaDeviceKind::Microphone,
        );
        append_devices(
            &mut devices,
            raw.speaker,
            raw.speaker_len,
            MediaDeviceKind::Speaker,
        );
        append_devices(
            &mut devices,
            raw.camera,
            raw.camera_len,
            MediaDeviceKind::Camera,
        );
        append_devices(
            &mut devices,
            raw.screen,
            raw.screen_len,
            MediaDeviceKind::Screen,
        );
        // SAFETY: successful enumeration owns the nested allocations until
        // released with the matching ntgcalls free function.
        unsafe { (self.api.ntg_media_devices_free)(&mut raw) };
        Ok(devices)
    }

    fn protocol(&self) -> EngineProtocol {
        self.protocol.clone()
    }

    fn is_available(&self) -> bool {
        true
    }
}

impl NtgcallsEngine {
    /// Phase C2g: remove one group video endpoint subscription. Only
    /// that endpoint's ssrcs leave the attribution maps, so another
    /// endpoint of the same user keeps receiving frames.
    fn remove_group_video_endpoint(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
        endpoint: &str,
    ) -> Result<(), EngineError> {
        let instance = self.ensure_instance()?;
        let native_endpoint = CString::new(endpoint).map_err(|_| EngineError::Engine {
            op: "ntg_remove_incoming_video",
            code: NTG_ERR_INVALID_PARAMS,
        })?;
        let mut removed: bool = false;
        // SAFETY: instance is live; `native_endpoint` is valid for this
        // call; `removed` is a valid C output.
        let rc = unsafe {
            (self.api.ntg_remove_incoming_video)(
                instance.as_ptr(),
                chat_id,
                native_endpoint.as_ptr(),
                &mut removed,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_remove_incoming_video",
                code: rc,
            });
        }
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        if let Some(stored) = media.endpoints.remove(endpoint) {
            let ssrcs: Vec<u32> = stored
                .ssrc_groups
                .iter()
                .flat_map(|group| group.ssrcs.iter().copied())
                .collect();
            for ssrc in ssrcs {
                media.ssrc_to_user.remove(&(chat_id, ssrc));
            }
        }
        Ok(())
    }
}

impl Drop for NtgcallsEngine {
    fn drop(&mut self) {
        let Some(instance) = self.instance.take() else {
            return;
        };
        // SAFETY: unregister the callback before destroying the instance, so
        // no new native callbacks can fire after teardown begins; the Arc
        // field keeps `CallbackShared` alive until the struct is fully
        // dropped.
        unsafe {
            let _ = (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
            let _ =
                (self.api.ntg_on_connection_change_callback)(instance.as_ptr(), None, null_mut());
            let _ = (self.api.ntg_on_frames_callback)(instance.as_ptr(), None, null_mut());
            let _ = (self.api.ntg_on_remote_source_change_callback)(
                instance.as_ptr(),
                None,
                null_mut(),
            );
        }
        self.call_to_user.clear();
        self.callback
            .user_to_call
            .lock()
            .expect("ntgcalls callback map")
            .clear();
        // SAFETY: callback registration is gone and this is the matching
        // destroy for the constructor used by `ensure_instance`.
        unsafe { (self.api.ntg_instance_destroy)(instance.as_ptr()) };
    }
}

#[cfg(test)]
mod native_tests {
    use super::*;

    #[test]
    fn native_default_audio_sources_before_connect() {
        if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
            return;
        }
        let mut engine = NtgcallsEngine::load().expect("Native engine");
        engine.start_call(1, 42, true).expect("Local transport");
        let screen = engine
            .device_input(None, MediaDeviceKind::Screen)
            .expect("Enumerated screen");
        let screen = screen.expect("Screen metadata");
        let metadata: serde_json::Value = serde_json::from_slice(screen.as_bytes()).unwrap();
        assert!(
            metadata.get("id").is_some(),
            "Desktop capturer requires a source ID"
        );
        let description = NtgcallsEngine::screen_video_description(Some(&screen));
        assert_eq!(
            unsafe { CStr::from_ptr(description.input) },
            screen.as_c_str()
        );
        let mut params = ConnectParams {
            encryption_key: Vec::new(),
            custom_parameters: String::new(),
            is_outgoing: true,
            servers: Vec::new(),
            library_versions: Vec::new(),
            p2p_allowed: false,
            mic_input: None,
            speaker_input: None,
            video_enabled: false,
            camera_input: None,
        };
        for length in [0, 1, 255, 257] {
            params.encryption_key = vec![1; length];
            assert!(matches!(
                engine.connect(1, &params),
                Err(EngineError::Engine {
                    op: "ntg_skip_exchange",
                    code: NTG_ERR_INVALID_PARAMS
                })
            ));
        }
        engine.call_media.insert(
            1,
            CallMediaConfig {
                mic: None,
                speaker: None,
                camera: None,
                camera_enabled: false,
                screen_share_on: false,
            },
        );
        // Before connect, ntgcalls constructs readers without opening them.
        // This checks device metadata without recording or making a call.
        engine.set_media_sources(1).expect("Default audio sources");
        let key = [1_u8; 256];
        let instance = engine.instance.expect("Live native instance");
        // Exercise the native operation reported in the user's screenshot.
        // No connection follows, so these synthetic bytes never leave the process.
        let result = unsafe {
            (engine.api.ntg_skip_exchange)(instance.as_ptr(), 42, key.as_ptr(), key.len(), true)
        };
        assert_eq!(
            result, NTG_OK,
            "Native key exchange must find the local call"
        );
        params.encryption_key = key.to_vec();
        // Unsupported versions fail before creating a network connection.
        params.library_versions = vec!["unsupported-offline-test".into()];
        for _ in 0..2 {
            assert!(
                matches!(
                    engine.connect(1, &params),
                    Err(EngineError::Engine {
                        op: "ntg_connect_p2p",
                        ..
                    })
                ),
                "Retry must replace the previous native key exchange"
            );
        }
        engine.hangup(1).expect("Teardown");
    }
}

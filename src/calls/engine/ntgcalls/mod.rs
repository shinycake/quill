//! `NtgcallsEngine`: runtime-loaded ntgcalls adapter implementing `CallEngine`.
use super::*;
use crate::locks::LockExt;
use ntgcalls_sys::{
    Loader, NTG_CONNECTION_MODE_NONE, NTG_ERR_INVALID_PARAMS, NTG_MEDIA_SOURCE_DESKTOP,
    NTG_MEDIA_SOURCE_DEVICE, NTG_MEDIA_SOURCE_EXTERNAL, NTG_OK, NTG_STREAM_MODE_CAPTURE,
    NTG_STREAM_MODE_PLAYBACK, ntg_connection_mode, ntg_instance, ntg_media_description,
    ntg_media_devices, ntg_protocol, ntg_ssrc_group, ntg_video_description,
};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::marker::PhantomData;
use std::ptr::{NonNull, null_mut};
use std::rc::Rc;
use std::sync::Arc;

mod call_engine;
mod call_sound;
mod external;
mod log;
mod stream_audio;

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
    /// Feeder threads' access to the native instance (`external.rs`).
    external_gate: Arc<external::ExternalGate>,
    stream_outputs: HashMap<i32, stream_audio::StreamOutput>,
    stream_volume: f32,
    /// 1:1 calls whose sound Quill plays, by call id (`call_sound.rs`).
    call_outputs: HashMap<i32, stream_audio::StreamOutput>,
    /// 1:1 calls moved back to the device's output.
    device_call_sound: std::collections::HashSet<i32>,
    _driver_thread_only: PhantomData<Rc<()>>,
}

/// The library, mapped by [`preload_ntgcalls`] on a worker and kept loaded,
/// so the engine's own load on the UI thread only resolves its symbols.
static PRELOADED: std::sync::OnceLock<Option<Loader>> = std::sync::OnceLock::new();

/// Load the ntgcalls library off the UI thread, ahead of the first engine.
pub fn preload_ntgcalls() {
    PRELOADED.get_or_init(|| Loader::load_default().ok());
}

impl NtgcallsEngine {
    /// Phase C2b: load the sidecar and cache its reported call protocol.
    pub fn load() -> Result<Self, EngineError> {
        let api = Loader::load_default().map_err(|_| EngineError::Unavailable)?;
        // SAFETY: `ntg_protocol` is a C struct of integers, bools and pointers;
        // all-zero (null pointers, zero lengths) is a valid value.
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
        log::install(&api);

        Ok(Self {
            api,
            instance: None,
            protocol,
            call_to_user: HashMap::new(),
            call_media: HashMap::new(),
            group_calls: HashMap::new(),
            callback: Arc::new(CallbackShared::new()),
            external_gate: Arc::default(),
            stream_outputs: HashMap::new(),
            stream_volume: 1.0,
            call_outputs: HashMap::new(),
            device_call_sound: std::collections::HashSet::new(),
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
        if let Err(err) = self.register_callbacks(instance, user_data) {
            // SAFETY: `instance` is live and owned here; clearing every
            // callback (registered or not) and then destroying it is the
            // documented teardown, so no callback can outlive this call.
            unsafe {
                self.clear_callbacks(instance);
                (self.api.ntg_instance_destroy)(instance.as_ptr());
            }
            return Err(err);
        }
        self.external_gate
            .attach(instance, self.api.ntg_send_external_frame);
        self.instance = Some(instance);
        Ok(instance)
    }

    /// Registers every instance-wide callback. ntgcalls callbacks carry
    /// the chat or user id, so each is registered once per instance, not
    /// per call. Stops at the first failure.
    fn register_callbacks(
        &self,
        instance: NonNull<ntg_instance>,
        user_data: *mut c_void,
    ) -> Result<(), EngineError> {
        let check = |op: &'static str, rc: i32| {
            if rc == NTG_OK {
                Ok(())
            } else {
                Err(EngineError::Engine { op, code: rc })
            }
        };
        let raw = instance.as_ptr();
        // SAFETY: (all registrations below) `raw` is the live instance and
        // `user_data` points to `self.callback`, which outlives every
        // registered callback: Drop clears them before the Arc goes. Each
        // trampoline catches panics before returning to C.
        check("ntg_on_signaling_data_callback", unsafe {
            (self.api.ntg_on_signaling_data_callback)(raw, Some(signaling_trampoline), user_data)
        })?;
        // SAFETY: as above.
        check("ntg_on_connection_change_callback", unsafe {
            (self.api.ntg_on_connection_change_callback)(
                raw,
                Some(connection_trampoline),
                user_data,
            )
        })?;
        // SAFETY: as above.
        check("ntg_on_frames_callback", unsafe {
            (self.api.ntg_on_frames_callback)(raw, Some(frames_trampoline), user_data)
        })?;
        // SAFETY: as above.
        check("ntg_on_remote_source_change_callback", unsafe {
            (self.api.ntg_on_remote_source_change_callback)(
                raw,
                Some(remote_source_trampoline),
                user_data,
            )
        })?;
        // SAFETY: as above. Stream mode: ntgcalls asks for the stream's
        // current time (RTMP only) and for segment parts.
        check("ntg_on_request_broadcast_timestamp_callback", unsafe {
            (self.api.ntg_on_request_broadcast_timestamp_callback)(
                raw,
                Some(broadcast_timestamp_trampoline),
                user_data,
            )
        })?;
        // SAFETY: as above.
        check("ntg_on_request_broadcast_part_callback", unsafe {
            (self.api.ntg_on_request_broadcast_part_callback)(
                raw,
                Some(broadcast_part_trampoline),
                user_data,
            )
        })
    }

    /// Clears every instance-wide callback so no native callback can fire
    /// into `self.callback` afterwards. Results are ignored: clearing is
    /// teardown.
    ///
    /// # Safety
    ///
    /// `instance` must be the live instance this engine created.
    unsafe fn clear_callbacks(&self, instance: NonNull<ntg_instance>) {
        let raw = instance.as_ptr();
        // SAFETY: the caller guarantees `raw` is live; a null callback with
        // null user data is ntgcalls' documented way to unregister.
        unsafe {
            let _ = (self.api.ntg_on_signaling_data_callback)(raw, None, null_mut());
            let _ = (self.api.ntg_on_connection_change_callback)(raw, None, null_mut());
            let _ = (self.api.ntg_on_frames_callback)(raw, None, null_mut());
            let _ = (self.api.ntg_on_remote_source_change_callback)(raw, None, null_mut());
            let _ = (self.api.ntg_on_request_broadcast_timestamp_callback)(raw, None, null_mut());
            let _ = (self.api.ntg_on_request_broadcast_part_callback)(raw, None, null_mut());
        }
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
        if !media.connected || media.role == GroupCallRole::Viewer {
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

    /// The mode ntgcalls chose for a connected group call. A failed query
    /// is logged and read as RTC, the mode every call had before streams.
    fn read_connection_mode(&self, instance: NonNull<ntg_instance>, chat_id: i64) -> GroupCallMode {
        let mut raw: ntg_connection_mode = NTG_CONNECTION_MODE_NONE;
        // SAFETY: the instance is live and `raw` is a valid C output for
        // this synchronous call.
        let rc =
            unsafe { (self.api.ntg_get_connection_mode)(instance.as_ptr(), chat_id, &mut raw) };
        if rc != NTG_OK {
            eprintln!("quill: ntg_get_connection_mode failed for chat {chat_id}: {rc}");
            return GroupCallMode::Rtc;
        }
        GroupCallMode::from_native(raw).unwrap_or(GroupCallMode::Rtc)
    }

    fn issue_group_sources(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get(&group_call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        if media.role == GroupCallRole::Viewer {
            return self.issue_group_playback(group_call_id);
        }
        let instance = self.ensure_instance()?;
        let camera = native_input(media.camera.as_deref())?;
        // The system's default microphone, as for 1:1 calls: without it
        // a voice chat is silent for the others. The output is issued
        // with the playback (`issue_group_playback`).
        let mic = self.capture_microphone_input(None)?;
        let mut mic_audio =
            external::capture_microphone_description(external_capture_enabled(), mic.as_ref());
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
            camera: if camera.is_some() {
                &mut camera_video
            } else {
                null_mut()
            },
            screen: null_mut(),
        };
        // Both stream modes are receive-only, so no capture source (and
        // no microphone) is opened for a listener. RTMP would also throw
        // `RTMPStreamingUnsupported` for one (`group_call.cpp`,
        // `set_stream_sources`). An MTProto stream carries nothing
        // outgoing: ntgcalls only sends media again after a new connect
        // with an RTC answer, which issues capture then.
        if !media.mode.is_stream() {
            // SAFETY: `instance` is live and the descriptions are
            // stack-local for the duration of this synchronous C call.
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
        }
        self.issue_group_playback(group_call_id)
    }

    /// The playback half of [`Self::issue_group_sources`], also re-issued
    /// alone when the output device or the local mute changes. ntgcalls
    /// rebuilds the audio writer for a changed device and drops it for a
    /// missing description (`StreamManager::maybe_reconfigure_device`):
    /// with no writer the call's sound is not played, which is the local
    /// mute (ntgcalls' own mute only covers capture).
    ///
    /// A chosen output that ntgcalls refuses (a device unplugged since it
    /// was picked) falls back to the system default once, so a stale
    /// choice never fails a connect or a camera toggle.
    fn issue_group_playback(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let chosen = self
            .group_calls
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .speaker
            .clone();
        let fell_back = playback_with_fallback(chosen.as_deref(), |speaker| {
            self.issue_group_playback_on(group_call_id, speaker)
        })?;
        if fell_back && let Some(media) = self.group_calls.get_mut(&group_call_id) {
            media.speaker = None;
        }
        self.apply_group_mute(group_call_id)
    }

    /// One `ntg_set_stream_sources(PLAYBACK)` on `speaker` (`None`: the
    /// system default).
    fn issue_group_playback_on(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get(&group_call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        let instance = self.ensure_instance()?;
        let external = self.open_stream_sound(group_call_id, speaker);
        let speaker_input = match external {
            Some(_) => None,
            None => self.device_input(speaker, MediaDeviceKind::Speaker)?,
        };
        // Everyone else's voices: the call mixes them into its playback
        // "microphone" receiver, which names the output device.
        let mut speaker_audio =
            external.unwrap_or_else(|| audio_description(speaker_input.as_ref()));
        // Every picture (a stream's, and participants' camera and screen
        // share in a normal video chat) reaches the frames callback only
        // through external playback writers: ntgcalls v3.0.3
        // `stream_manager.cpp` `optimize_sources` gates
        // `enable_video_incoming` on them, and `setup_video_playback_callbacks`
        // drops frames without one. NULL descriptions would silently drop
        // all incoming video, so RTC calls pass them too.
        let (mut playback_camera, mut playback_screen) = Self::external_playback_video();
        let writer_off = media.playback_muted && external.is_none();
        let playback = ntg_media_description {
            microphone: if writer_off {
                null_mut()
            } else {
                &mut speaker_audio
            },
            speaker: null_mut(),
            camera: &mut playback_camera,
            screen: &mut playback_screen,
        };
        // SAFETY: `instance` is live and the descriptions are stack-local
        // for the duration of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                media.chat_id,
                NTG_STREAM_MODE_PLAYBACK,
                playback,
            )
        };
        if rc != NTG_OK && external.is_some() {
            eprintln!(
                "quill: warning: ntgcalls refused external stream sound ({rc}); playing group call {group_call_id} on the device"
            );
            self.force_device_playback(group_call_id);
            return self.issue_group_playback_on(group_call_id, speaker);
        }
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        Ok(())
    }

    /// External playback descriptions (camera, screen) that make ntgcalls
    /// enable incoming video and hand decoded frames to the callback.
    fn external_playback_video() -> (ntg_video_description, ntg_video_description) {
        let camera = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_EXTERNAL,
            ..Self::screen_video_description(None)
        };
        (camera, camera)
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
        let screen = self.capture_screen_input()?;
        let mut screen_video = external::capture_screen_description(
            external_screen_capture_enabled(),
            screen.as_ref(),
        );
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

    fn set_media_sources(&mut self, call_id: i32) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get(&call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(call_id))?;
        let user_id = self.user_id(call_id)?;
        let instance = self.instance.ok_or(EngineError::NullInstance)?;
        let mic = self.capture_microphone_input(config.mic.as_deref())?;
        let speaker = self.device_input(config.speaker.as_deref(), MediaDeviceKind::Speaker)?;
        let camera = native_input(config.camera.as_deref())?;
        let screen = if config.screen_share_on {
            self.capture_screen_input()?
        } else {
            None
        };

        let mut mic_audio =
            external::capture_microphone_description(external_capture_enabled(), mic.as_ref());
        let mut camera_video = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_DEVICE,
            // Conventional tgvoip P2P default: 640x480@30.
            width: 640,
            height: 480,
            fps: 30,
            // ntgcalls opens the named device only; a NULL input fails.
            input: camera
                .as_ref()
                .map_or(null_mut(), |value| value.as_ptr().cast_mut()),
            keep_open: false,
        };
        let mut screen_video = external::capture_screen_description(
            external_screen_capture_enabled(),
            screen.as_ref(),
        );
        let capture = ntg_media_description {
            microphone: &mut mic_audio,
            speaker: null_mut(),
            // Phase C2i: screen share replaces the camera — ntgcalls
            // rejects camera+screen in Capture mode
            // (`stream_manager.cpp:69`, v3.0.0).
            camera: if camera.is_some() && !config.screen_share_on {
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
        crate::advanced::debug_mode::debug_log(&format!(
            "[call] {call_id} capture camera {:?} screen {} -> {rc}",
            config.camera, config.screen_share_on
        ));
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }

        let mut speaker_audio = self
            .open_call_sound(call_id, user_id, config.speaker.as_deref())
            .unwrap_or_else(|| audio_description(speaker.as_ref()));
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
        // SAFETY: `instance` is live and the descriptions are stack-local for
        // the duration of this synchronous C call.
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
        // Feeder threads stop reaching the instance before it goes away.
        self.external_gate.detach();
        // SAFETY: unregister the callbacks before destroying the instance,
        // so no new native callbacks can fire after teardown begins; the
        // Arc field keeps `CallbackShared` alive until the struct is fully
        // dropped.
        unsafe { self.clear_callbacks(instance) };
        self.call_to_user.clear();
        self.callback.user_to_call.lock_or_recover().clear();
        // SAFETY: callback registration is gone and this is the matching
        // destroy for the constructor used by `ensure_instance`.
        unsafe { (self.api.ntg_instance_destroy)(instance.as_ptr()) };
    }
}

/// Run `issue` on the chosen output; when that fails, log it and try the
/// system default once. Returns whether the default was used instead.
fn playback_with_fallback(
    chosen: Option<&str>,
    mut issue: impl FnMut(Option<&str>) -> Result<(), EngineError>,
) -> Result<bool, EngineError> {
    match issue(chosen) {
        Ok(()) => Ok(false),
        Err(err) if chosen.is_some() => {
            eprintln!("quill: group call output {chosen:?} refused ({err}); using the default");
            issue(None).map(|()| true)
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod playback_tests {
    use super::{EngineError, playback_with_fallback};

    #[test]
    fn a_refused_output_falls_back_to_the_default_once() {
        let mut tried = Vec::new();
        let used_default = playback_with_fallback(Some("gone"), |speaker| {
            tried.push(speaker.map(str::to_owned));
            match speaker {
                Some(_) => Err(EngineError::Engine {
                    op: "ntg_set_stream_sources",
                    code: -1,
                }),
                None => Ok(()),
            }
        });
        assert!(matches!(used_default, Ok(true)));
        assert_eq!(tried, vec![Some("gone".to_owned()), None]);
    }

    #[test]
    fn the_default_failing_is_reported_without_a_retry() {
        let mut calls = 0;
        let result = playback_with_fallback(None, |_| {
            calls += 1;
            Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: -1,
            })
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_working_output_is_kept() {
        assert!(matches!(
            playback_with_fallback(Some("usb"), |_| Ok(())),
            Ok(false)
        ));
    }
}

#[cfg(test)]
mod native_tests;

#[cfg(test)]
mod loopback_tests;

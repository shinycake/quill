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

mod call_engine;

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
mod native_tests;

//! Phase C2b: call-engine abstraction plus the runtime-loaded ntgcalls adapter.

use ntgcalls_sys::{
    Loader, NTG_CONNECTION_STATE_CLOSED, NTG_CONNECTION_STATE_CONNECTED,
    NTG_CONNECTION_STATE_CONNECTING, NTG_CONNECTION_STATE_FAILED, NTG_CONNECTION_STATE_TIMEOUT,
    NTG_ERR_INVALID_PARAMS, NTG_MEDIA_SOURCE_DEVICE, NTG_OK, NTG_STREAM_MODE_CAPTURE,
    NTG_STREAM_MODE_PLAYBACK, ntg_audio_description, ntg_connection_info, ntg_device_info,
    ntg_instance, ntg_media_description, ntg_media_devices, ntg_protocol, ntg_rtc_server,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::marker::PhantomData;
use std::ptr::{NonNull, null_mut};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// Phase C2b: engine-emitted signaling routed as `(tdlib_call_id, data)`.
pub type SignalingEmittedCallback = Arc<dyn Fn(i32, Vec<u8>) + Send + Sync + 'static>;

pub type TransportStateCallback = Arc<dyn Fn(i32, TransportState) + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Connecting,
    Connected,
    Failed,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtcServer {
    pub id: u64,
    pub ipv4: String,
    pub ipv6: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub turn: bool,
    pub stun: bool,
    pub tcp: bool,
    pub peer_tag: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectParams {
    pub encryption_key: Vec<u8>,
    pub is_outgoing: bool,
    pub servers: Vec<RtcServer>,
    pub library_versions: Vec<String>,
    pub p2p_allowed: bool,
    pub mic_input: Option<String>,
    pub speaker_input: Option<String>,
}

/// Phase C2b: the exact `callProtocol` capability reported to TDLib.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineProtocol {
    pub udp_p2p: bool,
    pub udp_reflector: bool,
    pub min_layer: i32,
    pub max_layer: i32,
    pub library_versions: Vec<String>,
}

impl EngineProtocol {
    /// Phase C2b: the existing honest fallback when no engine is available.
    pub fn signaling_only() -> Self {
        Self {
            udp_p2p: false,
            udp_reflector: false,
            min_layer: 65,
            max_layer: 92,
            library_versions: Vec::new(),
        }
    }

    /// Phase C2b: render `callProtocol` (`schema/td_api.tl:7008`).
    pub fn to_json(&self) -> Value {
        json!({
            "@type": "callProtocol",
            "udp_p2p": self.udp_p2p,
            "udp_reflector": self.udp_reflector,
            "min_layer": self.min_layer,
            "max_layer": self.max_layer,
            "library_versions": self.library_versions,
        })
    }
}

/// Phase C2b: an engine device category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDeviceKind {
    Microphone,
    Speaker,
    Camera,
    Screen,
}

/// Phase C2b: one engine-enumerated media device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDevice {
    /// Phase C2b: engine metadata descriptor used to select the device.
    pub id: String,
    pub name: String,
    pub kind: MediaDeviceKind,
}

/// Phase C2b: safe call-engine failure surface.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("call engine is unavailable")]
    Unavailable,
    #[error("call engine does not track TDLib call {0}")]
    NoSuchCall(i32),
    #[error("call engine operation {op} failed with code {code}")]
    Engine { op: &'static str, code: i32 },
    #[error("call engine returned a null instance")]
    NullInstance,
    #[error("there is no active call")]
    NoActiveCall,
}

/// Phase C2b: 1:1 engine lifecycle and bidirectional signaling.
///
/// There is deliberately no `connect_ready` method in this slice. Transport
/// connect (`ntg_skip_exchange` + `ntg_connect_p2p` + stream sources) is C2c:
/// `P2PCall::connect()` starts real default-device audio I/O, an unverifiable
/// production side effect. The engine queues inbound signaling until C2c
/// connects it.
pub trait CallEngine {
    /// Phase C2b: app -> engine; create the engine-side call keyed by
    /// Telegram user id.
    fn start_call(
        &mut self,
        call_id: i32,
        user_id: i64,
        is_outgoing: bool,
    ) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; mark a tracked incoming call as accepted.
    fn accept_call(&mut self, call_id: i32) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; feed TDLib `updateNewCallSignalingData`
    /// (`schema/td_api.tl:10862`) to `ntg_send_signaling_data`.
    fn send_signaling_data(&mut self, call_id: i32, data: &[u8]) -> Result<(), EngineError>;

    /// Phase C2b: engine -> app; route emitted bytes to the registered hook.
    /// The shared callback makes this callable without borrowing the driver
    /// thread.
    fn receive_signaling_data(&self, call_id: i32, data: &[u8]);

    /// Phase C2b: register the bridge that forwards engine-emitted data
    /// through TDLib `sendCallSignalingData` (`schema/td_api.tl:14218`).
    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback);

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback);

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError>;

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; idempotent teardown, where an unknown call
    /// id succeeds.
    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; set microphone mute state for a tracked call.
    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError>;

    /// Phase C2b: engine -> app; enumerate capture and playback devices.
    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError>;

    /// Phase C2b: engine -> app; return the protocol the engine itself reports.
    fn protocol(&self) -> EngineProtocol;

    /// Phase C2b: engine -> app; report whether operations are available.
    fn is_available(&self) -> bool;
}

#[derive(Default)]
struct MockInner {
    available: bool,
    calls: HashMap<i32, (i64, bool)>,
    started_calls: Vec<(i32, i64, bool)>,
    accepted_calls: Vec<i32>,
    signaling_received: Vec<(i32, Vec<u8>)>,
    hung_up_calls: Vec<i32>,
    mute_changes: Vec<(i32, bool)>,
    /// Test-only failure injection for `set_muted`.
    fail_mute: bool,
    devices: Vec<MediaDevice>,
    hook: Option<SignalingEmittedCallback>,
    transport_hook: Option<TransportStateCallback>,
    connects: Vec<(i32, ConnectParams)>,
    device_selections: Vec<(i32, Option<String>, Option<String>)>,
}

/// Phase C2b: deterministic in-memory engine used by driver tests.
#[derive(Clone)]
pub struct MockEngine {
    inner: Arc<Mutex<MockInner>>,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockInner {
                available: true,
                ..MockInner::default()
            })),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockInner::default())),
        }
    }

    pub fn started_calls(&self) -> Vec<(i32, i64, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .started_calls
            .clone()
    }

    pub fn accepted_calls(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .accepted_calls
            .clone()
    }

    pub fn signaling_received(&self) -> Vec<(i32, Vec<u8>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .signaling_received
            .clone()
    }

    pub fn hung_up_calls(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .hung_up_calls
            .clone()
    }

    pub fn mute_changes(&self) -> Vec<(i32, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .mute_changes
            .clone()
    }

    pub fn set_devices(&self, devices: Vec<MediaDevice>) {
        self.inner.lock().expect("mock call engine").devices = devices;
    }

    /// Test-only: make the next `set_muted` calls fail.
    pub fn fail_mute(&self) {
        self.inner.lock().expect("mock call engine").fail_mute = true;
    }

    pub fn connects(&self) -> Vec<(i32, ConnectParams)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .connects
            .clone()
    }

    pub fn device_selections(&self) -> Vec<(i32, Option<String>, Option<String>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .device_selections
            .clone()
    }

    pub fn emit_transport_state(&self, call_id: i32, state: TransportState) {
        let hook = self
            .inner
            .lock()
            .expect("mock call engine")
            .transport_hook
            .clone();
        if let Some(hook) = hook {
            hook(call_id, state);
        }
    }

    fn engine_protocol() -> EngineProtocol {
        EngineProtocol {
            udp_p2p: true,
            udp_reflector: true,
            min_layer: 92,
            max_layer: 92,
            library_versions: vec![
                "8.0.0".into(),
                "9.0.0".into(),
                "12.0.0".into(),
                "13.0.0".into(),
            ],
        }
    }
}

impl CallEngine for MockEngine {
    fn start_call(
        &mut self,
        call_id: i32,
        user_id: i64,
        is_outgoing: bool,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if inner.calls.contains_key(&call_id) {
            return Ok(());
        }
        inner.calls.insert(call_id, (user_id, is_outgoing));
        inner.started_calls.push((call_id, user_id, is_outgoing));
        Ok(())
    }

    fn accept_call(&mut self, call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.accepted_calls.push(call_id);
        Ok(())
    }

    fn send_signaling_data(&mut self, call_id: i32, data: &[u8]) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.signaling_received.push((call_id, data.to_vec()));
        Ok(())
    }

    fn receive_signaling_data(&self, call_id: i32, data: &[u8]) {
        let hook = self.inner.lock().expect("mock call engine").hook.clone();
        if let Some(hook) = hook {
            hook(call_id, data.to_vec());
        }
    }

    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback) {
        self.inner.lock().expect("mock call engine").hook = Some(callback);
    }

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback) {
        self.inner.lock().expect("mock call engine").transport_hook = Some(callback);
    }

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.connects.push((call_id, params.clone()));
        Ok(())
    }

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.device_selections.push((
            call_id,
            microphone.map(str::to_owned),
            speaker.map(str::to_owned),
        ));
        Ok(())
    }

    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if inner.calls.remove(&call_id).is_some() {
            inner.hung_up_calls.push(call_id);
        }
        Ok(())
    }

    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        if inner.fail_mute {
            return Err(EngineError::Engine {
                op: "set_muted",
                code: -1,
            });
        }
        inner.mute_changes.push((call_id, muted));
        Ok(())
    }

    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError> {
        let inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        Ok(inner.devices.clone())
    }

    fn protocol(&self) -> EngineProtocol {
        if self.is_available() {
            Self::engine_protocol()
        } else {
            EngineProtocol::signaling_only()
        }
    }

    fn is_available(&self) -> bool {
        self.inner.lock().expect("mock call engine").available
    }
}

struct CallbackShared {
    user_to_call: Mutex<HashMap<i64, i32>>,
    hook: Mutex<Option<SignalingEmittedCallback>>,
    transport_hook: Mutex<Option<TransportStateCallback>>,
}

/// Phase C2b: safe driver-thread adapter over the runtime-loaded C ABI.
///
/// `Rc` in the marker intentionally keeps this type `!Send`: the native
/// instance is created, used, and destroyed on the connect-driver thread.
pub struct NtgcallsEngine {
    api: Loader,
    instance: Option<NonNull<ntg_instance>>,
    protocol: EngineProtocol,
    call_to_user: HashMap<i32, i64>,
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
            callback: Arc::new(CallbackShared {
                user_to_call: Mutex::new(HashMap::new()),
                hook: Mutex::new(None),
                transport_hook: Mutex::new(None),
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
        self.instance = Some(instance);
        Ok(instance)
    }

    fn user_id(&self, call_id: i32) -> Result<i64, EngineError> {
        self.call_to_user
            .get(&call_id)
            .copied()
            .ok_or(EngineError::NoSuchCall(call_id))
    }

    fn set_audio_sources(
        &self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        let instance = self.instance.ok_or(EngineError::NullInstance)?;
        for (mode, input, microphone_source) in [
            (NTG_STREAM_MODE_CAPTURE, microphone, true),
            (NTG_STREAM_MODE_PLAYBACK, speaker, false),
        ] {
            let input = input
                .map(CString::new)
                .transpose()
                .map_err(|_| EngineError::Engine {
                    op: "ntg_set_stream_sources",
                    code: NTG_ERR_INVALID_PARAMS,
                })?;
            let mut audio = ntg_audio_description {
                media_source: NTG_MEDIA_SOURCE_DEVICE,
                sample_rate: 48_000,
                channel_count: 1,
                // ntgcalls' wrapper convention uses device metadata here;
                // NULL selects the default (the C header is silent).
                input: input
                    .as_ref()
                    .map_or(null_mut(), |value| value.as_ptr().cast_mut()),
                keep_open: false,
            };
            let media = ntg_media_description {
                microphone: if microphone_source {
                    &mut audio
                } else {
                    null_mut()
                },
                speaker: if microphone_source {
                    null_mut()
                } else {
                    &mut audio
                },
                camera: null_mut(),
                screen: null_mut(),
            };
            let rc = unsafe {
                (self.api.ntg_set_stream_sources)(instance.as_ptr(), user_id, mode, media)
            };
            if rc != NTG_OK {
                return Err(EngineError::Engine {
                    op: "ntg_set_stream_sources",
                    code: rc,
                });
            }
        }
        Ok(())
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
        let instance = self.instance.ok_or(EngineError::NullInstance)?;
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

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        let instance = self.ensure_instance()?;
        if params.encryption_key.is_empty() {
            return Err(EngineError::Engine {
                op: "ntg_skip_exchange",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        let servers = NativeRtcServers::new(&params.servers)?;
        let versions = c_strings(&params.library_versions)?;
        let version_ptrs: Vec<_> = versions.iter().map(|value| value.as_ptr()).collect();
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
        let rc = unsafe {
            (self.api.ntg_connect_p2p)(
                instance.as_ptr(),
                user_id,
                servers.raw.as_ptr(),
                servers.raw.len(),
                version_ptrs.as_ptr(),
                version_ptrs.len(),
                params.p2p_allowed,
                std::ptr::null(),
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_connect_p2p",
                code: rc,
            });
        }
        self.set_audio_sources(
            call_id,
            params.mic_input.as_deref(),
            params.speaker_input.as_deref(),
        )
    }

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        self.user_id(call_id)?;
        self.set_audio_sources(call_id, microphone, speaker)
    }

    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError> {
        let Some(user_id) = self.call_to_user.remove(&call_id) else {
            return Ok(());
        };
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
        let instance = self.instance.ok_or(EngineError::NullInstance)?;
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

unsafe extern "C" fn signaling_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    data: *const u8,
    len: usize,
    user_data: *mut c_void,
) {
    if user_data.is_null() || (data.is_null() && len != 0) {
        return;
    }
    // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
    // Drop unregisters the callback before releasing the Arc.
    let shared = unsafe { &*user_data.cast::<CallbackShared>() };
    let call_id = shared
        .user_to_call
        .lock()
        .expect("ntgcalls callback map")
        .get(&user_id)
        .copied();
    let hook = shared.hook.lock().expect("ntgcalls callback hook").clone();
    let (Some(call_id), Some(hook)) = (call_id, hook) else {
        return;
    };
    let bytes = if len == 0 {
        Vec::new()
    } else {
        // SAFETY: ntgcalls guarantees the callback byte span for this call;
        // copy it before returning to the worker thread.
        unsafe { std::slice::from_raw_parts(data, len) }.to_vec()
    };
    hook(call_id, bytes);
}

unsafe extern "C" fn connection_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    info: ntg_connection_info,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let state = match info.state {
        NTG_CONNECTION_STATE_CONNECTING => TransportState::Connecting,
        NTG_CONNECTION_STATE_CONNECTED => TransportState::Connected,
        NTG_CONNECTION_STATE_FAILED => TransportState::Failed,
        NTG_CONNECTION_STATE_TIMEOUT => TransportState::Failed,
        NTG_CONNECTION_STATE_CLOSED => TransportState::Closed,
        _ => return,
    };
    let shared = unsafe { &*user_data.cast::<CallbackShared>() };
    let call_id = shared
        .user_to_call
        .lock()
        .expect("ntgcalls callback map")
        .get(&user_id)
        .copied();
    let hook = shared
        .transport_hook
        .lock()
        .expect("ntgcalls transport callback hook")
        .clone();
    if let (Some(call_id), Some(hook)) = (call_id, hook) {
        hook(call_id, state);
    }
}

fn c_strings(values: &[String]) -> Result<Vec<CString>, EngineError> {
    values
        .iter()
        .map(|value| {
            CString::new(value.as_str()).map_err(|_| EngineError::Engine {
                op: "ntg_connect_p2p",
                code: NTG_ERR_INVALID_PARAMS,
            })
        })
        .collect()
}

struct NativeRtcServers {
    raw: Vec<ntg_rtc_server>,
    _strings: Vec<[CString; 4]>,
    _peer_tags: Vec<Vec<u8>>,
}

impl NativeRtcServers {
    fn new(servers: &[RtcServer]) -> Result<Self, EngineError> {
        let mut strings = Vec::with_capacity(servers.len());
        let mut peer_tags = Vec::with_capacity(servers.len());
        for server in servers {
            strings.push([
                native_string(&server.ipv4)?,
                native_string(&server.ipv6)?,
                native_string(&server.username)?,
                native_string(&server.password)?,
            ]);
            peer_tags.push(server.peer_tag.clone());
        }
        let raw = servers
            .iter()
            .enumerate()
            .map(|(index, server)| ntg_rtc_server {
                id: server.id,
                ipv4: strings[index][0].as_ptr().cast_mut(),
                ipv6: strings[index][1].as_ptr().cast_mut(),
                port: server.port,
                username: strings[index][2].as_ptr().cast_mut(),
                password: strings[index][3].as_ptr().cast_mut(),
                turn: server.turn,
                stun: server.stun,
                tcp: server.tcp,
                peer_tag: peer_tags[index].as_ptr().cast_mut(),
                peer_tag_len: peer_tags[index].len(),
            })
            .collect();
        Ok(Self {
            raw,
            _strings: strings,
            _peer_tags: peer_tags,
        })
    }
}

fn native_string(value: &str) -> Result<CString, EngineError> {
    CString::new(value).map_err(|_| EngineError::Engine {
        op: "ntg_connect_p2p",
        code: NTG_ERR_INVALID_PARAMS,
    })
}

fn c_string_pointer_array(values: *mut *mut std::ffi::c_char, len: usize) -> Vec<String> {
    if values.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: a successful ntgcalls output provides `len` pointer entries;
    // each individual pointer is still checked for null by `c_string`.
    unsafe { std::slice::from_raw_parts(values, len) }
        .iter()
        .map(|value| c_string(*value))
        .collect()
}

fn append_devices(
    output: &mut Vec<MediaDevice>,
    devices: *mut ntg_device_info,
    len: usize,
    kind: MediaDeviceKind,
) {
    if devices.is_null() || len == 0 {
        return;
    }
    // SAFETY: a successful device enumeration provides `len` entries; all
    // nested C strings are individually null-checked below.
    for device in unsafe { std::slice::from_raw_parts(devices, len) } {
        output.push(MediaDevice {
            id: c_string(device.metadata),
            name: c_string(device.name),
            kind,
        });
    }
}

fn c_string(value: *mut std::ffi::c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    // SAFETY: non-null strings in successful ntgcalls outputs are NUL
    // terminated and remain valid until their parent output is freed.
    unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_engine_lifecycle_and_protocol() {
        let mut engine = MockEngine::new();
        let handle = engine.clone();
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let emitted_for_hook = emitted.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |call_id, data| {
            emitted_for_hook
                .lock()
                .expect("emitted signaling")
                .push((call_id, data));
        }));
        let devices = vec![MediaDevice {
            id: "mic-1".into(),
            name: "Test microphone".into(),
            kind: MediaDeviceKind::Microphone,
        }];
        engine.set_devices(devices.clone());

        engine.start_call(77, 41, false).unwrap();
        engine.accept_call(77).unwrap();
        let params = ConnectParams {
            encryption_key: vec![1; 256],
            is_outgoing: false,
            servers: vec![RtcServer {
                id: 7,
                ipv4: "149.154.167.40".into(),
                ipv6: String::new(),
                port: 443,
                username: String::new(),
                password: String::new(),
                turn: true,
                stun: false,
                tcp: true,
                peer_tag: vec![0, 1, 2],
            }],
            library_versions: vec!["13.0.0".into()],
            p2p_allowed: true,
            mic_input: Some("mic-1".into()),
            speaker_input: Some("speaker-1".into()),
        };
        engine.connect(77, &params).unwrap();
        engine
            .select_devices(77, Some("mic-1"), Some("speaker-1"))
            .unwrap();
        engine.send_signaling_data(77, b"inbound").unwrap();
        handle.receive_signaling_data(77, b"outbound");
        engine.set_muted(77, true).unwrap();
        assert_eq!(engine.media_devices().unwrap(), devices);
        assert_eq!(engine.protocol(), MockEngine::engine_protocol());
        assert!(engine.is_available());
        engine.hangup(77).unwrap();

        assert_eq!(handle.started_calls(), vec![(77, 41, false)]);
        assert_eq!(handle.accepted_calls(), vec![77]);
        assert_eq!(handle.connects(), vec![(77, params)]);
        assert_eq!(
            handle.device_selections(),
            vec![(77, Some("mic-1".to_string()), Some("speaker-1".to_string()))]
        );
        assert_eq!(handle.signaling_received(), vec![(77, b"inbound".to_vec())]);
        assert_eq!(handle.mute_changes(), vec![(77, true)]);
        assert_eq!(handle.hung_up_calls(), vec![77]);
        assert_eq!(
            *emitted.lock().expect("emitted signaling"),
            vec![(77, b"outbound".to_vec())]
        );
    }

    #[test]
    fn mock_engine_unknown_calls_and_idempotent_hangup() {
        let mut engine = MockEngine::new();
        assert_eq!(engine.accept_call(77), Err(EngineError::NoSuchCall(77)));
        assert_eq!(
            engine.send_signaling_data(77, b"data"),
            Err(EngineError::NoSuchCall(77))
        );
        assert_eq!(engine.set_muted(77, true), Err(EngineError::NoSuchCall(77)));
        assert_eq!(engine.hangup(77), Ok(()));
    }

    #[test]
    fn unavailable_mock_rejects_operations() {
        let mut engine = MockEngine::unavailable();
        assert!(!engine.is_available());
        assert_eq!(engine.protocol(), EngineProtocol::signaling_only());
        assert_eq!(
            engine.start_call(77, 41, false),
            Err(EngineError::Unavailable)
        );
        assert_eq!(engine.accept_call(77), Err(EngineError::Unavailable));
        assert_eq!(
            engine.send_signaling_data(77, b"data"),
            Err(EngineError::Unavailable)
        );
        assert_eq!(engine.hangup(77), Err(EngineError::Unavailable));
        assert_eq!(engine.set_muted(77, true), Err(EngineError::Unavailable));
        assert_eq!(engine.media_devices(), Err(EngineError::Unavailable));
    }

    #[test]
    fn mock_engine_replaces_signaling_hook() {
        let mut engine = MockEngine::new();
        let first = Arc::new(Mutex::new(Vec::new()));
        let first_hook = first.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |_, data| {
            first_hook.lock().expect("first hook").push(data);
        }));
        let second = Arc::new(Mutex::new(Vec::new()));
        let second_hook = second.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |_, data| {
            second_hook.lock().expect("second hook").push(data);
        }));

        engine.receive_signaling_data(77, b"replacement");
        assert!(first.lock().expect("first hook").is_empty());
        assert_eq!(
            *second.lock().expect("second hook"),
            vec![b"replacement".to_vec()]
        );
    }

    #[test]
    fn mock_engine_delivers_transport_state_and_replaces_hook() {
        let mut engine = MockEngine::new();
        let first = Arc::new(Mutex::new(Vec::new()));
        let first_hook = first.clone();
        engine.set_transport_state_callback(Arc::new(move |call_id, state| {
            first_hook
                .lock()
                .expect("first transport hook")
                .push((call_id, state));
        }));
        let second = Arc::new(Mutex::new(Vec::new()));
        let second_hook = second.clone();
        engine.set_transport_state_callback(Arc::new(move |call_id, state| {
            second_hook
                .lock()
                .expect("second transport hook")
                .push((call_id, state));
        }));

        engine.emit_transport_state(77, TransportState::Connected);
        assert!(first.lock().expect("first transport hook").is_empty());
        assert_eq!(
            *second.lock().expect("second transport hook"),
            vec![(77, TransportState::Connected)]
        );
    }

    #[test]
    fn mock_engine_connect_and_device_selection_need_known_call() {
        let mut engine = MockEngine::new();
        let params = ConnectParams {
            encryption_key: vec![1; 256],
            is_outgoing: true,
            servers: Vec::new(),
            library_versions: Vec::new(),
            p2p_allowed: false,
            mic_input: None,
            speaker_input: None,
        };
        assert_eq!(
            engine.connect(77, &params),
            Err(EngineError::NoSuchCall(77))
        );
        assert_eq!(
            engine.select_devices(77, Some("mic-1"), None),
            Err(EngineError::NoSuchCall(77))
        );
        assert!(engine.connects().is_empty());
        assert!(engine.device_selections().is_empty());
    }
}

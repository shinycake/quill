//! `MockEngine`: in-memory `CallEngine` for tests and UI demos.
use super::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    /// Test-only failure injection for the next `connect`.
    fail_connect: bool,
    devices: Vec<MediaDevice>,
    hook: Option<SignalingEmittedCallback>,
    transport_hook: Option<TransportStateCallback>,
    /// Phase C2e: video frame and peer camera-state hooks (replace semantics).
    video_hook: Option<VideoFrameCallback>,
    remote_video_hook: Option<RemoteVideoStateCallback>,
    /// Phase C2j: peer 1:1 screen-share state hook (replace semantics).
    remote_screen_hook: Option<RemoteVideoStateCallback>,
    camera_changes: Vec<(i32, bool, Option<String>)>,
    /// Phase C2i: 1:1 screen-share toggle recording for driver tests.
    p2p_screen_share_changes: Vec<(i32, bool)>,
    frame_seq: u64,
    connects: Vec<(i32, ConnectParams)>,
    device_selections: Vec<(i32, Option<String>, Option<String>)>,
    /// Phase C2g: group transport recording for driver tests.
    group_offers: Vec<(i32, i64)>,
    group_connects: Vec<(i32, String, bool)>,
    group_video_syncs: Vec<(i32, Vec<GroupVideoSource>)>,
    group_camera_changes: Vec<(i32, bool, Option<String>)>,
    screen_share_offers: Vec<i32>,
    screen_share_connects: Vec<(i32, String)>,
    screen_share_stops: Vec<i32>,
    /// Phase C2g: group call ids with a live native presentation
    /// (initialized via `start_screen_share`, cleared by `stop_screen_share`).
    presentations: Vec<i32>,
    group_leaves: Vec<i32>,
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

    pub fn fail_next_connect(&self) {
        self.inner.lock().expect("mock call engine").fail_connect = true;
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

    pub fn camera_changes(&self) -> Vec<(i32, bool, Option<String>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .camera_changes
            .clone()
    }

    /// Phase C2g: recorded group transport calls for driver tests.
    pub fn group_offers(&self) -> Vec<(i32, i64)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_offers
            .clone()
    }

    pub fn group_connects(&self) -> Vec<(i32, String, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_connects
            .clone()
    }

    pub fn group_video_syncs(&self) -> Vec<(i32, Vec<GroupVideoSource>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_video_syncs
            .clone()
    }

    pub fn group_camera_changes(&self) -> Vec<(i32, bool, Option<String>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_camera_changes
            .clone()
    }

    pub fn screen_share_offers(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .screen_share_offers
            .clone()
    }

    pub fn screen_share_connects(&self) -> Vec<(i32, String)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .screen_share_connects
            .clone()
    }

    pub fn screen_share_stops(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .screen_share_stops
            .clone()
    }

    /// Phase C2i: recorded 1:1 screen-share toggles.
    pub fn p2p_screen_share_changes(&self) -> Vec<(i32, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .p2p_screen_share_changes
            .clone()
    }

    pub fn group_leaves(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_leaves
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

    /// Phase C2e: deliver a synthetic video frame to the registered hook,
    /// assigning the next sequence number from the internal counter.
    pub fn emit_video_frame(&self, call_id: i32, mut frame: VideoFrame) {
        let mut inner = self.inner.lock().expect("mock call engine");
        frame.seq = inner.frame_seq;
        inner.frame_seq += 1;
        let hook = inner.video_hook.clone();
        drop(inner);
        if let Some(hook) = hook {
            hook(call_id, frame);
        }
    }

    /// Phase C2e: deliver a synthetic peer camera state to the hook.
    pub fn emit_remote_video_state(&self, call_id: i32, state: RemoteVideoState) {
        let hook = self
            .inner
            .lock()
            .expect("mock call engine")
            .remote_video_hook
            .clone();
        if let Some(hook) = hook {
            hook(call_id, state);
        }
    }

    /// Phase C2j: deliver a synthetic peer screen-share state to the hook.
    pub fn emit_remote_screen_state(&self, call_id: i32, state: RemoteVideoState) {
        let hook = self
            .inner
            .lock()
            .expect("mock call engine")
            .remote_screen_hook
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

    fn set_video_frame_callback(&mut self, callback: VideoFrameCallback) {
        self.inner.lock().expect("mock call engine").video_hook = Some(callback);
    }

    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        self.inner
            .lock()
            .expect("mock call engine")
            .remote_video_hook = Some(callback);
    }

    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        self.inner
            .lock()
            .expect("mock call engine")
            .remote_screen_hook = Some(callback);
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
        if inner.fail_connect {
            inner.fail_connect = false;
            return Err(EngineError::Engine {
                op: "connect",
                code: -1,
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

    fn set_camera_enabled(
        &mut self,
        call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner
            .camera_changes
            .push((call_id, enabled, camera.map(str::to_owned)));
        Ok(())
    }

    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.p2p_screen_share_changes.push((call_id, enabled));
        Ok(())
    }

    fn create_group_call(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
    ) -> Result<String, EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.group_offers.push((group_call_id, chat_id));
        Ok(format!("mock-group-offer-{group_call_id}"))
    }

    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        video_enabled: bool,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .group_connects
            .push((group_call_id, answer_payload.to_owned(), video_enabled));
        Ok(())
    }

    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .group_video_syncs
            .push((group_call_id, sources.to_vec()));
        Ok(())
    }

    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .group_camera_changes
            .push((group_call_id, enabled, camera.map(str::to_owned)));
        Ok(())
    }

    fn start_screen_share(&mut self, group_call_id: i32) -> Result<String, EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.screen_share_offers.push(group_call_id);
        if !inner.presentations.contains(&group_call_id) {
            inner.presentations.push(group_call_id);
        }
        Ok(format!("mock-presentation-offer-{group_call_id}"))
    }

    fn connect_screen_share(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .screen_share_connects
            .push((group_call_id, answer_payload.to_owned()));
        Ok(())
    }

    fn stop_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.screen_share_stops.push(group_call_id);
        inner.presentations.retain(|id| *id != group_call_id);
        Ok(())
    }

    fn presentation_active(&self, group_call_id: i32) -> bool {
        self.inner
            .lock()
            .expect("mock call engine")
            .presentations
            .contains(&group_call_id)
    }

    fn leave_group_call(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.group_leaves.push(group_call_id);
        // Mirror the real engine: a live presentation is stopped first
        // (privacy — capture ends before the call).
        if inner.presentations.contains(&group_call_id) {
            inner.screen_share_stops.push(group_call_id);
        }
        inner.presentations.retain(|id| *id != group_call_id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use ntgcalls_sys::{
        NTG_STREAM_DEVICE_CAMERA, NTG_STREAM_DEVICE_SCREEN, NTG_STREAM_MODE_CAPTURE,
        NTG_STREAM_MODE_PLAYBACK, NTG_STREAM_STATUS_ACTIVE, NTG_STREAM_STATUS_PAUSED,
        NTG_VIDEO_ROTATION_VIDEO_ROTATION_90,
    };
    use ntgcalls_sys::{NTG_STREAM_STATUS_IDLING, NTG_VIDEO_ROTATION_VIDEO_ROTATION_0};

    /// Phase C2g: the audio SSRC is the first `a=ssrc:` line of the
    /// audio `m=` section; video sections are ignored, and 0 is the
    /// honest fallback (the join still goes out).
    #[test]
    fn group_offer_audio_source_id_parses_first_audio_ssrc() {
        let offer = "v=0\r\n\
            m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
            a=ssrc:12345 cname:audio\r\n\
            a=ssrc:12346 cname:audio\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
            a=ssrc:99999 cname:video\r\n";
        assert_eq!(group_offer_audio_source_id(offer), 12345);
    }

    #[test]
    fn group_offer_audio_source_id_zero_without_audio_ssrc() {
        assert_eq!(
            group_offer_audio_source_id("v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n"),
            0
        );
        assert_eq!(group_offer_audio_source_id(""), 0);
    }

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
            video_enabled: false,
            camera_input: None,
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
            video_enabled: false,
            camera_input: None,
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

    /// Phase C2e: validates the BT.601 conversion math. Neutral chroma
    /// passes Y straight through, so Y=235 ~ white and Y=16 ~ black.
    /// A saturated case with no clamping on red pins the 1.402 coefficient:
    /// Y=64, U=128, V=255 gives R = 64 + 1.402*127 ~ 242.
    #[test]
    fn i420_to_rgba_known_pixels() {
        let white = i420_to_rgba(
            2,
            2,
            &[235, 235, 235, 235, 128, 128],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        )
        .expect("valid 2x2 frame");
        assert_eq!(white.len(), 16);
        let (pixels, _) = white.as_chunks::<4>();
        for pixel in pixels {
            assert_pixel_close(pixel, &[235, 235, 235, 255]);
        }
        let black = i420_to_rgba(
            2,
            2,
            &[16, 16, 16, 16, 128, 128],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        )
        .expect("valid 2x2 frame");
        let (pixels, _) = black.as_chunks::<4>();
        for pixel in pixels {
            assert_pixel_close(pixel, &[16, 16, 16, 255]);
        }
        // I420 planes: Y(2x2) then U(1x1) then V(1x1).
        let saturated = i420_to_rgba(
            2,
            2,
            &[64, 64, 64, 64, 128, 255],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        )
        .expect("valid 2x2 frame");
        let (pixels, _) = saturated.as_chunks::<4>();
        for pixel in pixels {
            assert_pixel_close(pixel, &[242, 0, 64, 255]);
        }
    }

    fn assert_pixel_close(actual: &[u8], expected: &[u8; 4]) {
        assert_eq!(actual.len(), 4);
        for (index, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
            assert!(
                (i16::from(a) - i16::from(e)).abs() <= 2,
                "channel {index}: got {a}, want ~{e}"
            );
        }
    }

    /// Phase C2e: validates we never render garbage bytes.
    #[test]
    fn i420_to_rgba_rejects_bad_sizes() {
        assert_eq!(
            i420_to_rgba(2, 2, &[0; 5], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
        assert_eq!(
            i420_to_rgba(2, 2, &[0; 7], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
        assert_eq!(
            i420_to_rgba(0, 2, &[], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
        assert_eq!(
            i420_to_rgba(2, 0, &[], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
    }

    /// Phase C2e: validates rotation handling. 2x1 with distinct pixels,
    /// rotated 90° clockwise, becomes 1x2 with the right column on top.
    #[test]
    fn i420_to_rgba_rotation_90() {
        let rgba = i420_to_rgba(
            2,
            1,
            &[235, 16, 128, 128],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_90,
        )
        .expect("valid 2x1 frame");
        assert_eq!(
            rgba,
            vec![16, 16, 16, 255, 235, 235, 235, 255],
            "black pixel (right column) lands on top"
        );
    }

    /// Phase C2i: validates the 1:1 screen-share toggle contract the
    /// UI will drive (mirrors `mock_camera_toggle_state_machine`).
    #[test]
    fn mock_p2p_screen_share_toggle_state_machine() {
        let mut engine = MockEngine::new();
        engine.start_call(77, 41, false).unwrap();
        engine.set_screen_share_enabled(77, true).unwrap();
        engine.set_screen_share_enabled(77, false).unwrap();
        assert_eq!(
            engine.p2p_screen_share_changes(),
            vec![(77, true), (77, false)]
        );
        assert_eq!(
            engine.set_screen_share_enabled(99, true),
            Err(EngineError::NoSuchCall(99))
        );

        let mut unavailable = MockEngine::unavailable();
        assert_eq!(
            unavailable.set_screen_share_enabled(77, true),
            Err(EngineError::Unavailable)
        );
    }

    /// Phase C2i: a mid-call screen-share intent survives a transport
    /// reconnect — `connect` re-runs with the retained params and the
    /// engine must re-issue screen-only (not flip back to the camera).
    #[test]
    fn retained_call_media_preserves_screen_share_across_reconnect() {
        let params = ConnectParams {
            encryption_key: vec![1; 256],
            is_outgoing: false,
            servers: Vec::new(),
            library_versions: vec!["13.0.0".into()],
            p2p_allowed: true,
            mic_input: Some("mic-1".into()),
            speaker_input: Some("speaker-1".into()),
            video_enabled: true,
            camera_input: Some("cam-1".into()),
        };
        // Fresh connect: no prior entry, share starts off.
        let fresh = retained_call_media(&params, None);
        assert!(!fresh.screen_share_on);
        assert_eq!(fresh.mic.as_deref(), Some("mic-1"));
        // Mid-call the user starts sharing, then the transport drops
        // and `connect` re-runs with the same retained params: the
        // share intent must survive so `set_media_sources` re-issues
        // screen-only (`camera_enabled && !screen_share_on` stays
        // false for the camera).
        let mut sharing = fresh;
        sharing.screen_share_on = true;
        sharing.camera_enabled = false;
        let reconnected = retained_call_media(&params, Some(&sharing));
        assert!(reconnected.screen_share_on);
        // Stopping share before the drop must not stick the flag on
        // either.
        let mut stopped = sharing;
        stopped.screen_share_on = false;
        assert!(!retained_call_media(&params, Some(&stopped)).screen_share_on);
    }

    /// Phase C2e: validates the toggle contract the UI will drive.
    #[test]
    fn mock_camera_toggle_state_machine() {
        let mut engine = MockEngine::new();
        engine.start_call(77, 41, false).unwrap();
        engine.set_camera_enabled(77, true, Some("cam-1")).unwrap();
        engine.set_camera_enabled(77, false, None).unwrap();
        assert_eq!(
            engine.camera_changes(),
            vec![(77, true, Some("cam-1".to_string())), (77, false, None)]
        );
        assert_eq!(
            engine.set_camera_enabled(99, true, None),
            Err(EngineError::NoSuchCall(99))
        );

        let mut unavailable = MockEngine::unavailable();
        assert_eq!(
            unavailable.set_camera_enabled(77, true, None),
            Err(EngineError::Unavailable)
        );
    }

    /// Phase C2e: validates frame delivery and hook replacement.
    #[test]
    fn mock_frame_callback_plumbing() {
        let mut engine = MockEngine::new();
        let first = Arc::new(Mutex::new(Vec::new()));
        let first_hook = first.clone();
        engine.set_video_frame_callback(Arc::new(move |call_id, frame| {
            first_hook
                .lock()
                .expect("first frame hook")
                .push((call_id, frame));
        }));
        let frame = VideoFrame {
            seq: 999, // overwritten by the mock's internal counter
            width: 2,
            height: 2,
            rgba: vec![1, 2, 3, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            is_local: true,
            participant_user_id: None,
            is_screen: false,
        };
        engine.emit_video_frame(77, frame.clone());
        let second = Arc::new(Mutex::new(Vec::new()));
        let second_hook = second.clone();
        engine.set_video_frame_callback(Arc::new(move |call_id, frame| {
            second_hook
                .lock()
                .expect("second frame hook")
                .push((call_id, frame));
        }));
        engine.emit_video_frame(77, frame);

        let first = first.lock().expect("first frame hook");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].0, 77);
        assert!(first[0].1.is_local);
        assert_eq!(first[0].1.seq, 0);
        let second = second.lock().expect("second frame hook");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].1.seq, 1);
        assert_eq!(second[0].1.width, 2);
    }

    /// Phase C2j: P2P frame acceptance — local preview, peer camera, and
    /// peer screen share reach the app; anything else is dropped.
    #[test]
    fn p2p_frame_kind_accepts_screen_share() {
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_CAPTURE, NTG_STREAM_DEVICE_CAMERA),
            Some(false)
        );
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_PLAYBACK, NTG_STREAM_DEVICE_CAMERA),
            Some(false)
        );
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_PLAYBACK, NTG_STREAM_DEVICE_SCREEN),
            Some(true)
        );
        // Capture-side screen frames and unknown kinds never reach a tile.
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_CAPTURE, NTG_STREAM_DEVICE_SCREEN),
            None
        );
        assert_eq!(p2p_frame_kind(999, 999), None);
    }

    /// Phase C2j: validates peer screen-share state delivery ordering.
    #[test]
    fn mock_remote_screen_state_emission() {
        let mut engine = MockEngine::new();
        let states = Arc::new(Mutex::new(Vec::new()));
        let states_hook = states.clone();
        engine.set_remote_screen_state_callback(Arc::new(move |call_id, state| {
            states_hook
                .lock()
                .expect("remote screen hook")
                .push((call_id, state));
        }));
        engine.emit_remote_screen_state(77, RemoteVideoState::Active);
        engine.emit_remote_screen_state(77, RemoteVideoState::Inactive);
        assert_eq!(
            *states.lock().expect("remote screen hook"),
            vec![
                (77, RemoteVideoState::Active),
                (77, RemoteVideoState::Inactive)
            ]
        );
    }

    /// Phase C2e: validates peer-state delivery ordering.
    #[test]
    fn mock_remote_video_state_emission() {
        let mut engine = MockEngine::new();
        let states = Arc::new(Mutex::new(Vec::new()));
        let states_hook = states.clone();
        engine.set_remote_video_state_callback(Arc::new(move |call_id, state| {
            states_hook
                .lock()
                .expect("remote video hook")
                .push((call_id, state));
        }));
        engine.emit_remote_video_state(77, RemoteVideoState::Inactive);
        engine.emit_remote_video_state(77, RemoteVideoState::Active);
        assert_eq!(
            *states.lock().expect("remote video hook"),
            vec![
                (77, RemoteVideoState::Inactive),
                (77, RemoteVideoState::Active)
            ]
        );
    }

    /// Phase C2e: validates the ntg -> app mapping mirrors TGX VideoState.
    #[test]
    fn remote_video_state_mapping() {
        assert_eq!(
            remote_video_state_from(NTG_STREAM_STATUS_ACTIVE),
            RemoteVideoState::Active
        );
        assert_eq!(
            remote_video_state_from(NTG_STREAM_STATUS_PAUSED),
            RemoteVideoState::Paused
        );
        assert_eq!(
            remote_video_state_from(NTG_STREAM_STATUS_IDLING),
            RemoteVideoState::Inactive
        );
    }

    /// Phase C2e: validates the honest no-camera decision the driver will use.
    #[test]
    fn video_wanted_requires_camera() {
        let camera = MediaDevice {
            id: "cam-1".into(),
            name: "Cam".into(),
            kind: MediaDeviceKind::Camera,
        };
        let mic = MediaDevice {
            id: "mic-1".into(),
            name: "Mic".into(),
            kind: MediaDeviceKind::Microphone,
        };
        assert_eq!(
            video_wanted(true, &[camera.clone(), mic]),
            (true, Some("cam-1".to_string()))
        );
        assert_eq!(video_wanted(true, &[]), (false, None));
        assert_eq!(video_wanted(false, &[camera]), (false, None));
    }
}

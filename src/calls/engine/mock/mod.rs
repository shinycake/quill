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
    /// The peer's 1:1 microphone hook (replace semantics).
    remote_audio_hook: Option<RemoteAudioStateCallback>,
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
    group_mutes: Vec<(i32, bool)>,
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

    pub fn group_mutes(&self) -> Vec<(i32, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_mutes
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

    /// Deliver a synthetic peer microphone state to the hook.
    pub fn emit_remote_audio_muted(&self, call_id: i32, muted: bool) {
        let hook = self
            .inner
            .lock()
            .expect("mock call engine")
            .remote_audio_hook
            .clone();
        if let Some(hook) = hook {
            hook(call_id, muted);
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

    fn set_remote_audio_state_callback(&mut self, callback: RemoteAudioStateCallback) {
        self.inner
            .lock()
            .expect("mock call engine")
            .remote_audio_hook = Some(callback);
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

    fn set_group_muted(&mut self, group_call_id: i32, muted: bool) -> Result<(), EngineError> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_mutes
            .push((group_call_id, muted));
        Ok(())
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
mod tests;

//! `MockEngine`: in-memory `CallEngine` for tests and UI demos.
use super::*;
use crate::locks::LockExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

mod external;
pub use external::MockExternalFrame;

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
    /// Test-only failure injection for `set_group_muted`.
    fail_group_mute: bool,
    /// Test-only failure injection for the next `connect`.
    fail_connect: bool,
    devices: Vec<MediaDevice>,
    hook: Option<SignalingEmittedCallback>,
    transport_hook: Option<TransportStateCallback>,
    /// Phase C2e: the peer camera-state hook (replace semantics).
    remote_video_hook: Option<RemoteVideoStateCallback>,
    /// Phase C2j: peer 1:1 screen-share state hook (replace semantics).
    remote_screen_hook: Option<RemoteVideoStateCallback>,
    /// The peer's 1:1 microphone hook (replace semantics).
    remote_audio_hook: Option<RemoteAudioStateCallback>,
    camera_changes: Vec<(i32, Option<String>)>,
    /// Phase C2i: 1:1 screen-share toggle recording for driver tests.
    p2p_screen_share_changes: Vec<(i32, bool)>,
    frame_seq: u64,
    connects: Vec<(i32, ConnectParams)>,
    device_selections: Vec<(i32, Option<String>, Option<String>)>,
    /// Phase C2g: group transport recording for driver tests.
    group_offers: Vec<(i32, i64)>,
    group_roles: Vec<(i32, GroupCallRole)>,
    group_connects: Vec<(i32, String, Option<String>)>,
    group_video_syncs: Vec<(i32, Vec<GroupVideoSource>)>,
    group_camera_changes: Vec<(i32, Option<String>)>,
    screen_share_offers: Vec<i32>,
    screen_share_connects: Vec<(i32, String)>,
    screen_share_stops: Vec<i32>,
    /// Phase C2g: group call ids with a live native presentation
    /// (initialized via `start_screen_share`, cleared by `stop_screen_share`).
    presentations: Vec<i32>,
    group_leaves: Vec<i32>,
    group_mutes: Vec<(i32, bool)>,
    /// Stream mode: the mode the next group connect reports, per chat.
    group_modes: HashMap<i64, GroupCallMode>,
    /// Chats whose group call connected (for `group_call_mode`).
    connected_chats: Vec<i64>,
    /// Test-only: the next `connect_group_call` fails with this code.
    fail_group_connect: Option<i32>,
    fail_group_video_sync: bool,
    device_warms: usize,
    engine_event_hook: Option<EngineEventCallback>,
    live_frame_hook: Option<LiveFrameCallback>,
    stream_qualities: Vec<(i64, StreamQuality)>,
    group_playbacks: Vec<(i32, Option<String>, bool)>,
    stream_sound: HashMap<i32, crate::calls::stream_audio::watch::PlaybackHealth>,
    stream_volumes: Vec<f32>,
    stream_sound_reopens: Vec<i32>,
    call_sound: HashMap<i32, crate::calls::stream_audio::watch::PlaybackHealth>,
    call_sound_reopens: Vec<i32>,
    broadcast_times: Vec<(i64, BroadcastRequestId, i64)>,
    broadcast_parts: Vec<(i64, BroadcastPartAnswer)>,
    /// External frames pushed through `external_frame_sender`.
    external_frames: Vec<MockExternalFrame>,
    /// Conference calls by key, with the encryption each was given.
    conferences: Vec<(i32, Arc<dyn FrameCrypto>)>,
    rekeys: Vec<(i32, i32)>,
    ssrc_owners: Vec<(i32, SsrcOwners)>,
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
        self.inner.lock_or_recover().started_calls.clone()
    }

    pub fn accepted_calls(&self) -> Vec<i32> {
        self.inner.lock_or_recover().accepted_calls.clone()
    }

    pub fn signaling_received(&self) -> Vec<(i32, Vec<u8>)> {
        self.inner.lock_or_recover().signaling_received.clone()
    }

    pub fn hung_up_calls(&self) -> Vec<i32> {
        self.inner.lock_or_recover().hung_up_calls.clone()
    }

    pub fn mute_changes(&self) -> Vec<(i32, bool)> {
        self.inner.lock_or_recover().mute_changes.clone()
    }

    pub fn set_devices(&self, devices: Vec<MediaDevice>) {
        self.inner.lock_or_recover().devices = devices;
    }

    /// Test-only: make `set_group_muted` fail.
    pub fn fail_group_mute(&self) {
        self.inner.lock_or_recover().fail_group_mute = true;
    }

    /// Test-only: make the next `set_muted` calls fail.
    pub fn fail_mute(&self) {
        self.inner.lock_or_recover().fail_mute = true;
    }

    pub fn fail_next_connect(&self) {
        self.inner.lock_or_recover().fail_connect = true;
    }

    pub fn connects(&self) -> Vec<(i32, ConnectParams)> {
        self.inner.lock_or_recover().connects.clone()
    }

    pub fn device_selections(&self) -> Vec<(i32, Option<String>, Option<String>)> {
        self.inner.lock_or_recover().device_selections.clone()
    }

    pub fn camera_changes(&self) -> Vec<(i32, Option<String>)> {
        self.inner.lock_or_recover().camera_changes.clone()
    }

    /// Phase C2g: recorded group transport calls for driver tests.
    pub fn group_offers(&self) -> Vec<(i32, i64)> {
        self.inner.lock_or_recover().group_offers.clone()
    }

    pub fn group_roles(&self) -> Vec<(i32, GroupCallRole)> {
        self.inner.lock_or_recover().group_roles.clone()
    }

    /// Conference calls made, by their current key.
    pub fn conferences(&self) -> Vec<(i32, Arc<dyn FrameCrypto>)> {
        self.inner.lock_or_recover().conferences.clone()
    }

    pub fn rekeys(&self) -> Vec<(i32, i32)> {
        self.inner.lock_or_recover().rekeys.clone()
    }

    pub fn ssrc_owners(&self) -> Vec<(i32, SsrcOwners)> {
        self.inner.lock_or_recover().ssrc_owners.clone()
    }

    pub fn group_connects(&self) -> Vec<(i32, String, Option<String>)> {
        self.inner.lock_or_recover().group_connects.clone()
    }

    pub fn group_video_syncs(&self) -> Vec<(i32, Vec<GroupVideoSource>)> {
        self.inner.lock_or_recover().group_video_syncs.clone()
    }

    pub fn group_camera_changes(&self) -> Vec<(i32, Option<String>)> {
        self.inner.lock_or_recover().group_camera_changes.clone()
    }

    pub fn screen_share_offers(&self) -> Vec<i32> {
        self.inner.lock_or_recover().screen_share_offers.clone()
    }

    pub fn screen_share_connects(&self) -> Vec<(i32, String)> {
        self.inner.lock_or_recover().screen_share_connects.clone()
    }

    pub fn screen_share_stops(&self) -> Vec<i32> {
        self.inner.lock_or_recover().screen_share_stops.clone()
    }

    /// Phase C2i: recorded 1:1 screen-share toggles.
    pub fn p2p_screen_share_changes(&self) -> Vec<(i32, bool)> {
        self.inner
            .lock_or_recover()
            .p2p_screen_share_changes
            .clone()
    }

    pub fn group_mutes(&self) -> Vec<(i32, bool)> {
        self.inner.lock_or_recover().group_mutes.clone()
    }

    pub fn group_leaves(&self) -> Vec<i32> {
        self.inner.lock_or_recover().group_leaves.clone()
    }

    /// Stream mode: the group call in `chat_id` connects in `mode`.
    pub fn set_group_call_mode(&self, chat_id: i64, mode: GroupCallMode) {
        self.inner
            .lock_or_recover()
            .group_modes
            .insert(chat_id, mode);
    }

    /// Test-only: the next `connect_group_call` fails with `code`.
    pub fn fail_next_group_connect(&self, code: i32) {
        self.inner.lock_or_recover().fail_group_connect = Some(code);
    }

    pub fn device_warms(&self) -> usize {
        self.inner.lock_or_recover().device_warms
    }

    pub fn fail_next_group_video_sync(&self) {
        self.inner.lock_or_recover().fail_group_video_sync = true;
    }

    pub fn group_playbacks(&self) -> Vec<(i32, Option<String>, bool)> {
        self.inner.lock_or_recover().group_playbacks.clone()
    }

    pub fn set_stream_sound(
        &self,
        group_call_id: i32,
        health: crate::calls::stream_audio::watch::PlaybackHealth,
    ) {
        self.inner
            .lock_or_recover()
            .stream_sound
            .insert(group_call_id, health);
    }

    pub fn set_call_sound(
        &self,
        call_id: i32,
        health: crate::calls::stream_audio::watch::PlaybackHealth,
    ) {
        self.inner
            .lock_or_recover()
            .call_sound
            .insert(call_id, health);
    }

    pub fn call_sound_reopens(&self) -> Vec<i32> {
        self.inner.lock_or_recover().call_sound_reopens.clone()
    }

    pub fn stream_volumes(&self) -> Vec<f32> {
        self.inner.lock_or_recover().stream_volumes.clone()
    }

    pub fn stream_sound_reopens(&self) -> Vec<i32> {
        self.inner.lock_or_recover().stream_sound_reopens.clone()
    }

    pub fn stream_qualities(&self) -> Vec<(i64, StreamQuality)> {
        self.inner.lock_or_recover().stream_qualities.clone()
    }

    /// `(chat, request, timestamp)` per time answer.
    pub fn broadcast_times(&self) -> Vec<(i64, BroadcastRequestId, i64)> {
        self.inner.lock_or_recover().broadcast_times.clone()
    }

    pub fn broadcast_parts(&self) -> Vec<(i64, BroadcastPartAnswer)> {
        self.inner.lock_or_recover().broadcast_parts.clone()
    }

    /// Deliver a synthetic broadcast request, as the native trampolines do.
    pub fn emit_engine_event(&self, event: EngineEvent) {
        let hook = self.inner.lock_or_recover().engine_event_hook.clone();
        if let Some(hook) = hook {
            hook(event);
        }
    }

    /// Deliver a synthetic group-call or stream picture to `key`'s feed,
    /// assigning the next sequence number.
    pub fn emit_live_frame(&self, key: LiveKey, mut frame: LiveFrame) {
        let mut inner = self.inner.lock_or_recover();
        frame.seq = inner.frame_seq;
        inner.frame_seq += 1;
        let hook = inner.live_frame_hook.clone();
        drop(inner);
        if let Some(hook) = hook {
            hook(key, frame);
        }
    }

    pub fn emit_transport_state(&self, call_id: i32, state: TransportState) {
        let hook = self.inner.lock_or_recover().transport_hook.clone();
        if let Some(hook) = hook {
            hook(call_id, state);
        }
    }

    /// Phase C2e: deliver a synthetic peer camera state to the hook.
    pub fn emit_remote_video_state(&self, call_id: i32, state: RemoteVideoState) {
        let hook = self.inner.lock_or_recover().remote_video_hook.clone();
        if let Some(hook) = hook {
            hook(call_id, state);
        }
    }

    /// Deliver a synthetic peer microphone state to the hook.
    pub fn emit_remote_audio_muted(&self, call_id: i32, muted: bool) {
        let hook = self.inner.lock_or_recover().remote_audio_hook.clone();
        if let Some(hook) = hook {
            hook(call_id, muted);
        }
    }

    /// Phase C2j: deliver a synthetic peer screen-share state to the hook.
    pub fn emit_remote_screen_state(&self, call_id: i32, state: RemoteVideoState) {
        let hook = self.inner.lock_or_recover().remote_screen_hook.clone();
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
        let mut inner = self.inner.lock_or_recover();
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
        let mut inner = self.inner.lock_or_recover();
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
        let mut inner = self.inner.lock_or_recover();
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
        let hook = self.inner.lock_or_recover().hook.clone();
        if let Some(hook) = hook {
            hook(call_id, data.to_vec());
        }
    }

    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback) {
        self.inner.lock_or_recover().hook = Some(callback);
    }

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback) {
        self.inner.lock_or_recover().transport_hook = Some(callback);
    }

    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        self.inner.lock_or_recover().remote_video_hook = Some(callback);
    }

    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        self.inner.lock_or_recover().remote_screen_hook = Some(callback);
    }

    fn set_remote_audio_state_callback(&mut self, callback: RemoteAudioStateCallback) {
        self.inner.lock_or_recover().remote_audio_hook = Some(callback);
    }

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
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
        let mut inner = self.inner.lock_or_recover();
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
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if inner.calls.remove(&call_id).is_some() {
            inner.hung_up_calls.push(call_id);
        }
        Ok(())
    }

    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
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

    fn set_camera(&mut self, call_id: i32, camera: Option<&str>) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner
            .camera_changes
            .push((call_id, camera.map(str::to_owned)));
        Ok(())
    }

    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
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
        role: GroupCallRole,
    ) -> Result<String, EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.group_offers.push((group_call_id, chat_id));
        inner.group_roles.push((group_call_id, role));
        Ok(format!("mock-group-offer-{group_call_id}"))
    }

    fn create_conference_call(
        &mut self,
        key: i32,
        crypto: Arc<dyn FrameCrypto>,
    ) -> Result<String, EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.group_offers.push((key, 0));
        inner.group_roles.push((key, GroupCallRole::Member));
        inner.conferences.retain(|(old, _)| *old != key);
        inner.conferences.push((key, crypto));
        Ok(format!("mock-conference-offer-{key}"))
    }

    fn rekey_group_call(&mut self, from: i32, to: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.conferences.iter().any(|(key, _)| *key == from) {
            return Err(EngineError::NoSuchCall(from));
        }
        inner.conferences.retain(|(key, _)| *key != to);
        for (key, _) in inner.conferences.iter_mut() {
            if *key == from {
                *key = to;
            }
        }
        for (id, _) in inner.group_offers.iter_mut() {
            if *id == from {
                *id = to;
            }
        }
        inner.rekeys.push((from, to));
        Ok(())
    }

    fn set_group_ssrc_owners(&mut self, group_call_id: i32, owners: &SsrcOwners) {
        let mut inner = self.inner.lock_or_recover();
        if inner.ssrc_owners.last() != Some(&(group_call_id, owners.clone())) {
            inner.ssrc_owners.push((group_call_id, owners.clone()));
        }
    }

    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if let Some(code) = inner.fail_group_connect.take() {
            return Err(EngineError::Engine {
                op: "qc_group_set_join_response",
                code,
            });
        }
        inner.group_connects.push((
            group_call_id,
            answer_payload.to_owned(),
            camera.map(str::to_owned),
        ));
        let chat_id = inner
            .group_offers
            .iter()
            .rev()
            .find(|(id, _)| *id == group_call_id)
            .map(|(_, chat_id)| *chat_id);
        if let Some(chat_id) = chat_id
            && !inner.connected_chats.contains(&chat_id)
        {
            inner.connected_chats.push(chat_id);
        }
        Ok(())
    }

    fn set_engine_event_callback(&mut self, callback: EngineEventCallback) {
        self.inner.lock_or_recover().engine_event_hook = Some(callback);
    }

    fn set_live_frame_callback(&mut self, callback: LiveFrameCallback) {
        self.inner.lock_or_recover().live_frame_hook = Some(callback);
    }

    fn group_call_mode(&self, chat_id: i64) -> Option<GroupCallMode> {
        let inner = self.inner.lock_or_recover();
        inner
            .connected_chats
            .contains(&chat_id)
            .then(|| inner.group_modes.get(&chat_id).copied().unwrap_or_default())
    }

    fn set_stream_quality(&mut self, chat_id: i64, quality: StreamQuality) {
        self.inner
            .lock_or_recover()
            .stream_qualities
            .push((chat_id, quality));
    }

    fn stream_quality(&self, chat_id: i64) -> Option<StreamQuality> {
        self.inner
            .lock_or_recover()
            .stream_qualities
            .iter()
            .rev()
            .find(|(chat, _)| *chat == chat_id)
            .map(|(_, quality)| *quality)
    }

    fn answer_broadcast_time(
        &mut self,
        chat_id: i64,
        request: BroadcastRequestId,
        timestamp_ms: i64,
    ) -> Result<(), EngineError> {
        self.inner
            .lock_or_recover()
            .broadcast_times
            .push((chat_id, request, timestamp_ms));
        Ok(())
    }

    fn answer_broadcast_part(
        &mut self,
        chat_id: i64,
        answer: &BroadcastPartAnswer,
    ) -> Result<(), EngineError> {
        self.inner
            .lock_or_recover()
            .broadcast_parts
            .push((chat_id, answer.clone()));
        Ok(())
    }

    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if std::mem::take(&mut inner.fail_group_video_sync) {
            return Err(EngineError::Engine {
                op: "sync_group_video",
                code: -1,
            });
        }
        inner
            .group_video_syncs
            .push((group_call_id, sources.to_vec()));
        Ok(())
    }

    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .group_camera_changes
            .push((group_call_id, camera.map(str::to_owned)));
        Ok(())
    }

    fn start_screen_share(&mut self, group_call_id: i32) -> Result<String, EngineError> {
        let mut inner = self.inner.lock_or_recover();
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
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .screen_share_connects
            .push((group_call_id, answer_payload.to_owned()));
        Ok(())
    }

    fn stop_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.screen_share_stops.push(group_call_id);
        inner.presentations.retain(|id| *id != group_call_id);
        Ok(())
    }

    fn presentation_active(&self, group_call_id: i32) -> bool {
        self.inner
            .lock_or_recover()
            .presentations
            .contains(&group_call_id)
    }

    fn set_group_playback(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
        muted: bool,
    ) -> Result<(), EngineError> {
        self.inner.lock_or_recover().group_playbacks.push((
            group_call_id,
            speaker.map(str::to_owned),
            muted,
        ));
        Ok(())
    }

    fn set_stream_volume(&mut self, volume: f32) {
        self.inner.lock_or_recover().stream_volumes.push(volume);
    }

    fn stream_sound_health(
        &self,
        group_call_id: i32,
    ) -> Option<crate::calls::stream_audio::watch::PlaybackHealth> {
        self.inner
            .lock_or_recover()
            .stream_sound
            .get(&group_call_id)
            .copied()
    }

    /// As the tgcalls engine: a new output replaces the old one, so the
    /// stream keeps a (fresh) health and its volume.
    fn reopen_stream_sound(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if let Some(health) = inner.stream_sound.get_mut(&group_call_id) {
            *health = Default::default();
        }
        inner.stream_sound_reopens.push(group_call_id);
        Ok(())
    }

    fn call_sound_health(
        &self,
        call_id: i32,
    ) -> Option<crate::calls::stream_audio::watch::PlaybackHealth> {
        self.inner
            .lock_or_recover()
            .call_sound
            .get(&call_id)
            .copied()
    }

    /// As the tgcalls engine: a new output replaces the old one.
    fn reopen_call_sound(&mut self, call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if let Some(health) = inner.call_sound.get_mut(&call_id) {
            *health = Default::default();
        }
        inner.call_sound_reopens.push(call_id);
        Ok(())
    }

    fn set_group_muted(&mut self, group_call_id: i32, muted: bool) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if inner.fail_group_mute {
            return Err(EngineError::Engine {
                op: "set_group_muted",
                code: -1,
            });
        }
        inner.group_mutes.push((group_call_id, muted));
        Ok(())
    }

    fn leave_group_call(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.group_leaves.push(group_call_id);
        inner.conferences.retain(|(key, _)| *key != group_call_id);
        let chat_id = inner
            .group_offers
            .iter()
            .rev()
            .find(|(id, _)| *id == group_call_id)
            .map(|(_, chat_id)| *chat_id);
        inner.connected_chats.retain(|chat| Some(*chat) != chat_id);
        // Mirror the real engine: a live presentation is stopped first
        // (privacy — capture ends before the call).
        if inner.presentations.contains(&group_call_id) {
            inner.screen_share_stops.push(group_call_id);
        }
        inner.presentations.retain(|id| *id != group_call_id);
        Ok(())
    }

    fn external_frame_sender(
        &mut self,
        target: CaptureTarget,
    ) -> Option<Arc<dyn ExternalFrameSender>> {
        self.mock_external_sender(target)
    }

    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError> {
        let inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        Ok(inner.devices.clone())
    }

    fn warm_devices(&self) {
        self.inner.lock_or_recover().device_warms += 1;
    }

    fn protocol(&self) -> EngineProtocol {
        if self.is_available() {
            Self::engine_protocol()
        } else {
            EngineProtocol::signaling_only()
        }
    }

    fn is_available(&self) -> bool {
        self.inner.lock_or_recover().available
    }
}

#[cfg(test)]
mod tests;

//! Group calls (video chats) on tgcalls, driven the way Telegram Desktop
//! drives them (calls_group_call.cpp): one `qc_group` instance per call,
//! and a second one for its presentation (the shared screen) while you
//! share it.
//!
//! The library has no devices: Quill's microphone is pushed in through
//! the external frame sender, its screen through the presentation's
//! external source, and the call's sound comes out to a `StreamPlayer`.
use super::group_callbacks::{GroupContext, on_local_video};
use super::group_config::{RTMP_ENDPOINT, join_mode, requested_video_json, unified_min_quality};
use super::*;
use crate::locks::LockExt;
use quill_calls_sys::{QC_GROUP_MODE_BROADCAST, QC_GROUP_MODE_RTC, qc_group, qc_video_capture};
use std::collections::HashSet;
use std::ffi::CString;

/// One group call, by TDLib group call id.
pub(super) struct GroupCall {
    chat_id: i64,
    role: GroupCallRole,
    context: Arc<GroupContext>,
    handle: NonNull<qc_group>,
    /// Feeder threads' way in; cleared before the handles go away.
    gate: Arc<sender::Gate>,
    /// The join answer was applied (`connect_group_call`).
    connected: bool,
    mode: GroupCallMode,
    muted: bool,
    /// The camera to send, by device id, and its source while it runs.
    camera_id: Option<String>,
    camera: Option<NonNull<qc_video_capture>>,
    speaker: Option<String>,
    playback_muted: bool,
    sound: Option<StreamPlayer>,
    /// The participant videos last asked for, kept so a connect or a
    /// new stream quality asks again without waiting for the driver.
    video_sources: Vec<GroupVideoSource>,
    /// The requested-video JSON last given to tgcalls.
    requested_video: Option<String>,
    /// Endpoints with an incoming video output.
    outputs: HashSet<String>,
    presentation: Option<Presentation>,
    stream_quality: Option<StreamQuality>,
}

/// The screen-sharing instance beside a call (tdesktop's `_screenInstance`).
struct Presentation {
    /// What the instance's callbacks read; outlives `qc_group_destroy`.
    _context: Arc<GroupContext>,
    handle: NonNull<qc_group>,
    /// The external screencast source Quill's screen capture feeds.
    screen: NonNull<qc_video_capture>,
    connected: bool,
}

fn text(op: &'static str, value: &str) -> Result<CString, EngineError> {
    CString::new(value).map_err(|_| EngineError::Engine { op, code: -1 })
}

/// A new instance and its join payload; `None` (logged) when tgcalls
/// refuses it.
fn create_instance(
    api: &Loader,
    context: &Arc<GroupContext>,
    video: *mut qc_video_capture,
) -> Option<(NonNull<qc_group>, String)> {
    let config = if context.presentation {
        c"{\"presentation\":true}"
    } else {
        c"{\"presentation\":false}"
    };
    let callbacks = context.callbacks();
    // SAFETY: NUL-ended config; the context behind `callbacks.user` lives
    // in the engine until `qc_group_destroy` returns; the instance takes
    // its own reference to `video` (or none for NULL).
    let handle = unsafe { (api.qc_group_create)(config.as_ptr(), &callbacks, video) };
    let handle = NonNull::new(handle)?;
    // SAFETY: a live instance, not called from one of its callbacks; the
    // payload is the caller's to free.
    let raw = unsafe { (api.qc_group_join_payload)(handle.as_ptr(), std::ptr::null_mut()) };
    match take_string(api, raw) {
        Some(payload) => Some((handle, payload)),
        None => {
            // SAFETY: the instance made above, destroyed once.
            unsafe { (api.qc_group_destroy)(handle.as_ptr()) };
            None
        }
    }
}

/// Applies a join answer the way tdesktop's
/// `handlePossibleCreateOrJoinResponse` does: the connection mode first,
/// then (for RTC) the answer itself.
fn apply_answer(api: &Loader, handle: NonNull<qc_group>, answer: &CString, mode: GroupCallMode) {
    match mode {
        GroupCallMode::Rtc => {
            // SAFETY: a live instance; `answer` is NUL-ended and outlives
            // these synchronous calls.
            unsafe {
                (api.qc_group_set_connection_mode)(handle.as_ptr(), QC_GROUP_MODE_RTC, 0);
                (api.qc_group_set_join_response)(handle.as_ptr(), answer.as_ptr());
            }
        }
        GroupCallMode::Stream | GroupCallMode::Rtmp => {
            let unified = i32::from(mode == GroupCallMode::Rtmp);
            // SAFETY: a live instance.
            unsafe {
                (api.qc_group_set_connection_mode)(
                    handle.as_ptr(),
                    QC_GROUP_MODE_BROADCAST,
                    unified,
                )
            };
        }
    }
}

/// Clears the preview and drops Quill's reference to the call's camera.
fn release_camera(api: &Loader, call: &mut GroupCall) {
    if let Some(capture) = call.camera.take() {
        // SAFETY: Quill's live reference; once the preview is cleared no
        // picture reaches the context, then the reference goes.
        unsafe {
            (api.qc_video_capture_set_preview)(capture.as_ptr(), None, std::ptr::null_mut());
            (api.qc_video_capture_free)(capture.as_ptr());
        }
    }
}

/// Ends the presentation: no screen picture reaches it afterwards.
fn stop_presentation(api: &Loader, call: &mut GroupCall) {
    let Some(presentation) = call.presentation.take() else {
        return;
    };
    call.gate.set_screen(None);
    // SAFETY: the presentation's live handle, destroyed once and never
    // from its own callback; then Quill's reference to its screen source,
    // which the gate no longer borrows. The context stays alive until
    // both return.
    unsafe {
        (api.qc_group_destroy)(presentation.handle.as_ptr());
        (api.qc_video_capture_free)(presentation.screen.as_ptr());
    }
}

/// Ends the call in the library, its presentation first so screen capture
/// stops before the call: no callback or feeder reaches it after.
pub(super) fn destroy_group(api: &Loader, mut call: GroupCall) {
    stop_presentation(api, &mut call);
    call.gate.close();
    // SAFETY: a live instance destroyed once, never from its own callback
    // (the engine runs on the connect driver).
    unsafe { (api.qc_group_destroy)(call.handle.as_ptr()) };
    release_camera(api, &mut call);
    call.context.sound.store(None);
    call.sound = None;
}

impl TgcallsEngine {
    fn group(&self, group_call_id: i32) -> Result<&GroupCall, EngineError> {
        self.groups
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))
    }

    fn group_mut(&mut self, group_call_id: i32) -> Result<&mut GroupCall, EngineError> {
        self.groups
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))
    }

    /// A fresh instance for `group_call_id` (a rejoin replaces the old
    /// one) and its join payload, tdesktop's `GroupJoinPayload` JSON,
    /// which `joinVideoChat` carries as is.
    pub(super) fn create_group(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
        role: GroupCallRole,
    ) -> Result<String, EngineError> {
        if let Some(old) = self.groups.remove(&group_call_id) {
            destroy_group(self.api, old);
        }
        let context = Arc::new(GroupContext::new(
            group_call_id,
            chat_id,
            false,
            &self.callback,
        ));
        let (handle, payload) = create_instance(self.api, &context, std::ptr::null_mut()).ok_or(
            EngineError::Engine {
                op: "qc_group_create",
                code: -1,
            },
        )?;
        crate::advanced::debug_mode::call_log(&format!(
            "[call] group {group_call_id} tgcalls create ({role:?})"
        ));
        self.groups.insert(
            group_call_id,
            GroupCall {
                chat_id,
                role,
                context,
                handle,
                gate: Arc::new(sender::Gate::default()),
                connected: false,
                mode: GroupCallMode::Rtc,
                muted: true,
                camera_id: None,
                camera: None,
                speaker: None,
                playback_muted: false,
                sound: None,
                video_sources: Vec::new(),
                requested_video: None,
                outputs: HashSet::new(),
                presentation: None,
                stream_quality: None,
            },
        );
        Ok(payload)
    }

    /// Applies the `joinVideoChat` answer once, then the call's sound,
    /// microphone and `camera`. A stream is receive-only: nothing is sent.
    pub(super) fn connect_group(
        &mut self,
        group_call_id: i32,
        answer: &str,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        if call.connected {
            return Ok(());
        }
        let answer = text("qc_group_set_join_response", answer)?;
        let mode = join_mode(&answer.to_string_lossy());
        apply_answer(api, call.handle, &answer, mode);
        crate::advanced::debug_mode::call_log(&format!(
            "[call] group {group_call_id} tgcalls joined ({mode:?})"
        ));
        call.connected = true;
        call.mode = mode;
        call.camera_id = camera.map(str::to_owned);
        if call.role == GroupCallRole::Member && !mode.is_stream() {
            call.gate.open_group(api, call.handle.as_ptr());
        }
        // The mode decides where pictures go and whether an RTMP stream's
        // unified video is asked for: route every video again.
        call.requested_video = None;
        let sources = call.video_sources.clone();
        self.apply_group_mute(group_call_id)?;
        self.open_group_sound(group_call_id)?;
        self.sync_group_videos(group_call_id, &sources)?;
        self.apply_group_camera(group_call_id)
    }

    /// Whether the call sends anything: a member's RTC call does.
    fn group_sends(call: &GroupCall) -> bool {
        call.connected && call.role == GroupCallRole::Member && !call.mode.is_stream()
    }

    fn apply_group_mute(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group(group_call_id)?;
        // A viewer's instance stays muted, as it starts.
        let muted = call.muted || call.role == GroupCallRole::Viewer;
        // SAFETY: a live instance.
        unsafe { (api.qc_group_set_muted)(call.handle.as_ptr(), i32::from(muted)) };
        Ok(())
    }

    /// Opens (or reopens, for another speaker) the output the call's
    /// sound plays on. A failed open leaves the call silent and logs why.
    fn open_group_sound(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let stream_volume = self.stream_volume;
        let call = self.group_mut(group_call_id)?;
        let name = devices::speaker_name(call.speaker.as_deref());
        // A stream plays at the media viewer's volume, as in tdesktop.
        let volume = if call.mode.is_stream() {
            stream_volume
        } else {
            1.0
        };
        match StreamPlayer::open(name.as_deref(), volume, call.playback_muted) {
            Ok(player) => {
                call.context.sound.store(Some(player.audio().clone()));
                call.sound = Some(player);
            }
            Err(err) => {
                crate::advanced::debug_mode::call_log(&format!(
                    "[call] group {group_call_id} sound output did not open ({err})"
                ));
                call.context.sound.store(None);
                call.sound = None;
            }
        }
        Ok(())
    }

    /// Sends the call's camera, or stops sending video without one.
    /// Only a member's connected RTC call sends; the choice is kept
    /// for the connect otherwise.
    fn apply_group_camera(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        let sends = Self::group_sends(call);
        let wanted = call.camera_id.clone().filter(|_| sends);
        match (wanted, call.camera) {
            (Some(device), Some(capture)) => {
                let device = text("qc_video_capture_switch_camera", &device)?;
                // SAFETY: `capture` is Quill's live reference.
                unsafe { (api.qc_video_capture_switch_camera)(capture.as_ptr(), device.as_ptr()) };
                Ok(())
            }
            (Some(device), None) => {
                let device = text("qc_video_capture_camera", &device)?;
                // SAFETY: a NUL-ended id; the library returns a new source.
                let capture = unsafe { (api.qc_video_capture_camera)(device.as_ptr()) };
                let capture = NonNull::new(capture).ok_or(EngineError::NoCamera)?;
                // SAFETY: the context outlives the preview: it is cleared
                // before the capture is freed (`release_camera`).
                unsafe {
                    (api.qc_video_capture_set_preview)(
                        capture.as_ptr(),
                        Some(on_local_video),
                        call.context.as_user(),
                    );
                }
                call.camera = Some(capture);
                // SAFETY: both live; the instance takes its own reference.
                unsafe { (api.qc_group_set_video_capture)(call.handle.as_ptr(), capture.as_ptr()) };
                Ok(())
            }
            (None, Some(_)) => {
                // SAFETY: a live instance; NULL stops sending video.
                unsafe {
                    (api.qc_group_set_video_capture)(call.handle.as_ptr(), std::ptr::null_mut())
                };
                release_camera(api, call);
                Ok(())
            }
            (None, None) => Ok(()),
        }
    }

    pub(super) fn set_group_camera_device(
        &mut self,
        group_call_id: i32,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let call = self.group_mut(group_call_id)?;
        let previous = std::mem::replace(&mut call.camera_id, camera.map(str::to_owned));
        if let Err(err) = self.apply_group_camera(group_call_id) {
            self.group_mut(group_call_id)?.camera_id = previous;
            return Err(err);
        }
        Ok(())
    }

    /// Receives exactly `sources`' videos, plus the unified video of a
    /// connected RTMP stream (tdesktop's `RtmpEndpointId`): tgcalls is told
    /// only when the set (or its order, or the stream's quality) changed,
    /// and each new endpoint gets an output before stale ones lose
    /// theirs. In a stream every picture is the stream's, as the stream
    /// window shows one; in a call each goes to its participant's tile.
    pub(super) fn sync_group_videos(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        let unified = (call.connected && call.mode == GroupCallMode::Rtmp)
            .then(|| unified_min_quality(call.stream_quality));
        let requested = requested_video_json(sources, unified);
        if call.video_sources != sources {
            call.video_sources = sources.to_vec();
        }
        if call.requested_video.as_deref() == Some(requested.as_str()) {
            return Ok(());
        }
        let stream = call.connected && call.mode.is_stream();
        let key = |source: &GroupVideoSource| {
            if stream {
                LiveKey::Stream {
                    chat_id: call.chat_id,
                }
            } else {
                LiveKey::Video {
                    call_id: group_call_id,
                    user_id: source.user_id,
                    screen: source.screen,
                }
            }
        };
        let mut owners: HashMap<String, LiveKey> = sources
            .iter()
            .map(|source| (source.endpoint.clone(), key(source)))
            .collect();
        let mut endpoints = sources
            .iter()
            .map(|source| {
                text("qc_group_add_incoming_video", &source.endpoint)
                    .map(|native| (source.endpoint.clone(), native))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if unified.is_some() {
            owners.insert(
                RTMP_ENDPOINT.to_owned(),
                LiveKey::Stream {
                    chat_id: call.chat_id,
                },
            );
            endpoints.push((
                RTMP_ENDPOINT.to_owned(),
                text("qc_group_add_incoming_video", RTMP_ENDPOINT)?,
            ));
        }
        let channels = text("qc_group_set_requested_video", &requested)?;
        // SAFETY: a live instance; `channels` is NUL-ended.
        let accepted =
            unsafe { (api.qc_group_set_requested_video)(call.handle.as_ptr(), channels.as_ptr()) };
        if accepted == 0 {
            return Err(EngineError::Engine {
                op: "qc_group_set_requested_video",
                code: -1,
            });
        }
        // Pictures route by the new owners from here on.
        *call.context.endpoints.lock_or_recover() = owners;
        for (endpoint, native) in &endpoints {
            if call.outputs.insert(endpoint.clone()) {
                // SAFETY: a live instance; `native` is NUL-ended.
                unsafe { (api.qc_group_add_incoming_video)(call.handle.as_ptr(), native.as_ptr()) };
            }
        }
        let wanted: HashSet<&String> = endpoints.iter().map(|(endpoint, _)| endpoint).collect();
        let stale: Vec<String> = call
            .outputs
            .iter()
            .filter(|endpoint| !wanted.contains(endpoint))
            .cloned()
            .collect();
        for endpoint in stale {
            call.outputs.remove(&endpoint);
            let native = text("qc_group_remove_incoming_video", &endpoint)?;
            // SAFETY: a live instance; `native` is NUL-ended.
            unsafe { (api.qc_group_remove_incoming_video)(call.handle.as_ptr(), native.as_ptr()) };
        }
        call.requested_video = Some(requested);
        Ok(())
    }

    pub(super) fn set_group_mute(
        &mut self,
        group_call_id: i32,
        muted: bool,
    ) -> Result<(), EngineError> {
        self.group_mut(group_call_id)?.muted = muted;
        self.apply_group_mute(group_call_id)
    }

    /// The output device and your local mute of the call's sound: a new
    /// device reopens the output, a mute only silences it.
    pub(super) fn set_group_output(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
        muted: bool,
    ) -> Result<(), EngineError> {
        let call = self.group_mut(group_call_id)?;
        let reopen = call.speaker.as_deref() != speaker && call.connected;
        call.speaker = speaker.map(str::to_owned);
        call.playback_muted = muted;
        if let Some(player) = call.sound.as_ref() {
            player.audio().gain().set_muted(muted);
        }
        if reopen {
            self.open_group_sound(group_call_id)?;
        }
        Ok(())
    }

    /// The presentation's instance and its payload for
    /// `startGroupCallScreenSharing`; a running one is replaced.
    pub(super) fn start_presentation(&mut self, group_call_id: i32) -> Result<String, EngineError> {
        let api = self.api;
        let shared = Arc::clone(&self.callback);
        let call = self.group_mut(group_call_id)?;
        stop_presentation(api, call);
        // SAFETY: returns a new external screencast source owned here.
        let screen = unsafe { (api.qc_video_capture_external)(1) };
        let screen = NonNull::new(screen).ok_or(EngineError::NoScreenSource)?;
        let context = Arc::new(GroupContext::new(
            group_call_id,
            call.chat_id,
            true,
            &shared,
        ));
        let Some((handle, payload)) = create_instance(api, &context, screen.as_ptr()) else {
            // SAFETY: Quill's only reference to the source made above.
            unsafe { (api.qc_video_capture_free)(screen.as_ptr()) };
            return Err(EngineError::Engine {
                op: "qc_group_create",
                code: -1,
            });
        };
        call.presentation = Some(Presentation {
            _context: context,
            handle,
            screen,
            connected: false,
        });
        Ok(payload)
    }

    /// Applies the `startGroupCallScreenSharing` answer once; screen
    /// pictures reach the presentation from then on.
    pub(super) fn connect_presentation(
        &mut self,
        group_call_id: i32,
        answer: &str,
    ) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        let presentation = call.presentation.as_mut().ok_or(EngineError::Engine {
            op: "qc_group_set_join_response",
            code: -1,
        })?;
        if presentation.connected {
            return Ok(());
        }
        let answer = text("qc_group_set_join_response", answer)?;
        // tdesktop's screen instance is always RTC.
        apply_answer(api, presentation.handle, &answer, GroupCallMode::Rtc);
        presentation.connected = true;
        call.gate.set_screen(Some(presentation.screen.as_ptr()));
        Ok(())
    }

    pub(super) fn stop_group_presentation(
        &mut self,
        group_call_id: i32,
    ) -> Result<(), EngineError> {
        let api = self.api;
        stop_presentation(api, self.group_mut(group_call_id)?);
        Ok(())
    }

    pub(super) fn has_presentation(&self, group_call_id: i32) -> bool {
        self.groups
            .get(&group_call_id)
            .is_some_and(|call| call.presentation.is_some())
    }

    pub(super) fn leave_group(&mut self, group_call_id: i32) {
        if let Some(call) = self.groups.remove(&group_call_id) {
            crate::advanced::debug_mode::call_log(&format!(
                "[call] group {group_call_id} tgcalls leave"
            ));
            destroy_group(self.api, call);
        }
    }

    /// The sender for a member's connected RTC call: a stream (and a
    /// viewer) sends nothing.
    pub(super) fn group_sender(&self, group_call_id: i32) -> Option<Arc<dyn ExternalFrameSender>> {
        let call = self.groups.get(&group_call_id)?;
        Self::group_sends(call).then(|| Arc::clone(&call.gate) as Arc<dyn ExternalFrameSender>)
    }

    pub(super) fn connected_group_mode(&self, chat_id: i64) -> Option<GroupCallMode> {
        self.groups
            .values()
            .find(|call| call.chat_id == chat_id && call.connected)
            .map(|call| call.mode)
    }

    /// The stream tile's quality; an RTMP stream asks for its unified
    /// video again at once when the least quality it needs changed.
    pub(super) fn set_group_stream_quality(&mut self, chat_id: i64, quality: StreamQuality) {
        let Some((&group_call_id, call)) = self
            .groups
            .iter_mut()
            .find(|(_, call)| call.chat_id == chat_id)
        else {
            return;
        };
        call.stream_quality = Some(quality);
        let sources = call.video_sources.clone();
        if let Err(err) = self.sync_group_videos(group_call_id, &sources) {
            crate::advanced::debug_mode::call_log(&format!(
                "[call] group {group_call_id} stream quality not applied ({err})"
            ));
        }
    }

    pub(super) fn group_stream_quality(&self, chat_id: i64) -> Option<StreamQuality> {
        self.groups
            .values()
            .find(|call| call.chat_id == chat_id)
            .and_then(|call| call.stream_quality)
    }

    /// The media viewer's volume for every stream, now and later.
    pub(super) fn set_streams_volume(&mut self, volume: f32) {
        self.stream_volume = volume;
        for call in self.groups.values() {
            if call.mode.is_stream()
                && let Some(player) = call.sound.as_ref()
            {
                player.audio().gain().set_volume(volume);
            }
        }
    }

    pub(super) fn group_stream_health(
        &self,
        group_call_id: i32,
    ) -> Option<crate::calls::stream_audio::watch::PlaybackHealth> {
        let call = self.groups.get(&group_call_id)?;
        if !call.mode.is_stream() {
            return None;
        }
        call.sound.as_ref().map(|player| player.audio().health())
    }

    /// There is no device output inside the library: reopen Quill's.
    pub(super) fn reopen_group_sound(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        self.open_group_sound(group_call_id)
    }
}

#[path = "group_stream.rs"]
mod stream;

#[cfg(test)]
#[path = "group_tests.rs"]
mod tests;

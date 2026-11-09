//! The pumps every ingested update runs once the reducer applied it.
//! What the engine, the microphone or the devices report between updates
//! is pumped per poll instead (`request_tick`): your own level
//! (`pump_call_audio`), the engine's reports, the call media and the
//! connected group calls' transport.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Run every post-ingest pump, even when an earlier one fails to send:
    /// a 1:1 signaling send error must not skip the group-call transport
    /// (whose "before" ids exist only for this update), the call media,
    /// `setTdlibParameters` or the membership probe. The first error is
    /// returned once all have run.
    ///
    /// Each pump here reacts to what the update changed: the 1:1 and group
    /// call ids before it, the auth state, the open chat's membership.
    pub(super) fn run_post_ingest_pumps(
        &mut self,
        active_call_before: Option<i32>,
        bridge_signaling: Option<(i32, Vec<u8>)>,
        group_calls_before: [Option<i32>; 2],
    ) -> Result<(), ConnectSendError> {
        let call = self.pump_call_engine(active_call_before, bridge_signaling);
        let group_call = self.pump_group_call_transport(group_calls_before);
        self.pump_call_media(std::time::Instant::now());
        let parameters = self.maybe_send_parameters();
        let membership = self.maybe_probe_channel_membership();
        call.and(group_call).and(parameters).and(membership)
    }
}

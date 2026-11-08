//! The pumps every ingested update runs once the reducer applied it.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Run every post-ingest pump, even when an earlier one fails to send:
    /// a 1:1 signaling send error must not skip the group-call transport
    /// (whose "before" ids exist only for this update), the call media,
    /// `setTdlibParameters` or the membership probe. The first error is
    /// returned once all have run.
    pub(super) fn run_post_ingest_pumps(
        &mut self,
        active_call_before: Option<i32>,
        bridge_signaling: Option<(i32, Vec<u8>)>,
        group_calls_before: [Option<i32>; 2],
    ) -> Result<(), ConnectSendError> {
        let call = self.pump_call_engine(active_call_before, bridge_signaling);
        let group_call = self.pump_group_call_transport(group_calls_before);
        self.pump_call_media(std::time::Instant::now());
        let audio = self.pump_call_audio();
        let parameters = self.maybe_send_parameters();
        let membership = self.maybe_probe_channel_membership();
        call.and(group_call)
            .and(audio)
            .and(parameters)
            .and(membership)
    }
}

//! Connect driver: tell the session how a watched request ended.
use super::*;
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::EnvelopePayload;

impl<S: JsonSender> ConnectDriver<S> {
    /// An answer arrived: a request a box waits on (`Session::bots.sends`)
    /// ends as answered, or as failed when the answer is an error. Read
    /// before `apply` takes the pending request.
    pub(crate) fn settle_watched_send(&mut self, owned: &OwnedEnvelope) {
        if let Some(request) = owned.envelope.extra {
            let ok = !matches!(owned.envelope.payload, EnvelopePayload::Error(_));
            self.session.bots.sends.settle(request, ok);
        }
    }
}

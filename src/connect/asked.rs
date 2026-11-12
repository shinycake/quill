//! The requests this driver sent and has not seen answered, for the senders
//! that have no stage of their own (mark all read, clear recents, a menu's
//! facts): "is my request still out" is the id this driver recorded when it
//! sent, looked up in the registry, never "some request with this purpose".
//! A request the answer, the error, the sweep or the account change took
//! out of the registry is out no longer, so nothing here needs clearing by
//! hand.
use crate::ids::RequestId;
use crate::state::{RequestPurpose, RequestRegistry};

#[derive(Debug, Default)]
pub(crate) struct Asked {
    sent: Vec<(RequestPurpose, RequestId)>,
}

impl Asked {
    /// `id` went out for `purpose`; a request of that purpose recorded
    /// before is replaced (the newer one is the one that is waited for).
    pub(crate) fn sent(
        &mut self,
        requests: &RequestRegistry,
        purpose: RequestPurpose,
        id: RequestId,
    ) {
        self.sent
            .retain(|(kept, request)| *kept != purpose && requests.get(*request).is_some());
        self.sent.push((purpose, id));
    }

    /// The request recorded for `purpose` is still waiting for its answer.
    pub(crate) fn is_out(&self, requests: &RequestRegistry, purpose: RequestPurpose) -> bool {
        self.out(requests, purpose).is_some()
    }

    /// The request recorded for `purpose`, while it is waiting for its answer.
    pub(crate) fn out(
        &self,
        requests: &RequestRegistry,
        purpose: RequestPurpose,
    ) -> Option<RequestId> {
        self.sent
            .iter()
            .find(|(kept, request)| *kept == purpose && requests.get(*request).is_some())
            .map(|(_, request)| *request)
    }

    /// Stop waiting for the request recorded for `purpose`: its answer, if
    /// it still comes, finds nothing waiting. Returns the id that was out.
    pub(crate) fn cancel(
        &mut self,
        requests: &mut RequestRegistry,
        purpose: RequestPurpose,
    ) -> Option<RequestId> {
        let id = self.out(requests, purpose)?;
        self.sent.retain(|(_, request)| *request != id);
        requests.take(id);
        Some(id)
    }
}

impl<S: crate::connect::JsonSender> crate::connect::ConnectDriver<S> {
    /// Stop waiting for the request a stage held (it is replaced or
    /// cancelled): its answer, if it still comes, finds nothing waiting.
    pub(crate) fn drop_request(&mut self, request: Option<RequestId>) {
        if let Some(request) = request {
            self.session.requests.take(request);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::AccountKey;
    use crate::state::Session;

    fn session() -> Session {
        let sink: std::sync::Arc<dyn crate::diagnostics::DiagnosticSink> =
            std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        Session::new(AccountKey::primary(), sink)
    }

    #[test]
    fn a_request_is_out_until_the_registry_drops_it() {
        let mut session = session();
        let mut asked = Asked::default();
        let purpose = RequestPurpose::GetMe;
        assert!(!asked.is_out(&session.requests, purpose));
        let id = session.request(purpose, None);
        asked.sent(&session.requests, purpose, id);
        assert!(asked.is_out(&session.requests, purpose));
        session.requests.take(id);
        assert!(!asked.is_out(&session.requests, purpose));
    }

    #[test]
    fn another_sender_with_the_same_purpose_is_not_my_request() {
        let mut session = session();
        let mut asked = Asked::default();
        let purpose = RequestPurpose::GetMe;
        let _theirs = session.request(purpose, None);
        assert!(!asked.is_out(&session.requests, purpose));
        let mine = session.request(purpose, None);
        asked.sent(&session.requests, purpose, mine);
        assert_eq!(asked.out(&session.requests, purpose), Some(mine));
    }

    #[test]
    fn cancelling_takes_only_my_request_out() {
        let mut session = session();
        let mut asked = Asked::default();
        let purpose = RequestPurpose::GetMe;
        let mine = session.request(purpose, None);
        asked.sent(&session.requests, purpose, mine);
        assert_eq!(asked.cancel(&mut session.requests, purpose), Some(mine));
        assert!(session.requests.get(mine).is_none());
        assert_eq!(asked.cancel(&mut session.requests, purpose), None);
    }
}

//! Connect driver: web proxy carriers (`parity:data-proxy-web`).
//!
//! The reducer keeps the carrier table and queues what has to happen
//! (`crate::proxy_carrier::Carriers`); this side does it: starts and closes
//! hidden web views through the [`CarrierHost`], reports failures to TDLib
//! and hands TDLib's data to the pages. What the pages post goes from the
//! host to TDLib without coming through here; only TDLib's answers to it
//! pass by, and they are dropped. Every carrier request may be sent before
//! authorization.
use super::*;
use crate::proxy_carrier::{CARRIER_RELAY_EXTRA, CarrierError, CarrierHost, CarrierWork};
use crate::state::{RequestPurpose, SettingsPurpose};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{AuthorizationState, EnvelopePayload, SettingsPayload};
use crate::telegram::requests::report_web_proxy_carrier_error;

impl<S: JsonSender> ConnectDriver<S> {
    /// Install the host carriers run in. Without one (tests, CLI tools)
    /// every carrier is refused and web proxies are never enabled.
    pub fn set_carrier_host(&mut self, host: Box<dyn CarrierHost>) {
        self.session.apply_carrier_hosting(host.available());
        self.carrier_host = Some(host);
    }

    /// `updateWebProxyCarrierData` goes straight to the page (bytes are
    /// not state), and TDLib's answers to the relayed page messages are
    /// dropped; everything else comes back for the reducer.
    pub(crate) fn absorb_carrier_data(&mut self, owned: OwnedEnvelope) -> Option<OwnedEnvelope> {
        if owned.envelope.extra == Some(CARRIER_RELAY_EXTRA) {
            // `ok`, or an error for a carrier TDLib closed meanwhile: the
            // page has nothing to learn from either.
            return None;
        }
        let EnvelopePayload::Settings(SettingsPayload::WebProxyCarrierData { carrier_id, data }) =
            owned.envelope.payload
        else {
            return Some(owned);
        };
        // An open that arrived in the same batch has to reach the host
        // before its first data, or the host would not know the carrier.
        self.run_carrier_work();
        if self
            .session
            .settings
            .proxy
            .carriers
            .accepts_data(carrier_id)
            && let Some(host) = self.carrier_host.as_mut()
        {
            host.deliver(carrier_id, data);
        }
        // A host that could not take it reports the failure itself.
        self.carrier_tick();
        None
    }

    /// Per-poll carrier work: whether the host can still run carriers,
    /// what failed (through the reducer), then the queued work. Returns
    /// whether a carrier's state changed.
    pub fn carrier_tick(&mut self) -> bool {
        let Some(host) = self.carrier_host.as_mut() else {
            return self.run_carrier_work();
        };
        let failures = host.poll();
        let hosting = host.available();
        let mut changed = self.session.apply_carrier_hosting(hosting);
        for (carrier_id, error) in failures {
            self.session
                .apply_web_proxy_carrier_failed(carrier_id, error);
            changed = true;
        }
        let worked = self.run_carrier_work();
        changed || worked
    }

    /// Do what the reducer queued. Returns whether anything was done.
    pub(crate) fn run_carrier_work(&mut self) -> bool {
        if matches!(
            self.session.auth,
            AuthorizationState::Closed | AuthorizationState::LoggingOut
        ) {
            let busy = !self.session.settings.proxy.carriers.open.is_empty();
            self.shut_down_carriers();
            return busy;
        }
        let mut did = false;
        // Opening can fail at once, which queues more work: loop until dry.
        loop {
            let work = std::mem::take(&mut self.session.settings.proxy.carriers.work);
            if work.is_empty() {
                return did;
            }
            did = true;
            for item in work {
                self.do_carrier_work(item);
            }
        }
    }

    fn do_carrier_work(&mut self, item: CarrierWork) {
        match item {
            CarrierWork::Open(request) => {
                let opened = match self.carrier_host.as_mut() {
                    Some(host) => host.open(&request),
                    None => Err(CarrierError::Unsupported),
                };
                if let Err(error) = opened {
                    self.session
                        .apply_web_proxy_carrier_failed(request.id, error);
                }
            }
            CarrierWork::Report { carrier_id, error } => {
                let extra = self.session.request(
                    RequestPurpose::Settings(SettingsPurpose::ReportWebProxyCarrier),
                    None,
                );
                let sent = self
                    .sender
                    .send_json(&report_web_proxy_carrier_error(extra, carrier_id, &error));
                if sent.is_err() {
                    self.session.requests.take(extra);
                }
            }
            CarrierWork::Close {
                carrier_id,
                notify_page,
            } => {
                if let Some(host) = self.carrier_host.as_mut() {
                    host.close(carrier_id, notify_page);
                }
            }
        }
    }

    /// Logout, quit or a closed client: every web view goes at once, and
    /// this client hosts no more carriers.
    pub(crate) fn shut_down_carriers(&mut self) {
        self.session.settings.proxy.carriers.shut_down();
        // The driver runs on the UI thread: the shutdown, which waits for
        // helpers to exit and deletes files, goes to a thread of its own,
        // taking the host with it, so it runs once. If no thread can
        // start, dropping the host kills its helpers and deletes its files
        // in the background.
        let Some(mut host) = self.carrier_host.take() else {
            return;
        };
        self.session.apply_carrier_hosting(false);
        let _ =
            crate::thread_guard::spawn_guarded("quill-carrier-teardown", move || host.shutdown());
    }
}

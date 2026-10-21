//! Connect driver: web proxy carriers (`parity:data-proxy-web`).
//!
//! The reducer keeps the carrier table and queues what has to happen
//! (`crate::proxy_carrier::Carriers`); this side does it: starts and closes
//! hidden web views through the [`CarrierHost`], reports failures to TDLib,
//! and relays data both ways once the table says the carrier is live.
//! Every carrier request may be sent before authorization.
use super::*;
use crate::proxy_carrier::{CarrierError, CarrierHost, CarrierWork, HostEvent};
use crate::state::{RequestPurpose, SettingsPurpose};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{AuthorizationState, EnvelopePayload, SettingsPayload};
use crate::telegram::requests::{
    process_web_proxy_carrier_message, report_web_proxy_carrier_error,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Install the host carriers run in. Without one (tests, CLI tools)
    /// every carrier is refused and web proxies are never enabled.
    pub fn set_carrier_host(&mut self, host: Box<dyn CarrierHost>) {
        self.session.settings.proxy.carriers.hosting = host.available();
        self.carrier_host = Some(host);
    }

    /// `updateWebProxyCarrierData` goes straight to the page (bytes are
    /// not state); everything else comes back for the reducer.
    pub(crate) fn absorb_carrier_data(&mut self, owned: OwnedEnvelope) -> Option<OwnedEnvelope> {
        let EnvelopePayload::Settings(SettingsPayload::WebProxyCarrierData { carrier_id, data }) =
            owned.envelope.payload
        else {
            return Some(owned);
        };
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

    /// Per-poll carrier work: what the hosts reported (page messages go to
    /// TDLib, failures go through the reducer), then the queued work.
    /// Returns whether a carrier's state changed.
    pub fn carrier_tick(&mut self) -> bool {
        let events = self
            .carrier_host
            .as_mut()
            .map(|host| host.poll())
            .unwrap_or_default();
        let mut changed = false;
        for (carrier_id, event) in events {
            match event {
                HostEvent::Message(message) => {
                    if !self.session.settings.proxy.carriers.page_posted(carrier_id) {
                        continue;
                    }
                    let extra = self.session.request(
                        RequestPurpose::Settings(SettingsPurpose::ProcessWebProxyCarrierMessage),
                        None,
                    );
                    let sent = self.sender.send_json(&process_web_proxy_carrier_message(
                        extra, carrier_id, &message,
                    ));
                    if sent.is_err() {
                        self.session.requests.take(extra);
                    }
                }
                HostEvent::Failed(error) => {
                    self.session
                        .apply_web_proxy_carrier_failed(carrier_id, error);
                    changed = true;
                }
            }
        }
        changed | self.run_carrier_work()
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
        }
    }

    /// Logout, quit or a closed client: every web view goes at once.
    pub(crate) fn shut_down_carriers(&mut self) {
        self.session.settings.proxy.carriers.shut_down();
        if let Some(host) = self.carrier_host.as_mut() {
            host.shutdown();
        }
    }
}

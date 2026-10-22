//! Web proxy carriers (`parity:data-proxy-web`, TDLib 1.8.69).
//!
//! TDLib carries a web proxy's traffic through an HTTPS relay page that the
//! app loads in a hidden web view (`updateWebProxyCarrierOpened`), passes
//! the page's messages back (`processWebProxyCarrierMessage`), delivers
//! data to it (`updateWebProxyCarrierData`) and closes it
//! (`updateWebProxyCarrierClosed`). Failures go back with
//! `reportWebProxyCarrierError`.
//!
//! This module holds the carrier table the reducer owns ([`Carriers`]: one
//! entry per carrier TDLib opened, its phase, and the work the driver still
//! has to do) and the [`CarrierHost`] seam the driver talks to. The live
//! host ([`helper::HelperCarrierHost`]) runs one `quill-webview --carrier`
//! process per carrier; tests use a fake. Data never sits in the table:
//! TDLib's bytes go from the driver straight to the host, and the page's
//! go from the host straight to TDLib through a [`RelaySink`], tagged with
//! [`CARRIER_RELAY_EXTRA`] so their answers are dropped unread.

pub mod helper;

use crate::ids::RequestId;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The carrier's URL. It holds the proxy's capability, so `Debug` prints a
/// placeholder (the schema says it must never be logged or shown).
pub use quill_webview_protocol::carrier::SecretUrl as CarrierUrl;

/// The `@extra` of every `processWebProxyCarrierMessage`. Those requests
/// go out from the host's reader threads, not through the request table,
/// and their `ok` (or an error for a carrier TDLib already closed) means
/// nothing to anyone: the driver drops answers with this extra before the
/// reducer.
pub const CARRIER_RELAY_EXTRA: RequestId = RequestId(u64::MAX);

/// Sends one JSON request to this host's TDLib client from any thread;
/// `false` once the client is gone.
pub type RelaySink = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// `webProxyCarrier`: what TDLib asked the app to open.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CarrierRequest {
    pub id: i32,
    /// 0 for the temporary carrier of a ping or test.
    pub proxy_id: i32,
    pub url: CarrierUrl,
    pub origin: String,
    pub path_prefix: String,
    pub bridge_object_name: String,
    pub temporary: bool,
}

/// `WebProxyCarrierError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierError {
    Unsupported,
    LoadFailed(String),
    Terminated,
    Refused,
}

impl CarrierError {
    /// The error as TDLib's JSON object.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Unsupported => serde_json::json!({"@type": "webProxyCarrierErrorUnsupported"}),
            Self::LoadFailed(message) => serde_json::json!({
                "@type": "webProxyCarrierErrorLoadFailed",
                "message": message,
            }),
            Self::Terminated => serde_json::json!({"@type": "webProxyCarrierErrorTerminated"}),
            Self::Refused => serde_json::json!({"@type": "webProxyCarrierErrorRefused"}),
        }
    }
}

/// What the page posted to the bridge object (`WebProxyCarrierMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierMessage {
    /// Base64, as TDLib's JSON `bytes`.
    Binary(String),
    Text(String),
}

/// Where carriers run. The driver owns one per TDLib client.
pub trait CarrierHost: Send {
    /// Whether carriers can be hosted here: the helper is installed and
    /// nothing showed this system cannot run one. May change while Quill
    /// runs (a capability check finishing, a carrier finding out).
    fn available(&self) -> bool;
    /// Start a hidden web view for `request`.
    fn open(&mut self, request: &CarrierRequest) -> Result<(), CarrierError>;
    /// Hand base64 `data` to the page of `carrier_id`, in order. A carrier
    /// the host does not know fails instead.
    fn deliver(&mut self, carrier_id: i32, data: String);
    /// Close the carrier; with `notify_page`, a page that posted first
    /// gets `{"t":"close"}`.
    fn close(&mut self, carrier_id: i32, notify_page: bool);
    /// Carriers that failed since the last call.
    fn poll(&mut self) -> Vec<(i32, CarrierError)>;
    /// Whether [`CarrierHost::shutdown`] would wait for something (helpers
    /// still running or exiting). The driver then runs it on a thread of
    /// its own, never on the UI thread.
    fn shutdown_waits(&self) -> bool {
        false
    }
    /// End everything (logout, quit). May block for a bounded time when
    /// [`CarrierHost::shutdown_waits`].
    fn shutdown(&mut self);
}

/// Where a carrier is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierPhase {
    /// Asked of the host, and relaying once the page is up.
    Live,
    /// Reported to TDLib as failed; waiting for TDLib's close.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carrier {
    pub proxy_id: i32,
    pub phase: CarrierPhase,
}

/// What the driver has to do next, queued by the reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierWork {
    Open(CarrierRequest),
    Report {
        carrier_id: i32,
        error: CarrierError,
    },
    Close {
        carrier_id: i32,
        notify_page: bool,
    },
}

/// The reducer's view of every carrier (`ProxyState::carriers`).
#[derive(Debug, Clone, Default)]
pub struct Carriers {
    /// Whether this client has a host that can run carriers. Without one
    /// a web proxy is never enabled (switching to it would cut the
    /// connection) and every carrier is refused.
    pub hosting: bool,
    pub open: BTreeMap<i32, Carrier>,
    pub work: Vec<CarrierWork>,
}

/// What [`Carriers::opened`] decided for the proxy's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenVerdict {
    /// Hosting it: the proxy is trying again.
    Hosting { proxy_id: i32 },
    /// Refused: the proxy cannot connect.
    Refused { proxy_id: i32 },
    /// A ping or test carrier: no row is involved.
    Temporary,
}

impl Carriers {
    /// `updateWebProxyCarrierOpened`. Temporary carriers (a ping or a
    /// test) are refused: tdesktop never tests web proxies and Quill hides
    /// their check button, so hosting them would only spin up a web view
    /// for nothing.
    pub fn opened(&mut self, request: CarrierRequest) -> OpenVerdict {
        let carrier_id = request.id;
        let proxy_id = request.proxy_id;
        if request.temporary || proxy_id == 0 {
            self.work.push(CarrierWork::Report {
                carrier_id,
                error: CarrierError::Refused,
            });
            return OpenVerdict::Temporary;
        }
        // TDLib never reuses a live id; a repeat changes nothing.
        if self.open.contains_key(&carrier_id) {
            return OpenVerdict::Hosting { proxy_id };
        }
        if !self.hosting {
            self.work.push(CarrierWork::Report {
                carrier_id,
                error: CarrierError::Unsupported,
            });
            return OpenVerdict::Refused { proxy_id };
        }
        self.open.insert(
            carrier_id,
            Carrier {
                proxy_id,
                phase: CarrierPhase::Live,
            },
        );
        self.work.push(CarrierWork::Open(request));
        OpenVerdict::Hosting { proxy_id }
    }

    /// `updateWebProxyCarrierClosed`: forget it and tear its view down. The
    /// helper knows whether its page ever posted, so the page is told
    /// whenever it might be listening.
    pub fn closed(&mut self, carrier_id: i32) {
        // A failed carrier's view went when it failed.
        if let Some(carrier) = self.open.remove(&carrier_id)
            && carrier.phase != CarrierPhase::Failed
        {
            self.work.push(CarrierWork::Close {
                carrier_id,
                notify_page: true,
            });
        }
    }

    /// The carrier failed (the host said so, or could not start it). Its
    /// proxy id when this is news; a carrier fails once.
    pub fn failed(&mut self, carrier_id: i32, error: CarrierError) -> Option<i32> {
        let carrier = self.open.get_mut(&carrier_id)?;
        if carrier.phase == CarrierPhase::Failed {
            return None;
        }
        carrier.phase = CarrierPhase::Failed;
        self.work.push(CarrierWork::Report { carrier_id, error });
        self.work.push(CarrierWork::Close {
            carrier_id,
            notify_page: false,
        });
        Some(carrier.proxy_id)
    }

    /// Whether TDLib's data for `carrier_id` goes to the page.
    pub fn accepts_data(&self, carrier_id: i32) -> bool {
        self.open
            .get(&carrier_id)
            .is_some_and(|carrier| carrier.phase != CarrierPhase::Failed)
    }

    /// Logout or quit: every carrier goes, and TDLib hears nothing more.
    pub fn shut_down(&mut self) {
        self.open.clear();
        self.work.clear();
    }
}

#[cfg(test)]
mod helper_live_tests;
#[cfg(test)]
mod tests;

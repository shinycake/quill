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
//! process per carrier; tests use a fake. Data never sits in the table: the
//! driver hands it straight to the host or to TDLib once the table says the
//! carrier is live.

pub mod helper;

use std::collections::BTreeMap;

/// The carrier's URL. It holds the proxy's capability, so the schema says
/// it must never be logged or shown: `Debug` prints a placeholder.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct CarrierUrl(String);

impl CarrierUrl {
    pub fn new(url: String) -> Self {
        Self(url)
    }

    /// The URL itself, for the host only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for CarrierUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CarrierUrl(<hidden>)")
    }
}

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

/// What a host reports about one of its carriers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    Message(CarrierMessage),
    Failed(CarrierError),
}

/// Where carriers run. The driver owns one per TDLib client.
pub trait CarrierHost: Send {
    /// Whether carriers can be hosted at all (the helper is installed).
    fn available(&self) -> bool;
    /// Start a hidden web view for `request`.
    fn open(&mut self, request: &CarrierRequest) -> Result<(), CarrierError>;
    /// Hand base64 `data` to the page of `carrier_id`, in order.
    fn deliver(&mut self, carrier_id: i32, data: String);
    /// Close the carrier; `notify_page` first sends the page
    /// `{"t":"close"}`.
    fn close(&mut self, carrier_id: i32, notify_page: bool);
    /// Events since the last call.
    fn poll(&mut self) -> Vec<(i32, HostEvent)>;
    /// Close everything at once (logout, quit).
    fn shutdown(&mut self);
}

/// Where a carrier is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierPhase {
    /// Asked of the host; the page has said nothing yet.
    Opening,
    /// The page has posted at least once (it gets `{"t":"close"}` when
    /// TDLib closes it).
    Relaying,
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
                phase: CarrierPhase::Opening,
            },
        );
        self.work.push(CarrierWork::Open(request));
        OpenVerdict::Hosting { proxy_id }
    }

    /// `updateWebProxyCarrierClosed`: forget it and tear its view down.
    pub fn closed(&mut self, carrier_id: i32) {
        // A failed carrier's view went when it failed.
        if let Some(carrier) = self.open.remove(&carrier_id)
            && carrier.phase != CarrierPhase::Failed
        {
            self.work.push(CarrierWork::Close {
                carrier_id,
                notify_page: carrier.phase == CarrierPhase::Relaying,
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

    /// The page posted a message: whether it goes to TDLib. Messages of a
    /// carrier TDLib closed, or that failed, are dropped (the schema's
    /// "must be ignored").
    pub fn page_posted(&mut self, carrier_id: i32) -> bool {
        match self.open.get_mut(&carrier_id) {
            Some(carrier) if carrier.phase != CarrierPhase::Failed => {
                carrier.phase = CarrierPhase::Relaying;
                true
            }
            _ => false,
        }
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
mod tests;

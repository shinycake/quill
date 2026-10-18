//! Allowlisted diagnostics. Never record raw TDLib JSON, credentials, or message text.
#![deny(clippy::indexing_slicing)]

use crate::locks::LockExt;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub category: &'static str,
    pub type_name: Option<String>,
    pub extra: Option<u64>,
    pub seq: Option<u64>,
    pub note: &'static str,
}

impl Diagnostic {
    pub fn format_line(&self) -> String {
        format!(
            "cat={category} type={ty} extra={extra} seq={seq} note={note}",
            category = self.category,
            ty = self.type_name.as_deref().unwrap_or("-"),
            extra = self
                .extra
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            seq = self
                .seq
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            note = self.note
        )
    }
}

pub trait DiagnosticSink: Send + Sync {
    fn record(&self, event: Diagnostic);
}

/// Drops every diagnostic. The live client receives every TDLib update,
/// so a sink that keeps them grows for as long as the app runs.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiscardSink;

impl DiagnosticSink for DiscardSink {
    fn record(&self, _event: Diagnostic) {}
}

/// Keeps every diagnostic, for tests and short-lived probes.
#[derive(Debug, Default, Clone)]
pub struct MemorySink {
    inner: Arc<Mutex<Vec<Diagnostic>>>,
}

impl MemorySink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<Diagnostic> {
        self.inner.lock_or_recover().clone()
    }

    pub fn rendered(&self) -> String {
        self.snapshot()
            .iter()
            .map(Diagnostic::format_line)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl DiagnosticSink for MemorySink {
    fn record(&self, event: Diagnostic) {
        self.inner.lock_or_recover().push(event);
    }
}

/// Native TDLib log callback must never forward the message body.
/// Verbosity 0 is fatal inside TDLib after the callback returns; we only count it.
pub fn native_log_note(verbosity: i32) -> &'static str {
    if verbosity <= 0 {
        "native-log-fatal-level"
    } else {
        "native-log-redacted"
    }
}

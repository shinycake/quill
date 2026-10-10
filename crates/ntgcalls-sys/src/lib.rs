//! Raw FFI bindings for ntgcalls v3.0.2 — Quill's vendored call media engine.
//!
//! Quill's call UI keeps its honest "no audio yet" stance until Phase C2b
//! wires this engine up; this crate is the un-wired seam.
//!
//! **License (LGPLv3 sidecar).** The native library `libntgcalls.so`
//! (pytgcalls/ntgcalls v3.0.2, LGPL-3.0) is **never linked into the Quill
//! binary** — it is procured unmodified by `scripts/vendor-ntgcalls.sh`
//! (checksum-pinned, git-ignored) and loaded at runtime via `dlopen` by
//! [`Loader`]. See `THIRD_PARTY.md` for attribution and the LGPL source
//! offer.
//!
//! Every declaration below was verified verbatim against the vendored
//! `include/ntgcalls.h` (generated from `schema/ntgcalls.ntl`), not against
//! any third-party binding crate. There is no `ntg_destroy` in the header:
//! instance lifecycle is `ntg_instance_create` / `ntg_instance_destroy`.
//!
//! No safe wrappers live here (C2b builds them), and no test does any
//! network or audio I/O.

#![allow(non_camel_case_types)]

use std::ffi::{OsStr, c_char, c_int, c_void};
use std::fmt;
use std::path::PathBuf;

mod ffi;
mod loader;
pub use ffi::*;
pub use loader::*;

/// Native sidecar name, using Rust's platform library naming convention.
pub fn library_filename() -> String {
    format!(
        "{}ntgcalls{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

// ---------------------------------------------------------------------------
// dlopen loader (runtime only — no link-time dependency, LGPL sidecar rule)
// ---------------------------------------------------------------------------

/// Diagnostic failure from [`Loader::load`] / [`Loader::load_default`].
///
/// The call UI keeps its honest "no audio yet" stance until C2b wires the
/// engine; these errors are what C2b surfaces in that UI when the sidecar
/// library is absent or too old.
#[derive(Debug)]
pub enum LoadError {
    /// The library itself could not be opened. `searched` lists every
    /// location that was tried, in order.
    LibraryMissing { searched: Vec<PathBuf> },
    /// The library opened, but a bound `ntg_*` symbol was not exported by it.
    SymbolMissing { symbol: &'static str },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::LibraryMissing { searched } => {
                write!(f, "native call media library not found; searched: ")?;
                let mut first = true;
                for path in searched {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{}", path.display())?;
                }
                write!(
                    f,
                    " (run scripts/vendor-ntgcalls.sh or set QUILL_NTGCALLS_LIB)"
                )
            }
            LoadError::SymbolMissing { symbol } => write!(
                f,
                "native call media library does not export symbol `{symbol}`; \
                 the vendored library may be older than these bindings expect"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

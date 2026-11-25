//! Snapshot transforms (docs/decisions/snapshot-transitions.md): the tests
//! of the vendored GPUI's snapshot model, run against the real element
//! tree.

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;

//! The UI driver: agents read Quill's window as an element tree and click,
//! type and scroll in it (`docs/decisions/agent-access.md`, phase 5).
//!
//! Nothing here touches GPUI. The tool threads file a [`bridge::UiCommand`]
//! and wait; the UI thread drains the queue, reads the tree GPUI built and
//! injects the input (`src/ui/agent_driver.rs`). Everything is off until the
//! user turns on "Let agents drive Quill's window" (`super::access::ui_driver`).

pub mod bridge;
pub mod keys;
pub mod policy;
pub mod tree;

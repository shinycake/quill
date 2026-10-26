//! The UI driver: agents read Quill's window as an element tree and click,
//! type and scroll in it (`docs/decisions/agent-access.md`, phase 5).
//!
//! Nothing here touches GPUI. The tool threads file a [`bridge::UiCommand`]
//! and wait; the UI thread drains the queue, reads the tree GPUI built and
//! injects the input (`src/ui/agent_driver.rs`). Everything is off until the
//! user turns on "Let agents drive Quill's window" (`super::access::ui_driver`).
//!
//! The driver follows the chat grants like every other tool: the UI decides
//! the [`surface::Surface`] from its own state, cuts the tree to what the
//! grants allow, and while it injects, the [`guard`] refuses every write.

pub mod bridge;
pub mod guard;
pub mod keys;
pub mod policy;
pub mod redact;
pub mod shot;
pub mod surface;
pub mod takeover;
pub mod tree;

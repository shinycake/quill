//! Agent access: local AI agents (Claude Code, Claude Desktop, Cursor and
//! any other MCP client) use the user's account through the running Quill.
//!
//! Off by default (Settings > Advanced > Agent access). Agents reach Quill
//! over its existing local socket ([`crate::single_instance`]) with the
//! `quill-agent 1` header and a per-launch token ([`access`]); `quill mcp`
//! ([`mcp`]) is the stdio MCP server that bridges to it, and `quill agent`
//! ([`cli`]) the command line for scripts. Requests reach
//! TDLib with an `@extra` range of their own ([`td`]). There is no network
//! listener. Design and the Telegram terms it answers:
//! `docs/decisions/agent-access.md`.
#![deny(clippy::indexing_slicing)]

/// Where Agent access's page is in Quill's settings, for messages about its
/// other switches. The page exists only while access is on. A macro, so
/// `concat!` can build constant messages from it.
macro_rules! settings_place {
    () => {
        "Settings > Agent access"
    };
}

/// Where the user turns Agent access on: while it is off, its one switch
/// is on the Advanced page (`docs/decisions/agent-access.md`, "Master
/// switch"). Every "turn it on" message names this place.
macro_rules! switch_place {
    () => {
        "Settings > Advanced > Agent access"
    };
}

/// [`switch_place!`] as a constant, for `format!`.
pub const SWITCH_PLACE: &str = switch_place!();

pub mod access;
pub mod activity;
pub mod cli;
pub mod events;
pub mod grants;
pub mod guide;
pub mod link;
pub mod mcp;
pub mod pacing;
pub mod prompts;
pub mod server;
pub mod summary;
pub mod td;
pub mod tools;
pub mod ui_driver;

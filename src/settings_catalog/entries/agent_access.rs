//! Agent access page entries, one or more per section of the page. The
//! page scrolls to an entry's section when search opens it
//! (`ui/settings_window/agent_page.rs`).

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::AgentAccess;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("agent_access.allow", P, keys::Q_AGENT_ACCESS_ALLOW)
        .section(keys::Q_AGENT_PAGE_SEC_CONNECTION)
        .keywords(&["mcp", "ai", "claude", "automation", "agents"]),
    Entry::new("agent_access.reset_token", P, keys::Q_AGENT_PAGE_RESET)
        .section(keys::Q_AGENT_PAGE_SEC_CONNECTION)
        .keywords(&["token", "disconnect", "revoke"]),
    Entry::new("agent_access.connected", P, keys::Q_AGENT_CONNECTED_TITLE)
        .section(keys::Q_AGENT_PAGE_SEC_CONNECTION)
        .keywords(&["connected agents", "revoke", "disconnect"]),
    Entry::new("agent_access.chats", P, keys::Q_AGENT_GRANTS_TITLE)
        .section(keys::Q_AGENT_PAGE_SEC_CHATS)
        .keywords(&[
            "grants",
            "permissions",
            "saved messages",
            "bots",
            "channels",
        ]),
    Entry::new("agent_access.all_chats", P, keys::Q_AGENT_GRANTS_ALL_CHATS)
        .section(keys::Q_AGENT_PAGE_SEC_CHATS)
        .keywords(&["every chat", "secret chats"]),
    Entry::new("agent_access.contacts", P, keys::Q_AGENT_CONTACTS_TITLE)
        .section(keys::Q_AGENT_PAGE_SEC_CHATS)
        .keywords(&["contact list"]),
    Entry::new("agent_access.add_chat", P, keys::Q_AGENT_PAGE_ADD_CHAT)
        .section(keys::Q_AGENT_PAGE_SEC_CHATS)
        .keywords(&["grant a chat", "allow chat"]),
    Entry::new("agent_access.tools", P, keys::Q_AGENT_PAGE_SEC_TOOLS).keywords(&[
        "tools",
        "mcp tools",
        "capabilities",
    ]),
    Entry::new("agent_access.raw_calls", P, keys::Q_AGENT_PAGE_RAW_SWITCH)
        .section(keys::Q_AGENT_PAGE_SEC_TOOLS)
        .keywords(&["td_call", "tdlib", "raw requests"]),
    Entry::new("agent_access.always_asks", P, keys::Q_AGENT_PAGE_SEC_ASKS).keywords(&[
        "money",
        "stars",
        "gifts",
        "account changes",
        "never allowed",
    ]),
    Entry::new("agent_access.window", P, keys::Q_AGENT_UI_DRIVER)
        .section(keys::Q_AGENT_PAGE_SEC_WINDOW)
        .keywords(&["ui driver", "click", "screenshot", "takeover"]),
    Entry::new(
        "agent_access.takeover_effect",
        P,
        keys::Q_AGENT_TAKEOVER_EFFECT,
    )
    .section(keys::Q_AGENT_PAGE_SEC_WINDOW)
    .keywords(&["glow", "sweep"]),
    Entry::new("agent_access.confirm_sends", P, keys::Q_AGENT_CONFIRM_SENDS)
        .section(keys::Q_AGENT_PAGE_SEC_CONFIRM)
        .keywords(&["ask before sending", "approve"]),
    Entry::new("agent_access.activity", P, keys::Q_AGENT_ACTIVITY_TITLE)
        .keywords(&["log", "history", "undo"]),
    Entry::new("agent_access.connect", P, keys::Q_AGENT_ACCESS_CONNECT).keywords(&[
        "claude code",
        "claude desktop",
        "cursor",
        "setup",
    ]),
];

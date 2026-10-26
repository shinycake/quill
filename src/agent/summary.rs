//! What agents can reach right now, worked out once from live state.
//!
//! The Agent access page, `quill_status` and `quill://grants` all render
//! from [`AccessSummary`], so they can never disagree. [`summarize`] is a
//! pure function of [`Inputs`]; [`live`] gathers the inputs from Quill's
//! switches, the grants, the connected agents, the takeover and the tool
//! registry.

use super::activity::{Action, Connected};
use super::grants::{AccountGrants, Scope, is_secret_chat_id};
use super::tools::{ToolGroup, ToolInfo};
use crate::ids::AccountKey;
use serde_json::{Value, json};

/// What the window driver is doing, as the summary needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Takeover {
    Idle,
    Driving { agent: String },
    Paused,
    Stopped,
}

/// Everything the summary is computed from.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub enabled: bool,
    pub confirm_sends: bool,
    pub ui_driver: bool,
    pub raw_calls: bool,
    pub takeover: Takeover,
    /// Whether this build can take screenshots.
    pub screenshots_built: bool,
    pub grants: AccountGrants,
    /// Connected agents, revoked connections already left out.
    pub connected: Vec<Connected>,
    /// Agent names the user disconnected.
    pub refused: Vec<String>,
    pub tools: Vec<ToolInfo>,
    /// This account's activity log, newest first.
    pub log: Vec<Action>,
}

/// One chat the user added by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedChat {
    pub chat_id: i64,
    pub title: String,
    /// `None`: blocked even when a default or All chats covers it.
    pub scope: Option<Scope>,
    pub secret: bool,
    /// When an agent last acted in it (Unix seconds).
    pub last_used: Option<i64>,
}

/// The chat grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chats {
    pub saved: Option<Scope>,
    pub bots: Option<Scope>,
    pub own_channels: Option<Scope>,
    pub all_chats: Option<Scope>,
    pub contacts: bool,
    pub added: Vec<AddedChat>,
}

impl Chats {
    fn scopes(&self) -> impl Iterator<Item = Option<Scope>> + '_ {
        [self.saved, self.bots, self.own_channels, self.all_chats]
            .into_iter()
            .chain(self.added.iter().map(|chat| chat.scope))
    }

    /// Whether any chat at all allows writing.
    pub fn writable_anywhere(&self) -> bool {
        self.scopes().any(|scope| scope == Some(Scope::ReadWrite))
    }

    /// Whether any chat at all is readable.
    pub fn readable_anywhere(&self) -> bool {
        self.scopes().any(|scope| scope.is_some())
    }
}

/// What part of the window an agent sees when it drives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowView {
    /// It sees nothing: window control or Agent access is off.
    Nothing,
    /// Only granted chats; the rest of the window is blanked.
    GrantedChats,
    /// The whole window, because All chats is on.
    WholeWindow,
}

/// The window driver's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowState {
    Off,
    Ready,
    Driving { agent: String },
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// "Let agents drive Quill's window".
    pub switch: bool,
    pub view: WindowView,
    pub state: WindowState,
    pub screenshots: bool,
}

/// One connected agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub id: u64,
    pub name: String,
    pub since: i64,
    pub tools: Vec<String>,
}

/// When a tool asks the user first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asks {
    /// Confirm each send is on.
    EachSend,
    /// Every call asks (`request_access`).
    Always,
    /// Account changes, money and calls that reach beyond one chat ask;
    /// the rest run under the grants (`td_call`).
    RiskyCalls,
}

/// Why a tool can't run now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffReason {
    AccessOff,
    ContactsOff,
    NothingWritable,
    RawCallsOff,
    WindowControlOff,
    Paused,
    Stopped,
    NoCapture,
}

/// What the user can press to lift an [`OffReason`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    TurnOnAccess,
    TurnOnContacts,
    GrantWriting,
    TurnOnRawCalls,
    TurnOnWindowControl,
    LetContinue,
    RestartWindowControl,
}

impl OffReason {
    /// The button that lifts it; `None` when no button can (a build
    /// without screenshots).
    pub fn fix(self) -> Option<Fix> {
        match self {
            OffReason::AccessOff => Some(Fix::TurnOnAccess),
            OffReason::ContactsOff => Some(Fix::TurnOnContacts),
            OffReason::NothingWritable => Some(Fix::GrantWriting),
            OffReason::RawCallsOff => Some(Fix::TurnOnRawCalls),
            OffReason::WindowControlOff => Some(Fix::TurnOnWindowControl),
            OffReason::Paused => Some(Fix::LetContinue),
            OffReason::Stopped => Some(Fix::RestartWindowControl),
            OffReason::NoCapture => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            OffReason::AccessOff => "agent access is off",
            OffReason::ContactsOff => "the contact list is not granted",
            OffReason::NothingWritable => "no chat is granted read_write",
            OffReason::RawCallsOff => "the user turned raw TDLib calls off",
            OffReason::WindowControlOff => "the user has not let agents drive Quill's window",
            OffReason::Paused => "the user took control of the window",
            OffReason::Stopped => "the user pressed Stop",
            OffReason::NoCapture => "this build cannot take screenshots",
        }
    }
}

/// A tool's live state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Available,
    AsksFirst(Asks),
    Off(OffReason),
}

impl ToolState {
    fn as_str(self) -> &'static str {
        match self {
            ToolState::Available => "available",
            ToolState::AsksFirst(_) => "asks_first",
            ToolState::Off(_) => "off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub group: ToolGroup,
    pub state: ToolState,
}

/// Something Quill always asks the user about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskKind {
    /// Reaching a chat or the contacts that aren't granted.
    NewAccess,
    /// Raw calls that change more than one chat.
    BeyondOneChat,
    /// Profile, password, phone number, sessions, signing out.
    AccountAndSessions,
    /// Anything that spends money, Stars or buys gifts.
    MoneyStarsGifts,
    /// Every send, edit, delete, reaction and pin.
    EachSend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlwaysAsk {
    pub kind: AskKind,
    /// False when agents can't even ask (raw calls off, Agent access off).
    pub reachable: bool,
}

/// What no agent can do, whatever the user grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Never {
    /// Hide reading, online status or typing.
    HiddenStatus,
    /// Export a whole account or chat in one go.
    BulkExport,
    /// Open Settings or sign out.
    SettingsAndLogout,
    /// Pay for messages in chats that charge Stars.
    PaidMessages,
}

pub const NEVER: [Never; 4] = [
    Never::HiddenStatus,
    Never::BulkExport,
    Never::SettingsAndLogout,
    Never::PaidMessages,
];

/// The page's sections, in order; every claim links to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Connection,
    Chats,
    Tools,
    AlwaysAsks,
    Window,
    Confirmations,
    Activity,
    Connect,
}

impl Section {
    pub const ALL: [Section; 8] = [
        Section::Connection,
        Section::Chats,
        Section::Tools,
        Section::AlwaysAsks,
        Section::Window,
        Section::Confirmations,
        Section::Activity,
        Section::Connect,
    ];
}

/// A group of chats a claim names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    SavedMessages,
    Bots,
    OwnChannels,
    AllOtherChats,
    /// This many chats added by name.
    Added(usize),
}

/// One sentence of the summary card, from the user's side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    AccessOff,
    ReadWrite(Vec<Place>),
    ReadOnly(Vec<Place>),
    NoChats,
    /// Chats not granted stay hidden (All chats is off).
    OtherChatsHidden,
    /// Secret chats stay hidden unless added by name (All chats is on).
    SecretChatsHidden,
    Contacts(bool),
    RawCalls(bool),
    Window(WindowState),
    EachSend(bool),
    AlwaysAsks,
    Agents(Vec<String>),
}

impl Claim {
    pub fn section(&self) -> Section {
        match self {
            Claim::AccessOff | Claim::Agents(_) => Section::Connection,
            Claim::ReadWrite(_)
            | Claim::ReadOnly(_)
            | Claim::NoChats
            | Claim::OtherChatsHidden
            | Claim::SecretChatsHidden
            | Claim::Contacts(_) => Section::Chats,
            Claim::RawCalls(_) => Section::Tools,
            Claim::Window(_) => Section::Window,
            Claim::EachSend(_) => Section::Confirmations,
            Claim::AlwaysAsks => Section::AlwaysAsks,
        }
    }
}

/// The one picture of agent access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessSummary {
    pub enabled: bool,
    pub chats: Chats,
    pub window: Window,
    pub confirm_each_send: bool,
    pub raw_calls: bool,
    pub agents: Vec<Agent>,
    pub refused: Vec<String>,
    pub tools: Vec<Tool>,
    pub always_asks: Vec<AlwaysAsk>,
    pub claims: Vec<Claim>,
}

/// Tools that "Confirm each send" puts behind a click.
const CONFIRMED: &[&str] = &[
    "send_message",
    "send_file",
    "send_poll",
    "send_sticker",
    "forward_messages",
    "edit_message",
    "delete_messages",
    "react",
    "pin_message",
    "type_text",
];

fn tool_state(tool: &ToolInfo, inputs: &Inputs, chats: &Chats, window: &Window) -> ToolState {
    if !inputs.enabled {
        return ToolState::Off(OffReason::AccessOff);
    }
    let each_send = inputs.confirm_sends && CONFIRMED.contains(&tool.name);
    match tool.group {
        ToolGroup::Read => match tool.name {
            "list_contacts" if !chats.contacts => ToolState::Off(OffReason::ContactsOff),
            "request_access" => ToolState::AsksFirst(Asks::Always),
            _ => ToolState::Available,
        },
        ToolGroup::Write if !chats.writable_anywhere() => {
            ToolState::Off(OffReason::NothingWritable)
        }
        ToolGroup::Write if each_send => ToolState::AsksFirst(Asks::EachSend),
        ToolGroup::Write => ToolState::Available,
        ToolGroup::Raw if !inputs.raw_calls => ToolState::Off(OffReason::RawCallsOff),
        ToolGroup::Raw => ToolState::AsksFirst(Asks::RiskyCalls),
        ToolGroup::Window => match &window.state {
            WindowState::Off => ToolState::Off(OffReason::WindowControlOff),
            WindowState::Stopped => ToolState::Off(OffReason::Stopped),
            WindowState::Paused => ToolState::Off(OffReason::Paused),
            _ if tool.name == "take_screenshot" && !window.screenshots => {
                ToolState::Off(OffReason::NoCapture)
            }
            _ if each_send => ToolState::AsksFirst(Asks::EachSend),
            _ => ToolState::Available,
        },
    }
}

fn places(chats: &Chats, scope: Scope) -> Vec<Place> {
    let mut places: Vec<Place> = [
        (chats.saved, Place::SavedMessages),
        (chats.bots, Place::Bots),
        (chats.own_channels, Place::OwnChannels),
        (chats.all_chats, Place::AllOtherChats),
    ]
    .into_iter()
    .filter(|(granted, _)| *granted == Some(scope))
    .map(|(_, place)| place)
    .collect();
    let added = chats
        .added
        .iter()
        .filter(|chat| chat.scope == Some(scope))
        .count();
    if added > 0 {
        places.push(Place::Added(added));
    }
    places
}

fn claims(summary: &AccessSummary) -> Vec<Claim> {
    if !summary.enabled {
        return vec![Claim::AccessOff];
    }
    let chats = &summary.chats;
    let mut claims = Vec::new();
    let read_write = places(chats, Scope::ReadWrite);
    let read = places(chats, Scope::Read);
    if read_write.is_empty() && read.is_empty() {
        claims.push(Claim::NoChats);
    }
    if !read_write.is_empty() {
        claims.push(Claim::ReadWrite(read_write));
    }
    if !read.is_empty() {
        claims.push(Claim::ReadOnly(read));
    }
    claims.push(if chats.all_chats.is_some() {
        Claim::SecretChatsHidden
    } else {
        Claim::OtherChatsHidden
    });
    claims.push(Claim::Contacts(chats.contacts));
    claims.push(Claim::RawCalls(summary.raw_calls));
    claims.push(Claim::Window(summary.window.state.clone()));
    claims.push(Claim::EachSend(summary.confirm_each_send));
    claims.push(Claim::AlwaysAsks);
    claims.push(Claim::Agents(
        summary.agents.iter().map(|a| a.name.clone()).collect(),
    ));
    claims
}

/// Works out the summary from `inputs`.
pub fn summarize(inputs: &Inputs) -> AccessSummary {
    let grants = &inputs.grants;
    let last_used = |chat_id: i64| {
        inputs
            .log
            .iter()
            .find(|action| action.chat_id == chat_id)
            .map(|action| action.time)
    };
    let chats = Chats {
        saved: grants.saved,
        bots: grants.bots,
        own_channels: grants.own_channels,
        all_chats: grants.all_chats,
        contacts: grants.contacts,
        added: grants
            .chats
            .iter()
            .map(|(id, grant)| AddedChat {
                chat_id: *id,
                title: grant.title.clone(),
                scope: grant.scope,
                secret: is_secret_chat_id(*id),
                last_used: last_used(*id),
            })
            .collect(),
    };
    let on = inputs.enabled && inputs.ui_driver;
    let window = Window {
        switch: inputs.ui_driver,
        view: match (on, grants.all_chats.is_some()) {
            (false, _) => WindowView::Nothing,
            (true, false) => WindowView::GrantedChats,
            (true, true) => WindowView::WholeWindow,
        },
        state: match (&inputs.takeover, on) {
            (_, false) => WindowState::Off,
            (Takeover::Stopped, true) => WindowState::Stopped,
            (Takeover::Paused, true) => WindowState::Paused,
            (Takeover::Driving { agent }, true) => WindowState::Driving {
                agent: agent.clone(),
            },
            (Takeover::Idle, true) => WindowState::Ready,
        },
        screenshots: inputs.screenshots_built,
    };
    let tools = inputs
        .tools
        .iter()
        .map(|tool| Tool {
            name: tool.name,
            title: tool.title,
            group: tool.group,
            state: tool_state(tool, inputs, &chats, &window),
        })
        .collect();
    let raw = inputs.enabled && inputs.raw_calls;
    let mut always_asks = vec![
        AlwaysAsk {
            kind: AskKind::NewAccess,
            reachable: inputs.enabled,
        },
        AlwaysAsk {
            kind: AskKind::BeyondOneChat,
            reachable: raw,
        },
        AlwaysAsk {
            kind: AskKind::AccountAndSessions,
            reachable: raw,
        },
        AlwaysAsk {
            kind: AskKind::MoneyStarsGifts,
            reachable: raw,
        },
    ];
    if inputs.confirm_sends {
        always_asks.push(AlwaysAsk {
            kind: AskKind::EachSend,
            reachable: inputs.enabled && chats.writable_anywhere(),
        });
    }
    let mut summary = AccessSummary {
        enabled: inputs.enabled,
        chats,
        window,
        confirm_each_send: inputs.confirm_sends,
        raw_calls: inputs.raw_calls,
        agents: inputs
            .connected
            .iter()
            .map(|c| Agent {
                id: c.id,
                name: c.agent.clone(),
                since: c.since,
                tools: c.tools.clone(),
            })
            .collect(),
        refused: inputs.refused.clone(),
        tools,
        always_asks,
        claims: Vec::new(),
    };
    summary.claims = claims(&summary);
    summary
}

/// The summary for `account` as Quill stands now.
pub fn live(account: &AccountKey) -> AccessSummary {
    summarize(&live_inputs(account))
}

/// The inputs [`live`] reads, for a caller that adjusts them first.
pub fn live_inputs(account: &AccountKey) -> Inputs {
    use super::ui_driver::takeover;
    let takeover = if takeover::stopped() {
        Takeover::Stopped
    } else if takeover::paused() {
        Takeover::Paused
    } else {
        match takeover::current() {
            takeover::Phase::Driving { agent, .. } => Takeover::Driving { agent },
            _ => Takeover::Idle,
        }
    };
    Inputs {
        enabled: super::access::enabled(),
        confirm_sends: super::access::confirm_sends(),
        ui_driver: super::access::ui_driver(),
        raw_calls: super::access::raw_calls(),
        takeover,
        screenshots_built: super::ui_driver::shot::CAPTURE_BUILT,
        grants: super::grants::for_account(account),
        connected: super::activity::connected()
            .into_iter()
            .filter(|c| !super::activity::is_revoked(c.id))
            .collect(),
        refused: super::activity::refused_agents(),
        tools: super::tools::registry(),
        log: super::activity::recent(account),
    }
}

fn word(scope: Option<Scope>) -> &'static str {
    scope.map_or("off", Scope::as_str)
}

impl WindowState {
    fn as_str(&self) -> &'static str {
        match self {
            WindowState::Off => "off",
            WindowState::Ready => "ready",
            WindowState::Driving { .. } => "driving",
            WindowState::Paused => "paused",
            WindowState::Stopped => "stopped",
        }
    }
}

impl AccessSummary {
    /// The summary as agents read it (`quill_status`, `quill://grants`).
    pub fn describe(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                let mut entry = json!({
                    "name": tool.name,
                    "group": tool.group.as_str(),
                    "state": tool.state.as_str(),
                });
                if let ToolState::Off(reason) = tool.state {
                    entry["reason"] = Value::from(reason.as_str());
                }
                if let ToolState::AsksFirst(asks) = tool.state {
                    entry["asks"] = Value::from(match asks {
                        Asks::EachSend => "each call (Confirm each send is on)",
                        Asks::Always => "each call",
                        Asks::RiskyCalls => "account changes, money, and changes beyond one chat",
                    });
                }
                entry
            })
            .collect();
        let chats: Vec<Value> = self
            .chats
            .added
            .iter()
            .map(|chat| {
                json!({
                    "chat_id": chat.chat_id,
                    "title": chat.title,
                    "scope": word(chat.scope),
                    "secret": chat.secret,
                })
            })
            .collect();
        json!({
            "enabled": self.enabled,
            "chats": {
                "saved_messages": word(self.chats.saved),
                "bots": word(self.chats.bots),
                "own_channels": word(self.chats.own_channels),
                "all_chats": word(self.chats.all_chats),
                "secret_chats": "only those listed in chats",
                "contacts": self.chats.contacts,
                "chats": chats,
            },
            "confirm_each_send": self.confirm_each_send,
            "raw_tdlib_calls": self.raw_calls,
            "window_control": {
                "switch": self.window.switch,
                "state": self.window.state.as_str(),
                "sees": match self.window.view {
                    WindowView::Nothing => "nothing",
                    WindowView::GrantedChats => "granted chats only",
                    WindowView::WholeWindow => "the whole window (All chats is on)",
                },
                "screenshots": self.window.screenshots,
            },
            "tools": tools,
            "never": [
                "hide reading, online status or typing",
                "bulk export",
                "open Settings or sign out",
                "pay for messages",
            ],
        })
    }

    /// One line per fact, for text answers.
    pub fn text_lines(&self) -> Vec<String> {
        if !self.enabled {
            return vec!["Agent access is off.".into()];
        }
        let mut lines = vec![
            format!("Saved Messages: {}", word(self.chats.saved)),
            format!("Bots: {}", word(self.chats.bots)),
            format!("Channels you own: {}", word(self.chats.own_channels)),
            format!("All other chats: {}", word(self.chats.all_chats)),
            format!(
                "Contacts: {}",
                if self.chats.contacts { "read" } else { "off" }
            ),
        ];
        for chat in &self.chats.added {
            lines.push(format!(
                "\"{}\" [{}]: {}",
                chat.title,
                chat.chat_id,
                word(chat.scope)
            ));
        }
        lines.push(format!(
            "Confirm each send: {}",
            if self.confirm_each_send { "on" } else { "off" }
        ));
        lines.push(format!(
            "Raw TDLib calls (td_call): {}",
            if self.raw_calls { "on" } else { "off" }
        ));
        let off: Vec<String> = self
            .tools
            .iter()
            .filter_map(|tool| match tool.state {
                ToolState::Off(reason) => Some(format!("{} ({})", tool.name, reason.as_str())),
                _ => None,
            })
            .collect();
        if !off.is_empty() {
            lines.push(format!("Off now: {}", off.join(", ")));
        }
        lines
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;

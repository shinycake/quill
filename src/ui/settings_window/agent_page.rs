//! Settings > Agent access: what agents can reach, and every switch that
//! changes it (`docs/decisions/agent-access.md`, phase 6). Each render reads
//! one [`AccessSummary`] (`quill::agent::summary`), the same model
//! `quill_status` and `quill://grants` give agents, so the page and the
//! agents never disagree. The sections live in `agent_access.rs`
//! (connection, connect), `agent_grants.rs` (chats), `agent_tools.rs` (what
//! agents can do, always asks), `agent_window.rs` (window control,
//! confirmations) and `agent_activity.rs`.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::rows::group;
use crate::ui::agent_labels::agent_label;
use crate::ui::agent_state::visible_summary;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::summary::{AccessSummary, Claim, Place, Section, WindowState, WindowView};
use quill::lang::{keys, tr, tr_with, trn};
use quill::settings_catalog::Page;

/// Where the page can scroll to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum Spot {
    Section(Section),
    /// The "This chat" card the chat menus open.
    FocusChat,
}

/// One chat in the "Add a chat" picker.
#[derive(Debug, Clone)]
pub(in crate::ui) struct ChatChoice {
    id: i64,
    title: SharedString,
    secret: bool,
}

impl SearchableListItem for ChatChoice {
    type Value = i64;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(div().min_w_0().truncate().child(self.title.clone()))
            .when(self.secret, |this| {
                this.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_AGENT_CHAT_SECRET)),
                )
            })
    }

    fn value(&self) -> &i64 {
        &self.id
    }
}

pub(in crate::ui) type ChatPicker = Entity<SelectState<SearchableVec<ChatChoice>>>;

/// What the activity list shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) struct ActivityFilter {
    pub(in crate::ui) agent: Option<String>,
    pub(in crate::ui) chat: Option<i64>,
    pub(in crate::ui) tool: Option<String>,
}

/// The page's own state: where it scrolls, the chat the chat menus
/// opened it for, the activity filters and the chat picker. Everything
/// about agents themselves is read from `quill::agent` on each render.
pub(in crate::ui) struct AgentPageUi {
    pub(in crate::ui) scroll: ScrollHandle,
    anchors: Vec<(Spot, ScrollAnchor)>,
    /// Scrolled to on the next render.
    pub(in crate::ui) scroll_to: Option<Spot>,
    pub(in crate::ui) focus_chat: Option<i64>,
    pub(in crate::ui) filter: ActivityFilter,
    pub(in crate::ui) picker: Option<ChatPicker>,
    /// The chats the picker was built from (their count and the ids it
    /// leaves out), so it is rebuilt only when they change.
    picker_key: Option<(usize, Vec<i64>)>,
    /// The "Reset the agent token?" dialog is open.
    pub(in crate::ui) reset_confirm: bool,
}

impl AgentPageUi {
    pub(in crate::ui) fn new() -> Self {
        let scroll = ScrollHandle::new();
        let anchors = Section::ALL
            .into_iter()
            .map(Spot::Section)
            .chain([Spot::FocusChat])
            .map(|spot| (spot, ScrollAnchor::for_handle(scroll.clone())))
            .collect();
        Self {
            scroll,
            anchors,
            scroll_to: None,
            focus_chat: None,
            filter: ActivityFilter::default(),
            picker: None,
            picker_key: None,
            reset_confirm: false,
        }
    }

    pub(in crate::ui) fn anchor(&self, spot: Spot) -> Option<ScrollAnchor> {
        self.anchors
            .iter()
            .find(|(at, _)| *at == spot)
            .map(|(_, anchor)| anchor.clone())
    }
}

/// The section a search result opens.
pub(super) fn section_for_entry(id: &str) -> Option<Section> {
    let name = id.strip_prefix("agent_access.")?;
    Some(match name {
        "allow" | "reset_token" | "connected" => Section::Connection,
        "chats" | "all_chats" | "contacts" | "add_chat" => Section::Chats,
        "tools" | "raw_calls" => Section::Tools,
        "always_asks" => Section::AlwaysAsks,
        "window" | "takeover_effect" => Section::Window,
        "confirm_sends" => Section::Confirmations,
        "activity" => Section::Activity,
        "connect" => Section::Connect,
        _ => return None,
    })
}

fn section_title(section: Section) -> &'static str {
    tr(match section {
        Section::Connection => keys::Q_AGENT_PAGE_SEC_CONNECTION,
        Section::Chats => keys::Q_AGENT_PAGE_SEC_CHATS,
        Section::Tools => keys::Q_AGENT_PAGE_SEC_TOOLS,
        Section::AlwaysAsks => keys::Q_AGENT_PAGE_SEC_ASKS,
        Section::Window => keys::Q_AGENT_PAGE_SEC_WINDOW,
        Section::Confirmations => keys::Q_AGENT_PAGE_SEC_CONFIRM,
        Section::Activity => keys::Q_AGENT_ACTIVITY_TITLE,
        Section::Connect => keys::Q_AGENT_ACCESS_CONNECT,
    })
}

fn section_id(section: Section) -> &'static str {
    match section {
        Section::Connection => "connection",
        Section::Chats => "chats",
        Section::Tools => "tools",
        Section::AlwaysAsks => "always-asks",
        Section::Window => "window",
        Section::Confirmations => "confirmations",
        Section::Activity => "activity",
        Section::Connect => "connect",
    }
}

fn place_label(place: Place) -> String {
    match place {
        Place::SavedMessages => tr(keys::Q_AGENT_PAGE_PLACE_SAVED).to_string(),
        Place::Bots => tr(keys::Q_AGENT_PAGE_PLACE_BOTS).to_string(),
        Place::OwnChannels => tr(keys::Q_AGENT_PAGE_PLACE_OWN_CHANNELS).to_string(),
        Place::AllOtherChats => tr(keys::Q_AGENT_PAGE_PLACE_ALL_OTHER).to_string(),
        Place::Added(count) => trn(keys::Q_AGENT_PAGE_PLACE_ADDED, count),
    }
}

/// "Saved Messages, bots and 2 chats you added".
fn places_label(places: &[Place]) -> String {
    let mut words: Vec<String> = places.iter().map(|place| place_label(*place)).collect();
    match words.pop() {
        None => String::new(),
        Some(last) if words.is_empty() => last,
        Some(last) => tr_with(
            keys::Q_AGENT_PAGE_LIST_AND,
            &[("list", &words.join(", ")), ("last", &last)],
        ),
    }
}

/// A claim as one sentence, from the user's side.
pub(super) fn claim_text(claim: &Claim, summary: &AccessSummary) -> String {
    match claim {
        Claim::AccessOff => tr(keys::Q_AGENT_PAGE_CLAIM_OFF).to_string(),
        Claim::ReadWrite(places) => tr_with(
            keys::Q_AGENT_PAGE_CLAIM_READ_WRITE,
            &[("places", &places_label(places))],
        ),
        Claim::ReadOnly(places) => tr_with(
            keys::Q_AGENT_PAGE_CLAIM_READ_ONLY,
            &[("places", &places_label(places))],
        ),
        Claim::NoChats => tr(keys::Q_AGENT_PAGE_CLAIM_NO_CHATS).to_string(),
        Claim::OtherChatsHidden => tr(keys::Q_AGENT_PAGE_CLAIM_OTHERS_HIDDEN).to_string(),
        Claim::SecretChatsHidden => tr(keys::Q_AGENT_PAGE_CLAIM_SECRET_HIDDEN).to_string(),
        Claim::Contacts(true) => tr(keys::Q_AGENT_PAGE_CLAIM_CONTACTS_ON).to_string(),
        Claim::Contacts(false) => tr(keys::Q_AGENT_PAGE_CLAIM_CONTACTS_OFF).to_string(),
        Claim::RawCalls(true) => tr(keys::Q_AGENT_PAGE_CLAIM_RAW_ON).to_string(),
        Claim::RawCalls(false) => tr(keys::Q_AGENT_PAGE_CLAIM_RAW_OFF).to_string(),
        Claim::Window(state) => match state {
            WindowState::Off => tr(keys::Q_AGENT_PAGE_CLAIM_WINDOW_OFF).to_string(),
            WindowState::Ready if summary.window.view == WindowView::WholeWindow => {
                tr(keys::Q_AGENT_PAGE_CLAIM_WINDOW_READY_ALL).to_string()
            }
            WindowState::Ready => tr(keys::Q_AGENT_PAGE_CLAIM_WINDOW_READY).to_string(),
            WindowState::Driving { agent } => tr_with(
                keys::Q_AGENT_PAGE_CLAIM_WINDOW_DRIVING,
                &[("agent", &agent_label(agent))],
            ),
            WindowState::Paused => tr(keys::Q_AGENT_PAGE_CLAIM_WINDOW_PAUSED).to_string(),
            WindowState::Stopped => tr(keys::Q_AGENT_PAGE_CLAIM_WINDOW_STOPPED).to_string(),
        },
        Claim::EachSend(true) => tr(keys::Q_AGENT_PAGE_CLAIM_EACH_SEND_ON).to_string(),
        Claim::EachSend(false) => tr(keys::Q_AGENT_PAGE_CLAIM_EACH_SEND_OFF).to_string(),
        Claim::AlwaysAsks => tr(keys::Q_AGENT_PAGE_CLAIM_ALWAYS_ASKS).to_string(),
        Claim::Agents(names) => match names.as_slice() {
            [] => tr(keys::Q_AGENT_PAGE_CLAIM_AGENTS_NONE).to_string(),
            [one] => tr_with(
                keys::Q_AGENT_PAGE_CLAIM_AGENTS_ONE,
                &[("agent", &agent_label(one))],
            ),
            many => tr_with(
                keys::Q_AGENT_PAGE_CLAIM_AGENTS_MANY,
                &[("count", &many.len().to_string())],
            ),
        },
    }
}

fn claim_icon(claim: &Claim, summary: &AccessSummary) -> IconName {
    match claim {
        Claim::AccessOff => IconName::Ban,
        Claim::ReadWrite(_) => IconName::Pencil,
        Claim::ReadOnly(_) => IconName::Eye,
        Claim::NoChats | Claim::OtherChatsHidden | Claim::SecretChatsHidden => IconName::EyeOff,
        Claim::Contacts(_) => IconName::Users,
        Claim::RawCalls(_) => IconName::CodeXml,
        Claim::Window(_) if summary.window.switch => IconName::MousePointer2,
        Claim::Window(_) => IconName::Monitor,
        Claim::EachSend(_) => IconName::CircleCheck,
        Claim::AlwaysAsks => IconName::ShieldCheck,
        Claim::Agents(_) => IconName::Bot,
    }
}

impl QuillApp {
    /// The page, top to bottom: what it is, the summary card, then one
    /// section per area. With Agent access off only the switch, the log
    /// and the setup lines show; nothing else applies.
    pub(super) fn agent_access_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.agent_settings_account();
        let summary = visible_summary(&account);
        let muted = cx.theme().muted_foreground;
        let mut sections: Vec<(Section, AnyElement)> = vec![(
            Section::Connection,
            self.agent_connection_section(&summary, cx),
        )];
        if summary.enabled {
            sections.push((Section::Chats, self.agent_chats_section(&summary, cx)));
            sections.push((Section::Tools, self.agent_tools_section(&summary, cx)));
            sections.push((Section::AlwaysAsks, self.agent_asks_section(&summary, cx)));
            sections.push((Section::Window, self.agent_window_section(&summary, cx)));
            sections.push((
                Section::Confirmations,
                self.agent_confirm_section(&summary, cx),
            ));
        }
        sections.push((Section::Activity, self.agent_activity_section(cx)));
        sections.push((Section::Connect, self.agent_connect_section(cx)));
        let page = &self.agent_ui.page;
        div()
            .flex()
            .flex_col()
            .gap_8()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .id("agent-access-about")
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_AGENT_ACCESS_ABOUT)),
                    )
                    .child(self.agent_summary_card(&summary, cx)),
            )
            .children(sections.into_iter().map(|(section, body)| {
                div()
                    .id(SharedString::from(format!(
                        "agent-section-{}",
                        section_id(section)
                    )))
                    .debug_selector(move || format!("agent-section-{}", section_id(section)))
                    .anchor_scroll(page.anchor(Spot::Section(section)))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .child(section_title(section)),
                    )
                    .child(body)
            }))
            .child(
                div()
                    .id("agent-access-terms")
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_AGENT_ACCESS_TERMS)),
            )
            .into_any_element()
    }

    /// "Right now": one sentence per claim, each a button that scrolls to
    /// the section it comes from.
    fn agent_summary_card(&self, summary: &AccessSummary, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let rows = summary.claims.iter().enumerate().map(|(index, claim)| {
            let section = claim.section();
            let text = claim_text(claim, summary);
            Button::new(SharedString::from(format!("agent-claim-{index}")))
                .ghost()
                .w_full()
                .h_auto()
                .accessibility_label(text.clone())
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_3()
                        .w_full()
                        .py_1()
                        .child(
                            Icon::new(claim_icon(claim, summary))
                                .size_4()
                                .mt_0p5()
                                .flex_none()
                                .text_color(muted),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .text_left()
                                .whitespace_normal()
                                .child(text),
                        )
                        .child(
                            Icon::new(IconName::ChevronRight)
                                .size_4()
                                .mt_0p5()
                                .flex_none()
                                .text_color(muted),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.agent_ui.page.scroll_to = Some(Spot::Section(section));
                    cx.notify();
                }))
                .into_any_element()
        });
        div()
            .id("agent-summary")
            .debug_selector(|| "agent-summary".to_string())
            .child(group(Some(tr(keys::Q_AGENT_PAGE_SEC_SUMMARY)), rows))
            .into_any_element()
    }

    /// Before the page renders: build the chat picker once the window is
    /// at hand, and run a scroll the page asked for.
    pub(super) fn ensure_agent_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_agent_chat_picker(window, cx);
        if let Some(spot) = self.agent_ui.page.scroll_to.take()
            && let Some(anchor) = self.agent_ui.page.anchor(spot)
        {
            anchor.scroll_to(window, cx);
            // The offset changes on the next frame and paints nothing on
            // its own.
            window.on_next_frame(|window, _| window.refresh());
        }
    }

    fn ensure_agent_chat_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let account = self.agent_settings_account();
        let added: Vec<i64> = quill::agent::grants::for_account(&account)
            .chats
            .keys()
            .copied()
            .collect();
        let count = self.session().map_or(0, |s| s.chats.len());
        let key = (count, added.clone());
        if self.agent_ui.page.picker_key.as_ref() == Some(&key)
            && self.agent_ui.page.picker.is_some()
        {
            return;
        }
        let mut choices: Vec<ChatChoice> = self
            .session()
            .map(|s| {
                s.chats
                    .values()
                    .filter(|chat| !added.contains(&chat.id.0) && !chat.title.trim().is_empty())
                    .map(|chat| ChatChoice {
                        id: chat.id.0,
                        title: SharedString::from(chat.title.clone()),
                        secret: crate::ui::agent_state::is_secret_chat_id(chat.id.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        choices.sort_by_cached_key(|choice| (choice.title.to_lowercase(), choice.id));
        self.agent_ui.page.picker_key = Some(key);
        if let Some(picker) = self.agent_ui.page.picker.clone() {
            picker.update(cx, |picker, cx| {
                picker.set_items(SearchableVec::new(choices), window, cx);
            });
            return;
        }
        let picker = cx.new(|cx| {
            SelectState::new(SearchableVec::new(choices), None, window, cx).searchable(true)
        });
        cx.subscribe_in(
            &picker,
            window,
            |this, picker, event: &SelectEvent<SearchableVec<ChatChoice>>, window, cx| {
                let SelectEvent::Confirm(Some(chat_id)) = event else {
                    return;
                };
                this.add_agent_chat(*chat_id, cx);
                picker.update(cx, |picker, cx| picker.set_selected_index(None, window, cx));
            },
        )
        .detach();
        self.agent_ui.page.picker = Some(picker);
    }

    /// Leaving the page: forget the chat it was opened for and the picker.
    pub(super) fn leave_agent_page(&mut self) {
        self.agent_ui.page.focus_chat = None;
        self.agent_ui.page.scroll_to = None;
        self.agent_ui.page.picker = None;
        self.agent_ui.page.picker_key = None;
    }

    /// The chat menus' "Agent access": the page, scrolled to `chat_id`'s
    /// card. Never for an agent driving the window: Settings is out of
    /// its reach.
    pub(in crate::ui) fn open_agent_access_for_chat(
        &mut self,
        chat_id: i64,
        cx: &mut Context<Self>,
    ) {
        use quill::agent::ui_driver::guard;
        if guard::refuse_hard_if_injecting(guard::HardSurface::Settings) {
            return;
        }
        self.agent_ui.picked = None;
        self.open_settings(Some(Page::AgentAccess));
        self.agent_ui.page.focus_chat = Some(chat_id);
        self.agent_ui.page.scroll_to = Some(Spot::FocusChat);
        cx.notify();
    }
}

/// Screenshot demos: `QUILL_DEMO_AGENT_SECTION=<section id>|focus` scrolls
/// the page there; `QUILL_DEMO_AGENT_RAW=off` turns raw calls off.
pub(super) fn demo_scroll_from_env(app: &mut QuillApp) {
    if std::env::var("QUILL_DEMO_AGENT_RAW").is_ok_and(|raw| raw == "off") {
        quill::agent::access::set_raw_calls(false);
    }
    let Ok(name) = std::env::var("QUILL_DEMO_AGENT_SECTION") else {
        return;
    };
    app.agent_ui.page.scroll_to = if name == "focus" {
        Some(Spot::FocusChat)
    } else {
        Section::ALL
            .into_iter()
            .find(|section| section_id(*section) == name)
            .map(Spot::Section)
    };
}

impl QuillApp {
    /// Screenshot demos: a private chat with a person in the fixture.
    fn demo_agent_chat(&self) -> Option<i64> {
        let session = self.session()?;
        let mut ids: Vec<i64> = session
            .chats
            .values()
            .filter(|chat| {
                matches!(chat.kind, quill::telegram::envelope::ChatKind::Private { .. })
                    && !chat.title.trim().is_empty()
            })
            .map(|chat| chat.id.0)
            .collect();
        ids.sort_unstable();
        ids.into_iter()
            .find(|id| self.driver_chat_class(*id) == quill::agent::grants::ChatClass::Other)
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-agent-access").setup(
        |app, _, _| {
            app.open_settings(Some(Page::AgentAccess));
            demo_scroll_from_env(app);
        }
    ),
    // The page as a chat's menu opens it: the "This chat" card on top of
    // Chats. `QUILL_DEMO_AGENT=granted` adds the chat at Read first.
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-agent-chat").setup(
        |app, _, cx| {
            quill::fast_buttons::set_persistence(false);
            quill::agent::access::set_enabled(true);
            let Some(chat_id) = app.demo_agent_chat() else {
                return;
            };
            if std::env::var("QUILL_DEMO_AGENT").is_ok_and(|mode| mode == "granted") {
                app.add_agent_chat(chat_id, cx);
            }
            app.open_agent_access_for_chat(chat_id, cx);
        }
    ),
    // A chat open with Agent access on, for its header menu's entry.
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-chat-menu").setup(
        |app, window, cx| {
            quill::fast_buttons::set_persistence(false);
            quill::agent::access::set_enabled(true);
            if let Some(chat_id) = app.demo_agent_chat() {
                app.select_listed_chat(quill::ids::ChatId(chat_id), window, cx);
            }
        }
    ),
];

//! Agent access, the Chats section: the chat the chat menus opened the page
//! for, the default for each class of chat, contacts, the chats the user
//! added (with when an agent last used each) and "Add a chat". The rows
//! read the access summary on every render and write through
//! `QuillApp::choose_agent_scope` and friends; "All other chats" opens the
//! consent dialog first (`ui/agent_dialogs.rs`).
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::agent_access::{action_row, empty_line};
use super::agent_page::Spot;
use super::rows::{group, switch_row};
use crate::ui::agent_labels::now_unix;
use crate::ui::agent_state::{GrantTarget, active_account};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::select::Select;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::grants::{self, ChatClass, ChatGrant, Scope};
use quill::agent::summary::{AccessSummary, AddedChat};
use quill::ids::AccountKey;
use quill::lang::{keys, tr, tr_with};
use quill::local_time::{chat_list_stamp, civil_local};

/// The segments of a scope control, in order.
const SCOPES: [(&str, Option<Scope>); 3] = [
    ("off", None),
    ("read", Some(Scope::Read)),
    ("read-write", Some(Scope::ReadWrite)),
];

fn scope_label(scope: Option<Scope>) -> &'static str {
    match scope {
        None => tr(keys::Q_AGENT_SCOPE_OFF),
        Some(Scope::Read) => tr(keys::Q_AGENT_SCOPE_READ),
        Some(Scope::ReadWrite) => tr(keys::Q_AGENT_SCOPE_READ_WRITE),
    }
}

/// Where a chat's scope comes from, for the "This chat" card.
fn scope_source(grants: &grants::AccountGrants, chat_id: i64, class: ChatClass) -> Option<String> {
    if grants.chats.contains_key(&chat_id) {
        return Some(tr(keys::Q_AGENT_PAGE_FOCUS_ADDED).to_string());
    }
    let (default, name) = match class {
        ChatClass::Saved => (grants.saved, keys::Q_AGENT_GRANTS_SAVED),
        ChatClass::Bot => (grants.bots, keys::Q_AGENT_GRANTS_BOTS),
        ChatClass::OwnChannel => (grants.own_channels, keys::Q_AGENT_GRANTS_OWN_CHANNELS),
        ChatClass::Secret => return Some(tr(keys::Q_AGENT_PAGE_FOCUS_SECRET).to_string()),
        ChatClass::Other => (None, keys::Q_AGENT_GRANTS_ALL_CHATS),
    };
    let source = if class != ChatClass::Saved && grants.all_chats > default {
        keys::Q_AGENT_GRANTS_ALL_CHATS
    } else if default.is_some() {
        name
    } else {
        return None;
    };
    Some(tr_with(
        keys::Q_AGENT_PAGE_FOCUS_FROM,
        &[("source", tr(source))],
    ))
}

impl QuillApp {
    pub(super) fn agent_chats_section(
        &self,
        summary: &AccessSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let account = self.agent_settings_account();
        let accounts = self.agent_account_list();
        let chats = &summary.chats;
        let muted = cx.theme().muted_foreground;
        let mut section = div().flex().flex_col().gap_4();
        if accounts.len() > 1 {
            section = section.child(group(
                None,
                [self.agent_account_row(&accounts, &account, cx)],
            ));
        }
        if let Some(card) = self.agent_focus_card(&account, cx) {
            section = section.child(card);
        }
        // A class All other chats raises says so, since the control shows
        // only the class's own setting.
        let raised = |set: Option<Scope>, effective: Option<Scope>| {
            (effective != set).then(|| {
                tr_with(
                    keys::Q_AGENT_GRANTS_RAISED,
                    &[("scope", scope_label(effective))],
                )
            })
        };
        let rows = [
            (
                "saved",
                GrantTarget::Saved,
                chats.saved,
                keys::Q_AGENT_GRANTS_SAVED,
                None,
            ),
            (
                "bots",
                GrantTarget::Bots,
                chats.bots,
                keys::Q_AGENT_GRANTS_BOTS,
                raised(chats.bots, chats.effective.bots),
            ),
            (
                "own-channels",
                GrantTarget::OwnChannels,
                chats.own_channels,
                keys::Q_AGENT_GRANTS_OWN_CHANNELS,
                raised(chats.own_channels, chats.effective.own_channels),
            ),
            (
                "all-chats",
                GrantTarget::AllChats,
                chats.all_chats,
                keys::Q_AGENT_GRANTS_ALL_CHATS,
                Some(tr(keys::Q_AGENT_GRANTS_ALL_CHATS_HINT).to_string()),
            ),
        ];
        let rows: Vec<AnyElement> = rows
            .into_iter()
            .map(|(id, target, scope, title, hint)| {
                let control = scope_control(
                    format!("agent-grant-{id}"),
                    scope,
                    move |this, scope, cx| {
                        this.choose_agent_scope(target, scope, cx);
                    },
                    cx,
                );
                grant_row(
                    tr(title).into(),
                    hint.map(Into::into),
                    control,
                    None,
                    muted,
                )
            })
            .collect();
        section = section
            .child(group(Some(tr(keys::Q_AGENT_GRANTS_TITLE)), rows))
            .child(
                div()
                    .id("agent-grants-caption")
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_AGENT_GRANTS_CAPTION)),
            )
            .child(group(
                None,
                [switch_row(
                    "agent-contacts-switch",
                    tr(keys::Q_AGENT_CONTACTS_TITLE),
                    Some(tr(keys::Q_AGENT_CONTACTS_HINT)),
                    chats.contacts,
                    cx.listener(|this, &next: &bool, _, cx| this.set_agent_contacts(next, cx)),
                    cx,
                )],
            ));
        section
            .children(self.agent_added_chats(&chats.added, cx))
            .child(
                div()
                    .id("agent-secret-note")
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_AGENT_PAGE_SECRET_NOTE)),
            )
            .into_any_element()
    }

    /// "This chat": the chat the chat menus opened the page for, its scope
    /// and where that comes from, and a control that sets it for this chat
    /// alone.
    fn agent_focus_card(&self, account: &AccountKey, cx: &mut Context<Self>) -> Option<AnyElement> {
        let chat_id = self.agent_ui.page.focus_chat?;
        // The chat belongs to the active account; another account's grants
        // say nothing about it.
        if account != &active_account() {
            return None;
        }
        let grants = grants::for_account(account);
        let class = self.driver_chat_class(chat_id);
        let scope = grants.scope_for(chat_id, class);
        let title = self.agent_chat_title(chat_id);
        let hint = scope_source(&grants, chat_id, class)
            .unwrap_or_else(|| tr(keys::Q_AGENT_PAGE_FOCUS_NONE).to_string());
        let control = scope_control(
            "agent-focus-chat".to_string(),
            scope,
            move |this, scope, cx| {
                this.set_agent_chat_scope(chat_id, scope, cx);
            },
            cx,
        );
        let row = grant_row(
            title.into(),
            Some(hint.into()),
            control,
            None,
            cx.theme().muted_foreground,
        );
        Some(
            div()
                .id("agent-focus-chat-card")
                .debug_selector(|| "agent-focus-chat-card".to_string())
                .anchor_scroll(self.agent_ui.page.anchor(Spot::FocusChat))
                .child(group(Some(tr(keys::Q_AGENT_PAGE_FOCUS_TITLE)), [row]))
                .into_any_element(),
        )
    }

    /// "Chats you added": one row per entry, or the empty state, then
    /// "Add a chat".
    fn agent_added_chats(&self, added: &[AddedChat], cx: &mut Context<Self>) -> Vec<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let now = civil_local(now_unix());
        let mut entries: Vec<&AddedChat> = added.iter().collect();
        entries.sort_by_cached_key(|chat| (chat.title.to_lowercase(), chat.chat_id));
        let mut rows: Vec<AnyElement> = if entries.is_empty() {
            vec![empty_line(
                "agent-chats-empty",
                tr(keys::Q_AGENT_CHATS_EMPTY),
                cx,
            )]
        } else {
            entries
                .into_iter()
                .map(|chat| {
                    let chat_id = chat.chat_id;
                    let control = scope_control(
                        format!("agent-chat-{chat_id}"),
                        chat.scope,
                        move |this, scope, cx| {
                            this.choose_agent_scope(GrantTarget::Chat(chat_id), scope, cx);
                        },
                        cx,
                    );
                    let remove =
                        Button::new(SharedString::from(format!("agent-chat-{chat_id}-remove")))
                            .icon(IconName::X)
                            .ghost()
                            .small()
                            .accessibility_label(tr(keys::Q_AGENT_CHAT_REMOVE))
                            .tooltip(tr(keys::Q_AGENT_CHAT_REMOVE))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_agent_chat(chat_id, cx)
                            }))
                            .into_any_element();
                    let used = match chat.last_used {
                        Some(time) => tr_with(
                            keys::Q_AGENT_PAGE_LAST_USED,
                            &[("time", &chat_list_stamp(&civil_local(time), &now))],
                        ),
                        None => tr(keys::Q_AGENT_PAGE_NOT_USED).to_string(),
                    };
                    let hint = if chat.secret {
                        format!("{} · {used}", tr(keys::Q_AGENT_CHAT_SECRET))
                    } else {
                        used
                    };
                    grant_row(
                        SharedString::from(chat.title.clone()),
                        Some(hint.into()),
                        control,
                        Some(remove),
                        muted,
                    )
                })
                .collect()
        };
        if let Some(picker) = self.agent_ui.page.picker.as_ref() {
            rows.push(action_row(
                tr(keys::Q_AGENT_PAGE_ADD_CHAT),
                Some(tr(keys::Q_AGENT_PAGE_ADD_CHAT_HINT).into()),
                div().id("agent-add-chat").w(px(240.)).child(
                    Select::new(picker)
                        .placeholder(tr(keys::Q_AGENT_PAGE_ADD_CHAT))
                        .search_placeholder(tr(keys::Q_AGENT_PAGE_ADD_CHAT_SEARCH))
                        .accessibility_label(tr(keys::Q_AGENT_PAGE_ADD_CHAT))
                        .menu_max_h(px(320.))
                        .small(),
                ),
                cx,
            ));
        }
        vec![
            group(Some(tr(keys::Q_AGENT_CHATS_TITLE)), rows).into_any_element(),
            div()
                .id("agent-chats-caption")
                .text_xs()
                .text_color(muted)
                .child(tr(keys::Q_AGENT_CHATS_HINT))
                .into_any_element(),
        ]
    }

    /// A chat's title as Quill knows it now, else as it was granted.
    fn agent_chat_title(&self, chat_id: i64) -> String {
        self.session()
            .and_then(|s| s.chats.get(&chat_id))
            .map(|chat| chat.title.clone())
            .filter(|title| !title.trim().is_empty())
            .or_else(|| {
                grants::for_account(&self.agent_settings_account())
                    .chats
                    .get(&chat_id)
                    .map(|grant| grant.title.clone())
            })
            .unwrap_or_default()
    }

    /// "Add a chat": the picked chat at Read, the narrowest scope.
    pub(super) fn add_agent_chat(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let account = self.agent_settings_account();
        let title = self.agent_chat_title(chat_id);
        grants::update(&account, |g| {
            g.chats.entry(chat_id).or_insert(ChatGrant {
                scope: Some(Scope::Read),
                title,
            });
        });
        cx.notify();
    }

    /// The "This chat" card's control: an entry of its own for the chat,
    /// unless the pick matches what it has already.
    fn set_agent_chat_scope(&mut self, chat_id: i64, scope: Option<Scope>, cx: &mut Context<Self>) {
        let account = self.agent_settings_account();
        let current = grants::for_account(&account);
        if current.chats.contains_key(&chat_id) {
            self.choose_agent_scope(GrantTarget::Chat(chat_id), scope, cx);
            return;
        }
        if current.scope_for(chat_id, self.driver_chat_class(chat_id)) == scope {
            return;
        }
        let title = self.agent_chat_title(chat_id);
        grants::update(&account, |g| {
            g.chats.insert(chat_id, ChatGrant { scope, title });
        });
        cx.notify();
    }

    /// The account the grants below belong to, as a menu button.
    fn agent_account_row(
        &self,
        accounts: &[quill::settings::AccountRecord],
        selected: &AccountKey,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        let current = accounts
            .iter()
            .find(|record| &record.key == selected)
            .map(|record| record.display_name.clone())
            .unwrap_or_default();
        let choices: Vec<(AccountKey, String)> = accounts
            .iter()
            .map(|record| (record.key.clone(), record.display_name.clone()))
            .collect();
        let selected = selected.clone();
        let picker = Button::new("agent-account")
            .label(current)
            .outline()
            .small()
            .dropdown_caret(true)
            .accessibility_label(tr(keys::Q_AGENT_ACCOUNT))
            .dropdown_menu(move |mut menu, _, _| {
                for (key, name) in &choices {
                    let owner = owner.clone();
                    let picked = key.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(key == &selected)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.agent_ui.picked = Some(picked.clone());
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });
        action_row(tr(keys::Q_AGENT_ACCOUNT), None, picker, cx)
    }
}

/// One row: title and an optional line under it on the left, the scope
/// control (and a remove button) on the right.
fn grant_row(
    title: SharedString,
    hint: Option<SharedString>,
    control: ButtonGroup,
    remove: Option<AnyElement>,
    muted: Hsla,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_3()
        .w_full()
        .min_h(px(40.))
        .px_3()
        .py_2()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().truncate().child(title))
                .when_some(hint, |this, hint| {
                    this.child(div().text_xs().text_color(muted).child(hint))
                }),
        )
        .child(div().flex_shrink_0().child(control))
        .when_some(remove, |this, remove| {
            this.child(div().flex_shrink_0().child(remove))
        })
        .into_any_element()
}

/// Off, Read, Read and write as one segmented control.
fn scope_control(
    id: String,
    current: Option<Scope>,
    on_pick: impl Fn(&mut QuillApp, Option<Scope>, &mut Context<QuillApp>) + 'static,
    cx: &mut Context<QuillApp>,
) -> ButtonGroup {
    let segments = SCOPES.iter().map(|&(name, scope)| {
        Button::new(SharedString::from(format!("{id}-{name}")))
            .label(scope_label(scope))
            .selected(current == scope)
            .when(current == scope, |button| button.primary())
    });
    ButtonGroup::new(SharedString::from(id.clone()))
        .small()
        .children(segments)
        .on_click(cx.listener(move |this, picked: &Vec<usize>, _, cx| {
            let scope = picked
                .first()
                .and_then(|&index| SCOPES.get(index))
                .map(|&(_, scope)| scope);
            if let Some(scope) = scope {
                on_pick(this, scope, cx);
            }
        }))
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_grants_tests.rs"]
mod tests;

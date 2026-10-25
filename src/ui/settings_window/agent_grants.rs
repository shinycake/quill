//! Settings > Advanced > Agent access, the part under the switch: which
//! chats agents may use and with what scope (`quill::agent::grants`). The
//! rows read the grants on every render and write through
//! `QuillApp::choose_agent_scope` and friends; "All other chats" opens the
//! consent dialog first (`ui/agent_dialogs.rs`).
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::rows::{group, switch_row};
use crate::ui::agent_state::{GrantTarget, is_secret_chat_id, scope_of};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::grants::{self, Scope};
use quill::ids::AccountKey;
use quill::lang::{keys, tr};

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

impl QuillApp {
    /// Everything under the switch: the account picker (with several
    /// accounts), the default rows, contacts and the chats the user added.
    pub(super) fn agent_grants_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let account = self.agent_settings_account();
        let accounts = self.agent_account_list();
        let grants = grants::for_account(&account);
        let muted = cx.theme().muted_foreground;
        let mut section = Vec::new();
        if accounts.len() > 1 {
            section.push(
                group(None, [self.agent_account_row(&accounts, &account, cx)]).into_any_element(),
            );
        }
        let rows = [
            (
                "saved",
                GrantTarget::Saved,
                tr(keys::Q_AGENT_GRANTS_SAVED),
                None,
            ),
            (
                "bots",
                GrantTarget::Bots,
                tr(keys::Q_AGENT_GRANTS_BOTS),
                None,
            ),
            (
                "own-channels",
                GrantTarget::OwnChannels,
                tr(keys::Q_AGENT_GRANTS_OWN_CHANNELS),
                None,
            ),
            (
                "all-chats",
                GrantTarget::AllChats,
                tr(keys::Q_AGENT_GRANTS_ALL_CHATS),
                Some(tr(keys::Q_AGENT_GRANTS_ALL_CHATS_HINT)),
            ),
        ];
        let rows: Vec<AnyElement> = rows
            .into_iter()
            .map(|(id, target, title, hint)| {
                let control = scope_control(
                    format!("agent-grant-{id}"),
                    scope_of(&grants, target),
                    target,
                    cx,
                );
                grant_row(title.into(), hint.map(Into::into), control, None, muted)
            })
            .collect();
        section.push(group(Some(tr(keys::Q_AGENT_GRANTS_TITLE)), rows).into_any_element());
        section.push(
            div()
                .id("agent-grants-caption")
                .text_xs()
                .text_color(muted)
                .child(tr(keys::Q_AGENT_GRANTS_CAPTION))
                .into_any_element(),
        );
        section.push(
            group(
                None,
                [switch_row(
                    "agent-contacts-switch",
                    tr(keys::Q_AGENT_CONTACTS_TITLE),
                    Some(tr(keys::Q_AGENT_CONTACTS_HINT)),
                    grants.contacts,
                    cx.listener(|this, &next: &bool, _, cx| this.set_agent_contacts(next, cx)),
                    cx,
                )],
            )
            .into_any_element(),
        );
        section.extend(self.agent_added_chats(&grants, cx));
        section
    }

    /// "Chats you added": one row per entry, or the empty state.
    fn agent_added_chats(
        &self,
        grants: &grants::AccountGrants,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let mut entries: Vec<_> = grants.chats.iter().collect();
        entries.sort_by_cached_key(|(id, grant)| (grant.title.to_lowercase(), **id));
        if entries.is_empty() {
            return vec![
                group(
                    Some(tr(keys::Q_AGENT_CHATS_TITLE)),
                    [div()
                        .id("agent-chats-empty")
                        .debug_selector(|| "agent-chats-empty".to_string())
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(keys::Q_AGENT_CHATS_EMPTY))
                        .into_any_element()],
                )
                .into_any_element(),
            ];
        }
        let rows: Vec<AnyElement> = entries
            .into_iter()
            .map(|(&chat_id, grant)| {
                let control = scope_control(
                    format!("agent-chat-{chat_id}"),
                    grant.scope,
                    GrantTarget::Chat(chat_id),
                    cx,
                );
                let remove =
                    Button::new(SharedString::from(format!("agent-chat-{chat_id}-remove")))
                        .icon(IconName::X)
                        .ghost()
                        .small()
                        .accessibility_label(tr(keys::Q_AGENT_CHAT_REMOVE))
                        .tooltip(tr(keys::Q_AGENT_CHAT_REMOVE))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.remove_agent_chat(chat_id, cx)),
                        )
                        .into_any_element();
                let hint = is_secret_chat_id(chat_id)
                    .then(|| SharedString::from(tr(keys::Q_AGENT_CHAT_SECRET)));
                grant_row(
                    SharedString::from(grant.title.clone()),
                    hint,
                    control,
                    Some(remove),
                    muted,
                )
            })
            .collect();
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
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(tr(keys::Q_AGENT_ACCOUNT)),
            )
            .child(picker)
            .into_any_element()
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
    target: GrantTarget,
    cx: &mut Context<QuillApp>,
) -> ButtonGroup {
    let segments = SCOPES.iter().map(|&(name, scope)| {
        Button::new(SharedString::from(format!("{id}-{name}")))
            .label(scope_label(scope))
            .selected(current == scope)
    });
    ButtonGroup::new(SharedString::from(id.clone()))
        .outline()
        .small()
        .children(segments)
        .on_click(cx.listener(move |this, picked: &Vec<usize>, _, cx| {
            let scope = picked
                .first()
                .and_then(|&index| SCOPES.get(index))
                .map(|&(_, scope)| scope);
            if let Some(scope) = scope {
                this.choose_agent_scope(target, scope, cx);
            }
        }))
}

/// The page with access on, a second account, a wider default, a blocked
/// chat and a secret chat added. `QUILL_DEMO_AGENT=empty` leaves the added
/// chats empty and the account single.
fn demo_grants(app: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    use quill::agent::grants::ChatGrant;
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    let empty = std::env::var("QUILL_DEMO_AGENT").is_ok_and(|mode| mode == "empty");
    if !empty && let Some(root) = quill::settings::safe_app_root() {
        let _ = quill::settings::add_account(&root, "Work");
    }
    let account = crate::ui::agent_state::active_account();
    grants::update(&account, |g| {
        if empty {
            return;
        }
        g.all_chats = Some(Scope::Read);
        g.contacts = true;
        let mut add = |id: i64, title: &str, scope: Option<Scope>| {
            g.chats.insert(
                id,
                ChatGrant {
                    scope,
                    title: title.to_string(),
                },
            );
        };
        add(7, "Ann Lee", Some(Scope::ReadWrite));
        add(-1_001_234_567_890, "Family", None);
        add(-1_999_000_000_000, "Ben Ortiz", Some(Scope::Read));
    });
    app.open_settings(None);
    app.settings.page = Some(quill::settings_catalog::Page::Advanced);
    app.settings.sub = Some(super::SubPage::AgentAccess);
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-agent-grants")
        .setup(demo_grants),
];

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_grants_tests.rs"]
mod tests;

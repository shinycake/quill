use super::super::app::QuillApp;
use super::super::pressable::action_row;
use super::super::shell::{DialogKind, QuillShell};
use super::super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{InfoPanelTarget, Session};
use quill::telegram::client::copy_and_parse;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// Slice G10: communities create + hub + info UI
/// (`parity:communities-create/hub/info`). Backend landed earlier —
/// `create_community` / `get_community_full_info` / `set_community_name`
/// drivers in connect.rs plus state sync in state.rs — this module is
/// the UI layer only: the create dialog, the owned-communities hub
/// dialog, and the info-panel body. Name edits reuse `UsernameDialog`
/// with `TextPromptKind::CommunityName`; the panel target is
/// `InfoPanelTarget::Community`. Dialog state bundles into
/// `CommunityUi` (one field on `QuillApp`) instead of one field per
/// dialog.
#[derive(Default)]
pub struct CommunityUi {
    pub(crate) create_dialog: Option<CreateCommunityDialog>,
    pub(crate) hub_open: bool,
}

/// Slice G10: "New community" dialog (side-menu entry). `createCommunity`
/// (schema 1.8.67, line 11806) upgrades an existing chat into a
/// community: the picker is single-select over the chat list, and
/// `is_chat_hidden` (line 11805: the chat is visible only to
/// community administrators) is a creation-time checkbox — there is no
/// TDLib method to toggle it afterwards
/// (`parity:communities-chat-visibility` stays blocked).
pub struct CreateCommunityDialog {
    pub(crate) name_input: Entity<TextareaState>,
    pub(crate) search_input: Entity<TextareaState>,
    pub(crate) chat_id: Option<i64>,
    pub(crate) hide_chat: bool,
}

impl CreateCommunityDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Community name")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search chats")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            name_input,
            search_input,
            chat_id: None,
            hide_chat: false,
        }
    }
}

/// Slice G10: (chat_id, title) rows for the base-chat picker, filtered
/// by the search query and sorted by title.
pub(crate) fn community_chat_rows(session: &Session, query: &str) -> Vec<(i64, String)> {
    let query = query.trim().to_lowercase();
    let mut rows: Vec<(i64, String)> = session
        .chats
        .values()
        .filter(|chat| query.is_empty() || chat.title.to_lowercase().contains(&query))
        .map(|chat| (chat.id.0, chat.title.clone()))
        .collect();
    rows.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then_with(|| a.0.cmp(&b.0))
    });
    rows
}

/// Slice G10: kit dialog builder for the create-community dialog (see
/// `build_create_chat_dialog` for the kit `window.open_dialog` hosting
/// pattern).
pub fn build_create_community_dialog(
    app: &Entity<QuillApp>,
    shell: &Entity<QuillShell>,
    dialog: Dialog,
    cx: &mut App,
) -> Dialog {
    let on_close =
        QuillShell::on_close_kind(app, shell, DialogKind::CommunityCreate, |this, _, cx| {
            this.close_create_community_dialog(cx);
        });
    app.update(cx, |this, cx| {
        let dialog = dialog.overlay(true);
        let Some(dialog_state) = this.community_ui.create_dialog.as_ref() else {
            return dialog
                .title(crate::ui::shell::dialog_title("New community"))
                .on_close(on_close);
        };
        let query = EditProfileDialog::text(&dialog_state.search_input, cx);
        let rows = this
            .session()
            .map(|session| community_chat_rows(session, &query))
            .unwrap_or_default();
        let selected = dialog_state.chat_id;
        let hide_chat = dialog_state.hide_chat;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div().flex_1().child(
                    Textarea::new(&dialog_state.name_input)
                        .aria_label("Community name")
                        .h(px(40.)),
                ),
            )
            .child(
                div().flex_1().child(
                    Textarea::new(&dialog_state.search_input)
                        .aria_label("Search chats to add to community")
                        .h(px(40.)),
                ),
            );
        let mut list = div()
            .id("g10-create-chats")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.))
            .overflow_y_scroll();
        if rows.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No chats found"),
            );
        }
        for (chat_id, title) in rows.iter().take(50) {
            let chat_id = *chat_id;
            let is_selected = selected == Some(chat_id);
            let title = title.clone();
            list = list.child(
                div()
                    .id(format!("g10-create-chat-{chat_id}"))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        // Single-select: clicking a picked chat clears it.
                        Checkbox::new(format!("g10-create-toggle-{chat_id}"))
                            .checked(is_selected)
                            .label(title)
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if on != is_selected {
                                    this.toggle_create_community_chat(chat_id, cx);
                                }
                            })),
                    ),
            );
        }
        body = body.child(list);
        body = body.child(
            div()
                .id("g10-create-hide")
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Checkbox::new("g10-create-hide-toggle")
                        .checked(hide_chat)
                        .label("Hide the community chat (visible to admins only)")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(dialog) = this.community_ui.create_dialog.as_mut() {
                                dialog.hide_chat = on;
                            }
                            cx.notify();
                        })),
                ),
        );
        let footer = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("g10-create-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_create_community_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::CommunityCreate, window, cx);
                    })),
            )
            .child(
                Button::new("g10-create-submit")
                    .label("Create community")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_create_community_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::CommunityCreate, window, cx);
                    })),
            );
        let body = body.into_any_element();
        dialog
            .title(crate::ui::shell::dialog_title("New community"))
            .content(crate::ui::shell::scrollable_dialog_content({
                // `content` needs an `Fn` closure, but the body is built once
                // per dialog render — hand it over through a one-shot cell.
                let body = Rc::new(RefCell::new(Some(body)));
                move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                }
            }))
            .footer(footer)
            .on_close(on_close)
    })
}

/// Slice G10: kit dialog builder for the communities hub — the owned
/// communities from `session.communities` with per-row "Info" buttons
/// opening the info panel. No per-row state, so the hub is a bare
/// `hub_open` flag.
pub fn build_community_hub_dialog(
    app: &Entity<QuillApp>,
    shell: &Entity<QuillShell>,
    dialog: Dialog,
    cx: &mut App,
) -> Dialog {
    let on_close =
        QuillShell::on_close_kind(app, shell, DialogKind::CommunityHub, |this, _, cx| {
            this.close_community_hub(cx);
        });
    app.update(cx, |this, cx| {
        let dialog = dialog.overlay(true);
        let communities: Vec<(i64, String)> = this
            .session()
            .map(|session| {
                // An inaccessible community (`have_access = false`, e.g.
                // right after `deleteCommunity`) can't be passed to any
                // method, so it has no place in the hub.
                let mut rows: Vec<(i64, String)> = session
                    .communities
                    .values()
                    .filter(|community| community.have_access)
                    .map(|community| (community.id, community.name.clone()))
                    .collect();
                rows.sort_by_key(|a| a.1.to_lowercase());
                rows
            })
            .unwrap_or_default();
        let mut body = div().flex().flex_col().gap_1();
        if communities.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No communities yet — create one from the side menu."),
            );
        }
        for (community_id, name) in communities {
            body = body.child(
                div()
                    .id(format!("g10-hub-row-{community_id}"))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().flex_1().text_sm().font_semibold().child(name))
                    .child(
                        // Parity slice `parity:communities-chatlist-mode`:
                        // enter chat-list mode for this community — closes
                        // the hub, filters the main chat list to the
                        // community's chats, and fires
                        // `getCommunityFullInfo` so membership resolves.
                        Button::new(format!("cm-hub-view-chats-{community_id}"))
                            .label("💬 View chats")
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_community_hub(cx);
                                this.enter_community_chat_list_mode(community_id, cx);
                                this.close_kit_dialog_if_done(DialogKind::CommunityHub, window, cx);
                            })),
                    )
                    .child(
                        Button::new(format!("g10-hub-info-{community_id}"))
                            .label("ℹ Info")
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_community_hub(cx);
                                this.open_info_panel_target(
                                    InfoPanelTarget::Community(community_id),
                                    window,
                                    cx,
                                );
                                this.close_kit_dialog_if_done(DialogKind::CommunityHub, window, cx);
                            })),
                    ),
            );
        }
        let body = body.into_any_element();
        dialog
            .title(crate::ui::shell::dialog_title("Communities"))
            .content(crate::ui::shell::scrollable_dialog_content({
                let body = Rc::new(RefCell::new(Some(body)));
                move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                }
            }))
            .on_close(on_close)
    })
}

/// Slice G10: the community info panel body for
/// `InfoPanelTarget::Community` — name with an edit button (opens the
/// `CommunityName` text prompt; shown with `can_change_info`), full-info
/// counts, the member chats with hidden badges, and the TDLib 1.8.68
/// management controls: the member chat-list permission switch
/// (`can_ban_members`) and "Delete community" (owner). `getCommunityFullInfo` is kicked off by
/// `open_info_panel_target`; a missing pack renders "Loading…".
pub fn render_community_info_panel(
    app: &QuillApp,
    community_id: i64,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let session = app.session();
    let Some(community) = session.and_then(|s| s.communities.get(&community_id).cloned()) else {
        return div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("Community not found")
            .into_any_element();
    };
    let full_info = session
        .and_then(|s| s.community_full_infos.get(&community_id))
        .cloned();
    let mut body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_3()
        .p_4()
        .child(
            div()
                .text_lg()
                .font_semibold()
                .child(community.name.clone()),
        )
        .when(community.can_change_info, |body| {
            body.child(
                Button::new(format!("g10-info-edit-name-{community_id}"))
                    .label("✏️ Edit name")
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_community_name_dialog(community_id, window, cx);
                    })),
            )
        });
    body = body.child(match full_info {
        Some(info) => {
            let stats = div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "{} admins · {} banned · {} pending requests",
                    info.administrator_count, info.banned_count, info.add_chat_request_count
                ));
            let mut chats = div().flex().flex_col().gap_1().w_full();
            chats = chats.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Chats ({})", info.chats.len())),
            );
            for chat in &info.chats {
                let title = session
                    .and_then(|s| s.chats.get(&chat.chat_id))
                    .map(|c| c.title.clone())
                    .unwrap_or_else(|| format!("Chat {}", chat.chat_id));
                let mut row = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().flex_1().text_sm().child(title));
                if chat.is_hidden {
                    row = row.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("hidden"),
                    );
                }
                chats = chats.child(row);
            }
            div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(stats)
                .child(chats)
        }
        None => div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("Loading…"),
    });
    // TDLib 1.8.68 `setCommunityPermissions` (needs `can_ban_members`):
    // the community's single member permission. Not optimistic — the
    // switch follows `updateCommunity`.
    if community.can_ban_members {
        let allowed = community.members_can_edit_chat_list;
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().child("Members can edit the chat list"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Let members add and remove community chats"),
                        ),
                )
                .child(
                    div().flex_none().child(
                        Switch::new(format!("community-members-edit-chats-{community_id}"))
                            .checked(allowed)
                            .accessibility_label("Members can edit the chat list")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.set_community_members_can_edit_chat_list(community_id, on, cx);
                            })),
                    ),
                ),
        );
    }
    // TDLib 1.8.68 `deleteCommunity` — owner only, behind a confirm.
    if community.is_owner {
        body = body.child(
            action_row(
                format!("community-delete-{community_id}"),
                Some(gpui_kit::assets::IconName::Trash),
                "Delete community",
                true,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_group_confirm(
                    quill::ids::ChatId(0),
                    GroupConfirmAction::DeleteCommunity { community_id },
                    cx,
                );
            })),
        );
    }
    body.into_any_element()
}

/// Slice G10: injected community fixtures (no live Telegram) — two
/// communities plus a full-info pack for the first, through the real
/// `updateCommunity` / `updateCommunityFullInfo` reducer path.
pub fn apply_ready_communities(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":9001,"have_access":true,"name":"Rustaceans","date":1759000000,"status":{"@type":"communityMemberStatusCreator"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":true}}}"#
            .to_string(),
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":9002,"have_access":true,"name":"Demo makers","date":1759100000,"status":{"@type":"communityMemberStatusMember"},"permissions":{"@type":"communityPermissions"}}}"#
            .to_string(),
        r#"{"@type":"updateCommunityFullInfo","community_id":9001,"community_full_info":{"@type":"communityFullInfo","chats":[{"@type":"communityChat","chat_id":11,"can_view_history":true,"is_hidden":false},{"@type":"communityChat","chat_id":13,"can_view_history":true,"is_hidden":true}],"administrator_count":3,"banned_count":1,"add_chat_request_count":2}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

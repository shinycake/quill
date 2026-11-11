//! The community boxes: the community picker (`ShowAddToCommunityBox`),
//! the visibility choice (`ChooseVisibilityBox`) and the confirmations.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use quill::lang::trn;
use quill::local_path::sandboxed_display_path;
use std::cell::RefCell;
use std::rc::Rc;

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;

/// Hands a body built once per render to the dialog's `Fn` content.
pub(super) fn once(
    body: AnyElement,
) -> impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static {
    let body = Rc::new(RefCell::new(Some(body)));
    move |content, _, _| {
        let body = body
            .borrow_mut()
            .take()
            .unwrap_or_else(|| div().into_any_element());
        content.child(body)
    }
}

impl CommunityBox {
    pub(super) fn kind(&self) -> DialogKind {
        match self {
            Self::Pick { .. } => DialogKind::CommunityPick,
            Self::ChooseChat { .. } => DialogKind::CommunityChooseChat,
            Self::Visibility { .. } | Self::CreateVisibility { .. } => {
                DialogKind::CommunityVisibility
            }
            Self::Create { .. } => DialogKind::CommunityIdentity,
            Self::Requests { .. } => DialogKind::CommunityRequests,
            Self::HiddenChat { .. } => DialogKind::CommunityHiddenChat,
            Self::MuteFor { .. } => DialogKind::CommunityMuteFor,
            Self::ConfirmAdd { .. }
            | Self::ConfirmRemove { .. }
            | Self::ConfirmJoin { .. }
            | Self::ConfirmProcessAll { .. }
            | Self::Ungroup { .. }
            | Self::BanWarning { .. } => DialogKind::CommunityConfirm,
        }
    }
}

impl QuillApp {
    pub(super) fn community_chat_avatar(&self, chat_id: i64, title: &str, size: f32) -> AnyElement {
        let roots = self.media_display_roots();
        let photo = self
            .session()
            .and_then(|s| s.chat_photo_path(quill::ids::ChatId(chat_id)))
            .and_then(|path| sandboxed_display_path(path, &roots));
        crate::ui::chat_row::chat_avatar(title, photo.as_deref(), size).into_any_element()
    }

    fn community_pick_row(
        &self,
        community_id: i64,
        name: String,
        chat_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        div()
            .id(("community-pick", community_id as u64))
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .py_1p5()
            .rounded_md()
            .cursor_pointer()
            .role(Role::Button)
            .aria_label(name.clone())
            .hover(|row| row.bg(cx.theme().list_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.choose_community_visibility(
                    community_id,
                    chat_id,
                    Some(Back::Pick { chat_id }),
                    cx,
                );
            }))
            .child(self.community_userpic(community_id, 40., cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().font_semibold().truncate().child(name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::COMMUNITY_STATUS)),
                    ),
            )
            .into_any_element()
    }

    fn build_community_pick_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CommunityPick, |this, _, cx| {
                this.flush_and_close_community_box(cx)
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(BOX_WIDTH));
            let Some(CommunityBox::Pick { chat_id }) = this.admin.community.flow.open else {
                return dialog.on_close(on_close);
            };
            let kind = this.community_chat_kind(chat_id);
            let joined: Vec<(i64, String)> = this
                .session()
                .and_then(|s| {
                    let ids = s.communities.joined.data()?;
                    Some(
                        ids.iter()
                            .filter_map(|id| s.groups.communities.get(id))
                            .filter(|community| community.have_access)
                            .map(|community| (community.id, community.name.clone()))
                            .collect(),
                    )
                })
                .unwrap_or_default();
            let loading = this
                .session()
                .is_some_and(|s| s.communities.joined.is_loading());
            let joined_error = this
                .session()
                .and_then(|s| s.communities.joined.error().map(str::to_string));
            let muted = cx.theme().muted_foreground;
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_sm().text_color(muted).child(by_kind(
                    kind,
                    keys::COMMUNITY_ADD_ABOUT,
                    keys::COMMUNITY_ADD_ABOUT_CHANNEL,
                    keys::COMMUNITY_ADD_ABOUT_BOT,
                )))
                .child(
                    crate::ui::pressable::action_row(
                        "community-pick-create",
                        Some(IconName::Plus),
                        tr(keys::COMMUNITY_CREATE),
                        false,
                        cx,
                    )
                    .text_color(cx.theme().primary)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_new_community(chat_id, window, cx);
                    })),
                );
            if !joined.is_empty() {
                body = body.child(div().h(px(1.)).bg(cx.theme().border)).child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(cx.theme().accent_foreground)
                        .child(tr(keys::COMMUNITY_EXISTING)),
                );
                for (community_id, name) in joined {
                    body = body.child(this.community_pick_row(community_id, name, chat_id, cx));
                }
            } else if loading {
                body = body.child(
                    div()
                        .py_2()
                        .flex()
                        .justify_center()
                        .child(gpui_kit::component::spinner::Spinner::new()),
                );
            } else if let Some(error) = joined_error {
                // The list did not load: say so, with a Retry, instead of
                // showing only "Create".
                body = body.child(
                    div()
                        .py_2()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_2()
                        .child(div().text_sm().text_color(muted).child(error))
                        .child(
                            Button::new("community-pick-retry")
                                .label(tr(keys::Q_CHAT_RETRY))
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(live) = this.live.as_mut()
                                        && let Err(err) = live.driver.fetch_joined_communities()
                                    {
                                        this.set_note(crate::ui::status_note::StatusNote::error(
                                            crate::ui::user_error::user_error(&err),
                                        ));
                                    }
                                    cx.notify();
                                })),
                        ),
                );
            }
            let footer = div().flex().justify_end().child(
                Button::new("community-pick-close")
                    .label(tr(keys::CLOSE))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.flush_and_close_community_box(cx))),
            );
            dialog
                .title(crate::ui::shell::dialog_title(tr(keys::COMMUNITY_TITLE)))
                .content(crate::ui::shell::scrollable_dialog_content(once(
                    body.into_any_element(),
                )))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn build_community_visibility_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::CommunityVisibility,
            |this, _, cx| this.flush_and_close_community_box(cx),
        );
        let ok_app = app.clone();
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(BOX_WIDTH));
            let session = this.session();
            let (hidden, busy, confirm) = match &this.admin.community.flow.open {
                Some(CommunityBox::Visibility {
                    community_id,
                    chat_id,
                    hidden,
                    ..
                }) => (
                    *hidden,
                    session.is_some_and(|s| s.community_add_in_flight(*community_id, *chat_id)),
                    tr(keys::COMMUNITY_ADD_TO),
                ),
                Some(CommunityBox::CreateVisibility { hidden, .. }) => (
                    *hidden,
                    session.is_some_and(|s| {
                        s.requests
                            .has_purpose(quill::state::RequestPurpose::CreateCommunity)
                    }),
                    tr(keys::CREATE_GROUP_CREATE),
                ),
                _ => return dialog.on_close(on_close),
            };
            let muted = cx.theme().muted_foreground;
            let option = |label: Key, about: Key| {
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(tr(label))
                    .child(div().text_xs().text_color(muted).child(tr(about)))
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    RadioGroup::vertical("community-visibility")
                        .selected_index(Some(usize::from(hidden)))
                        .child(Radio::new("community-visibility-visible").child(option(
                            keys::COMMUNITY_VISIBILITY_VISIBLE,
                            keys::COMMUNITY_VISIBILITY_VISIBLE_ABOUT,
                        )))
                        .child(Radio::new("community-visibility-hidden").child(option(
                            keys::COMMUNITY_VISIBILITY_HIDDEN,
                            keys::COMMUNITY_VISIBILITY_HIDDEN_ABOUT,
                        )))
                        .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                            if let Some(
                                CommunityBox::Visibility { hidden, .. }
                                | CommunityBox::CreateVisibility { hidden, .. },
                            ) = this.admin.community.flow.open.as_mut()
                            {
                                *hidden = ix == 1;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::COMMUNITY_VISIBILITY_NOTE)),
                );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("community-visibility-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.community_visibility_back(window, cx)
                        })),
                )
                .child(
                    Button::new("community-visibility-add")
                        .label(confirm)
                        .primary()
                        .loading(busy)
                        .disabled(busy)
                        .on_click(
                            cx.listener(|this, _, _, cx| this.submit_community_visibility(cx)),
                        ),
                );
            dialog
                .title(crate::ui::shell::dialog_title(tr(
                    keys::COMMUNITY_VISIBILITY_TITLE,
                )))
                .on_ok(move |_, _, cx| {
                    ok_app.update(cx, |this, cx| this.submit_community_visibility(cx));
                    false
                })
                .content(once(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// What the confirmation on screen asks: title, text, confirm label,
    /// whether the confirm is destructive, and whether its request is in
    /// flight.
    fn community_confirm_copy(&self) -> Option<(Option<String>, String, String, bool, bool)> {
        let session = self.session();
        Some(match self.admin.community.flow.open.as_ref()? {
            CommunityBox::ConfirmAdd {
                community_id,
                chat_id,
                ..
            } => (
                Some(tr(keys::COMMUNITY_ADD_TO).to_string()),
                by_kind(
                    self.community_chat_kind(*chat_id),
                    keys::COMMUNITY_ADD_CONFIRM,
                    keys::COMMUNITY_ADD_CONFIRM_CHANNEL,
                    keys::COMMUNITY_ADD_CONFIRM_BOT,
                )
                .to_string(),
                tr(keys::COMMUNITY_ADD_CONFIRM_ADD).to_string(),
                false,
                session.is_some_and(|s| s.community_add_in_flight(*community_id, *chat_id)),
            ),
            CommunityBox::ConfirmRemove { chat_id, .. } => (
                None,
                tr_with(
                    keys::COMMUNITY_REMOVE_SURE,
                    &[("group", &self.community_chat_title(*chat_id))],
                ),
                tr(keys::BOX_REMOVE).to_string(),
                true,
                false,
            ),
            CommunityBox::ConfirmJoin { chat_id, .. } => (
                None,
                tr_with(
                    keys::COMMUNITY_JOIN_SURE,
                    &[("group", &self.community_chat_title(*chat_id))],
                ),
                tr(if self.community_chat_kind(*chat_id) == CommunityChatKind::Channel {
                    keys::PROFILE_JOIN_CHANNEL
                } else {
                    keys::PROFILE_JOIN_GROUP
                })
                .to_string(),
                false,
                false,
            ),
            CommunityBox::ConfirmProcessAll {
                community_id,
                approve,
                count,
            } => (
                Some(
                    tr(if *approve {
                        keys::COMMUNITY_REQUESTS_ADD_ALL_TITLE
                    } else {
                        keys::COMMUNITY_REQUESTS_DECLINE_ALL_TITLE
                    })
                    .to_string(),
                ),
                trn(
                    if *approve {
                        keys::COMMUNITY_REQUESTS_ADD_ALL_SURE
                    } else {
                        keys::COMMUNITY_REQUESTS_DECLINE_ALL_SURE
                    },
                    *count,
                ),
                tr(if *approve {
                    keys::COMMUNITY_REQUEST_ADD
                } else {
                    keys::COMMUNITY_REQUEST_DECLINE
                })
                .to_string(),
                !approve,
                session.is_some_and(|s| s.community_requests_processing(*community_id)),
            ),
            CommunityBox::Ungroup { community_id } => (
                Some(tr(keys::COMMUNITY_UNGROUP_TITLE).to_string()),
                tr(keys::COMMUNITY_UNGROUP_TEXT).to_string(),
                tr(keys::COMMUNITY_UNGROUP).to_string(),
                true,
                session.is_some_and(|s| s.community_collapse_in_flight(*community_id)),
            ),
            CommunityBox::BanWarning {
                community_id,
                member,
                name,
                owned_chat_ids,
            } => (
                Some(tr(keys::COMMUNITY_BAN_WARNING_TITLE).to_string()),
                quill::lang::trn_with(
                    keys::COMMUNITY_BAN_WARNING,
                    owned_chat_ids.len(),
                    &[("user", name)],
                ),
                tr(keys::COMMUNITY_BAN_BUTTON).to_string(),
                true,
                session.is_some_and(|s| {
                    s.community_request_in_flight(*community_id, |purpose| {
                        matches!(purpose, quill::state::CommunitiesPurpose::ToggleMemberBanned { member: m, .. } if m == *member)
                    })
                }),
            ),
            _ => return None,
        })
    }

    /// The confirm button or Enter; nothing while the box's request is
    /// in flight (the button shows it loading).
    pub(super) fn confirm_community_box(&mut self, cx: &mut Context<Self>) {
        if self.community_confirm_copy().is_some_and(|(.., busy)| busy) {
            return;
        }
        match self.admin.community.flow.open {
            Some(CommunityBox::ConfirmAdd {
                community_id,
                chat_id,
                hidden,
            }) => self.send_add_community_chat(community_id, chat_id, hidden, cx),
            Some(CommunityBox::ConfirmRemove { .. }) => self.confirm_remove_from_community(cx),
            Some(CommunityBox::ConfirmJoin { .. }) => self.confirm_join_community_chat(cx),
            Some(CommunityBox::ConfirmProcessAll { .. }) => self.confirm_process_all_requests(cx),
            Some(CommunityBox::Ungroup { community_id }) => {
                self.admin.community.flow.open = None;
                self.set_community_collapsed(community_id, false, cx);
            }
            Some(CommunityBox::BanWarning { .. }) => self.confirm_community_ban(cx),
            _ => {}
        }
    }

    fn cancel_community_box(&mut self, cx: &mut Context<Self>) {
        match self.admin.community.flow.open {
            Some(CommunityBox::ConfirmProcessAll { .. }) => self.cancel_process_all_requests(cx),
            _ => self.flush_and_close_community_box(cx),
        }
    }

    fn build_community_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CommunityConfirm, |this, _, cx| {
                this.cancel_community_box(cx)
            });
        let ok_app = app.clone();
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(BOX_WIDTH));
            let Some((title, text, confirm, danger, busy)) = this.community_confirm_copy() else {
                return dialog.on_close(on_close);
            };
            let owned = match &this.admin.community.flow.open {
                Some(CommunityBox::BanWarning { owned_chat_ids, .. }) => owned_chat_ids.clone(),
                _ => Vec::new(),
            };
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(text));
            for chat_id in owned {
                let title = this.community_chat_title(chat_id);
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(this.community_chat_avatar(chat_id, &title, 32.))
                        .child(div().text_sm().truncate().child(title)),
                );
            }
            let confirm_button = Button::new("community-confirm-ok")
                .label(confirm)
                .loading(busy)
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| this.confirm_community_box(cx)));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("community-confirm-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_community_box(cx))),
                )
                .child(if danger {
                    confirm_button.danger()
                } else {
                    confirm_button.primary()
                });
            dialog
                .when_some(title, |dialog, title| {
                    dialog.title(crate::ui::shell::dialog_title(title))
                })
                .on_ok(move |_, _, cx| {
                    ok_app.update(cx, |this, cx| this.confirm_community_box(cx));
                    false
                })
                .content(once(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "Community": a chat picks the community to join.
    CommunityPick => DialogSpec::new(
        5411,
        |app| matches!(app.admin.community.flow.open, Some(CommunityBox::Pick { .. })),
        QuillApp::build_community_pick_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
    /// "Chat visibility": Visible or Hidden, chosen once.
    CommunityVisibility => DialogSpec::new(
        5412,
        |app| matches!(
            app.admin.community.flow.open,
            Some(CommunityBox::Visibility { .. } | CommunityBox::CreateVisibility { .. })
        ),
        QuillApp::build_community_visibility_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
    /// The community confirmations (add request, remove, Add All /
    /// Decline All, ungroup, ban warning).
    CommunityConfirm => DialogSpec::new(
        5414,
        |app| app
            .admin
            .community
            .flow
            .open
            .as_ref()
            .is_some_and(|open| open.kind() == DialogKind::CommunityConfirm),
        QuillApp::build_community_confirm_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
}

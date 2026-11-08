//! The forward box: a modal chat chooser for the selected messages.
//!
//! Mirrors tdesktop's `ShowForwardMessagesBox` (window/window_peer_menu.cpp):
//! a `PeerListBox` over the window with a search field, Saved Messages first,
//! one row per chat (userpic, name, status line), a check on the userpic of
//! every chosen chat, a comment field that appears once a chat is chosen,
//! and Send / Cancel. The send menu (tdesktop opens it with a right click on
//! Send) holds the forward options, send without sound, schedule, open chat
//! and copy link. See `docs/decisions/forward-box.md`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::{chat_avatar, saved_messages_avatar};
use super::pressable::PressableDiv;
use super::scheduled::ScheduleTarget;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::schedule::ScheduleKind;
use quill::share_box::ShareSend;
use std::path::PathBuf;
use std::time::Duration;

/// tdesktop `boxWideWidth`.
const BOX_WIDTH: f32 = 420.;
/// One row: a 40 px userpic with 6 px above and below (tdesktop
/// `peerListBox` rows are 58 px with a 46 px photo; Quill's denser type
/// scale keeps the same proportions).
const ROW_HEIGHT: f32 = 52.;
const USERPIC: f32 = 40.;
const CHECK: f32 = 20.;
/// At most this many rows show before the list scrolls.
const VISIBLE_ROWS: f32 = 7.;
/// The fewest rows the list keeps in a short window.
const MIN_ROWS: f32 = 3.;
/// Room the title, search, comment and footer take around the list.
const CHROME_HEIGHT: f32 = 330.;
const CHECK_POP: Duration = Duration::from_millis(160);

/// One chat a chooser offers, already resolved for drawing.
pub(super) struct DestRow {
    pub(super) id: ChatId,
    title: String,
    saved: bool,
    photo: Option<PathBuf>,
    status: Option<(String, bool)>,
    checked: bool,
}

/// What a click on a chooser row does.
#[derive(Clone, Copy)]
pub(super) enum RowPick {
    /// Tick or untick the chat (forward box).
    Toggle,
    /// Take the reply to this chat ("Reply in Another Chat").
    Reply,
}

/// The list height for a window `viewport` tall: up to [`VISIBLE_ROWS`],
/// never fewer than [`MIN_ROWS`].
pub(super) fn list_height(viewport: Pixels) -> Pixels {
    let room = (f32::from(viewport) - CHROME_HEIGHT).max(MIN_ROWS * ROW_HEIGHT);
    px(room.min(VISIBLE_ROWS * ROW_HEIGHT))
}

/// The userpic, with the accent check of a chosen chat on its corner.
fn userpic_with_check(row: &DestRow, cx: &App) -> Div {
    let theme = cx.theme();
    let (primary, on_primary, ring) = (theme.primary, theme.primary_foreground, theme.background);
    let reduce_motion = cx.reduce_motion();
    let pic = if row.saved {
        saved_messages_avatar(USERPIC)
    } else {
        chat_avatar(&row.title, row.photo.as_deref(), USERPIC).into_any_element()
    };
    let id = row.id.0 as u64;
    div()
        .relative()
        .flex_none()
        .size(px(USERPIC))
        .child(pic)
        .when(row.checked, |this| {
            let check = div()
                .absolute()
                .right(px(-3.))
                .bottom(px(-3.))
                .size(px(CHECK))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_2()
                .border_color(ring)
                .bg(primary)
                .child(
                    Icon::new(IconName::Check)
                        .size(px(11.))
                        .text_color(on_primary),
                );
            if reduce_motion {
                return this.child(check);
            }
            this.child(check.with_animation(
                ("forward-check-pop", id),
                Animation::new(CHECK_POP).with_easing(ease_out_quint()),
                |check, t| check.opacity(t).size(px(CHECK * (0.55 + 0.45 * t))),
            ))
        })
}

/// One chat of a chooser: userpic, name and status line.
pub(super) fn chooser_row(
    row: DestRow,
    pick: RowPick,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let theme = cx.theme();
    let (muted, primary) = (theme.muted_foreground, theme.primary);
    let selected_fill = theme.primary.opacity(0.10);
    let id = row.id;
    let shown_title = if row.saved {
        tr(keys::SAVED_MESSAGES).to_string()
    } else {
        row.title.clone()
    };
    let label = match pick {
        RowPick::Toggle => format!(
            "{shown_title}, {}",
            if row.checked {
                tr(keys::Q_OPS_SELECTED)
            } else {
                tr(keys::Q_OPS_NOT_SELECTED)
            }
        ),
        RowPick::Reply => tr_with(keys::FORUM_REPLY_IN, &[("topic", &shown_title)]),
    };
    let pic = userpic_with_check(&row, cx);
    // Name and status sit in rows of their own and keep their natural
    // width, so a right-to-left name starts next to the userpic instead of
    // resting against the far edge of the list (`bidi_line::one_line`).
    let name =
        div()
            .flex()
            .min_w_0()
            .child(div().min_w_0().truncate().text_sm().font_medium().child(
                super::emoji_sprites::one_line(&shown_title, px(14.)).unwrap_or_else(|| {
                    super::bidi_line::one_line_plain(shown_title.clone()).into_any_element()
                }),
            ));
    let status = row.status.map(|(line, online)| {
        div().flex().min_w_0().child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(if online { primary } else { muted })
                .child(super::bidi_line::one_line_plain(line)),
        )
    });
    div()
        .id((
            match pick {
                RowPick::Toggle => "forward-dest",
                RowPick::Reply => "reply-dest",
            },
            id.0 as u64,
        ))
        .flex()
        .flex_none()
        .items_center()
        .gap_3()
        .h(px(ROW_HEIGHT))
        .px_2()
        .rounded_md()
        .role(gpui_kit::Role::Button)
        .a11y_tag(quill::agent::ui_driver::redact::chat_tag(id.0))
        .aria_label(label)
        .tab_index(0)
        .when(row.checked, |this| this.bg(selected_fill))
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, window, cx| match pick {
            RowPick::Toggle => this.toggle_share_destination(id, cx),
            RowPick::Reply => {
                this.choose_reply_chat(id, window, cx);
                this.close_kit_dialog_if_done(DialogKind::ReplyElsewhere, window, cx);
            }
        }))
        .child(pic)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(name)
                .children(status),
        )
}

impl QuillApp {
    /// The chooser's search field with its magnifier.
    pub(super) fn chooser_search(&self, label: &'static str, cx: &App) -> Div {
        let muted = cx.theme().muted_foreground;
        div()
            .relative()
            .flex()
            .items_center()
            .child(
                Textarea::new(&self.share.search_input)
                    .aria_label(label)
                    .w_full()
                    .pl(px(30.))
                    .h(px(36.)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(Icon::new(IconName::Search).size(px(14.)).text_color(muted)),
            )
    }

    /// The chats a chooser lists for the typed query, ready to draw.
    pub(super) fn chooser_rows(&self, query: &str) -> Vec<DestRow> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let roots = self.media_display_roots();
        session
            .share_destinations(query)
            .into_iter()
            .map(|chat| DestRow {
                id: chat.id,
                title: chat.title.clone(),
                saved: session.is_saved_messages(chat.id),
                photo: session
                    .chat_photo_path(chat.id)
                    .and_then(|path| quill::local_path::sandboxed_display_path(path, &roots)),
                status: session.share_row_status(chat),
                checked: self.share.selection.contains(chat.id),
            })
            .collect()
    }

    /// The search field with its magnifier, the list, and the comment
    /// field once a chat is chosen.
    fn forward_box_body(&self, viewport: Pixels, cx: &mut Context<Self>) -> AnyElement {
        if self
            .chat_ui
            .schedule_picker
            .as_ref()
            .is_some_and(|picker| picker.target == ScheduleTarget::Share)
        {
            // Schedule is a step of the box, not a second dialog over it.
            return self.schedule_popup(cx);
        }
        let query = self.share.search_input.read(cx).value().to_string();
        let rows = self.chooser_rows(&query);
        let searching = self
            .session()
            .is_some_and(|s| s.messages.share_search.is_searching());
        let muted = cx.theme().muted_foreground;
        let search = self.chooser_search(tr(keys::Q_OPS_SEARCH_FORWARDING_DESTINATIONS), cx);
        let mut list = div()
            .id("forward-dest-list")
            .flex()
            .flex_col()
            .h(list_height(viewport))
            .overflow_y_scroll();
        if rows.is_empty() {
            list = list.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(muted)
                    .child(if searching {
                        tr(keys::Q_OPS_SEARCHING)
                    } else {
                        tr(keys::Q_OPS_NO_MATCHING_CHATS)
                    }),
            );
        } else {
            for row in rows {
                list = list.child(chooser_row(row, RowPick::Toggle, cx));
            }
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(search)
            .child(list)
            .when(!self.share.selection.is_empty(), |this| {
                this.child(
                    Textarea::new(&self.share.comment_input)
                        .aria_label(tr(keys::Q_OPS_COMMENT))
                        .w_full(),
                )
            })
            .into_any_element()
    }

    /// Who is chosen, the send menu, Cancel and Send.
    fn forward_box_footer(&self, owner: WeakEntity<Self>, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.share.selection.len();
        let summary = match selected {
            0 => tr(keys::Q_OPS_CHOOSE_ONE_OR_MORE_CHATS).to_string(),
            1 => tr(keys::Q_OPS_1_CHAT_SELECTED).to_string(),
            n => trn(keys::Q_OPS_CHATS_SELECTED, n),
        };
        let menu = self.forward_box_menu(owner);
        div()
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(summary),
            )
            .child(menu)
            .child(
                Button::new("forward-box-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_forward_picker(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::ForwardBox, window, cx);
                    })),
            )
            .child(
                Button::new("share-send")
                    .label(tr(keys::SHARE_CONFIRM))
                    .primary()
                    .disabled(selected == 0)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_share(ShareSend::Normal, cx);
                        this.close_kit_dialog_if_done(DialogKind::ForwardBox, window, cx);
                    })),
            )
            .into_any_element()
    }

    /// tdesktop's send menu on the Send button: forward options, then the
    /// send variants, then the box's other ways out.
    fn forward_box_menu(&self, owner: WeakEntity<Self>) -> impl IntoElement {
        let draft = self.forward_draft();
        let multiple = draft.as_ref().is_some_and(|d| d.count() > 1);
        let send_copy = self
            .share
            .pending_forward
            .as_ref()
            .is_some_and(|d| d.send_copy);
        let remove_caption = self
            .share
            .pending_forward
            .as_ref()
            .is_some_and(|d| d.remove_caption);
        let selected = self.share.selection.len();
        let single_saved = self
            .share
            .selection
            .single()
            .and_then(|id| self.session().map(|s| s.is_saved_messages(id)))
            .unwrap_or(false);
        let schedule_label = match ScheduleKind::for_saved_messages(single_saved) {
            ScheduleKind::Reminder => tr(keys::Q_OPS_REMIND_ME),
            ScheduleKind::Schedule => tr(keys::Q_OPS_SCHEDULE),
        };
        let can_link = draft.as_ref().is_some_and(|d| d.count() == 1) && self.live.is_some();
        let hide_names = if multiple {
            tr(keys::Q_OPS_HIDE_SENDER_NAMES)
        } else {
            tr(keys::Q_OPS_HIDE_SENDER_NAME)
        };
        let hide_captions = if multiple {
            tr(keys::Q_OPS_HIDE_CAPTIONS)
        } else {
            tr(keys::Q_OPS_HIDE_CAPTION)
        };
        Button::new("forward-box-more")
            .icon(IconName::EllipsisVertical)
            .ghost()
            .tooltip(tr(keys::Q_MEDIA_MORE))
            .accessibility_label(tr(keys::Q_MEDIA_MORE))
            .dropdown_menu(move |menu, _, _| {
                let act = |owner: &WeakEntity<QuillApp>,
                           run: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
                    let owner = owner.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        let _ = owner.update(cx, |this, cx| run(this, window, cx));
                    }
                };
                let mut menu = menu
                    .item(
                        PopupMenuItem::new(hide_names)
                            .checked(send_copy)
                            .on_click(act(&owner, |this, _, cx| {
                                this.set_forward_send_copy(
                                    !this
                                        .share
                                        .pending_forward
                                        .as_ref()
                                        .is_some_and(|d| d.send_copy),
                                    cx,
                                );
                            })),
                    )
                    .item(
                        PopupMenuItem::new(hide_captions)
                            .checked(remove_caption)
                            .disabled(!send_copy)
                            .on_click(act(&owner, |this, _, cx| {
                                this.toggle_forward_remove_caption(cx);
                            })),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new(tr(keys::Q_OPS_SEND_WITHOUT_SOUND))
                            .disabled(selected == 0)
                            .on_click(act(&owner, |this, window, cx| {
                                this.submit_share(ShareSend::Silent, cx);
                                this.close_kit_dialog_if_done(DialogKind::ForwardBox, window, cx);
                            })),
                    )
                    .item(
                        PopupMenuItem::new(schedule_label)
                            .disabled(selected == 0)
                            .on_click(act(&owner, |this, window, cx| {
                                this.open_schedule_picker(ScheduleTarget::Share, window, cx);
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr(keys::Q_OPS_OPEN_CHAT))
                            .disabled(selected != 1)
                            .on_click(act(&owner, |this, window, cx| {
                                this.share_continue_in_chat(window, cx);
                                this.close_kit_dialog_if_done(DialogKind::ForwardBox, window, cx);
                            })),
                    );
                if can_link {
                    menu = menu.separator().item(
                        PopupMenuItem::new(tr(keys::CONTEXT_COPY_LINK)).on_click(act(
                            &owner,
                            |this, _, cx| {
                                this.share_copy_link(cx);
                            },
                        )),
                    );
                }
                menu
            })
    }

    pub(super) fn build_forward_box_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ForwardBox, |this, window, cx| {
                this.close_forward_picker(window, cx);
            });
        let owner = app.downgrade();
        let scheduling = app
            .read(cx)
            .chat_ui
            .schedule_picker
            .as_ref()
            .is_some_and(|picker| picker.target == ScheduleTarget::Share);
        let footer = (!scheduling)
            .then(|| app.update(cx, |this, cx| this.forward_box_footer(owner.clone(), cx)));
        let body_app = app.clone();
        let dialog = dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(tr(keys::Q_OPS_FORWARD_TO)))
            .content(move |content, window, cx| {
                let viewport = window.viewport_size().height;
                let body = body_app.update(cx, |this, cx| {
                    if std::mem::take(&mut this.share.focus_search) {
                        let search = this.share.search_input.clone();
                        window.defer(cx, move |window, cx| {
                            search.update(cx, |input, cx| input.focus(window, cx));
                        });
                    }
                    this.forward_box_body(viewport, cx)
                });
                content.child(body)
            });
        let dialog = match footer {
            Some(footer) => dialog.footer(footer),
            None => dialog,
        };
        dialog.on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    /// The forward box over the window (tdesktop `ShowForwardMessagesBox`).
    ForwardBox => DialogSpec::new(
        2930,
        |app| app.share.forward_picker_open && app.forward_draft().is_some(),
        QuillApp::build_forward_box_dialog,
    )
    .close(|app, window, cx| app.close_forward_picker(window, cx)),
}

#[cfg(test)]
mod tests {
    use super::{MIN_ROWS, ROW_HEIGHT, VISIBLE_ROWS, list_height};
    use gpui_kit::px;

    #[test]
    fn the_list_shows_seven_rows_and_keeps_three_in_a_short_window() {
        assert_eq!(list_height(px(1000.)), px(VISIBLE_ROWS * ROW_HEIGHT));
        assert_eq!(list_height(px(300.)), px(MIN_ROWS * ROW_HEIGHT));
    }
}

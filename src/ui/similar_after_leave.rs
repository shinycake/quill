//! "Similar channels" offered after you leave a channel (Telegram Desktop
//! `info/similar_peers` list, which tdesktop also puts under the join
//! message; Quill shows it as a box right after the leave, so nothing is
//! drawn inside the message history). The data is the cached
//! `getChatSimilarChats` list the channel profile already uses
//! ([`ProfileChatsKind::SimilarChats`]); this module only decides when to
//! offer it and renders it.

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::state::{ProfileChatsFetch, ProfileChatsKind, Session};
use std::cell::RefCell;
use std::rc::Rc;

/// The channel that was just left; the box opens once its similar list
/// has loaded and has something to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SimilarAfterLeave {
    pub(crate) chat_id: ChatId,
    pub(crate) title: String,
}

/// The ids to offer: the left channel itself and repeats are dropped,
/// and only chats the session knows about (so the row has a title).
pub(crate) fn suggestions(left: ChatId, ids: &[i64], known: impl Fn(i64) -> bool) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for &id in ids {
        if id != left.0 && known(id) && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Suggestions are offered for channels only (`getChatSimilarChats` has
/// nothing for groups).
fn offers_similar(session: &Session, chat_id: ChatId) -> bool {
    session
        .chats
        .get(&chat_id.0)
        .is_some_and(|c| c.is_channel())
}

impl QuillApp {
    /// Call after a successful `leaveChat` request: remember the channel
    /// and make sure the similar list is requested (the driver dedupes).
    pub(in crate::ui) fn note_left_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        if !offers_similar(session, chat_id) {
            return;
        }
        let title = session
            .chats
            .get(&chat_id.0)
            .map(|c| c.title.clone())
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_similar_chats(chat_id);
        }
        self.dialogs.similar_after_leave = Some(SimilarAfterLeave { chat_id, title });
        cx.notify();
    }

    fn pending_similar_ids(&self) -> Vec<i64> {
        let (Some(pending), Some(session)) = (&self.dialogs.similar_after_leave, self.session())
        else {
            return Vec::new();
        };
        suggestions(
            pending.chat_id,
            session.profile_chat_list(ProfileChatsKind::SimilarChats, pending.chat_id.0),
            |id| session.chats.contains_key(&id),
        )
    }

    fn close_similar_after_leave(&mut self, cx: &mut Context<Self>) {
        self.dialogs.similar_after_leave = None;
        cx.notify();
    }

    fn build_similar_after_leave_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SimilarAfterLeave, |this, _, cx| {
                this.close_similar_after_leave(cx);
            });
        app.update(cx, |this, cx| {
            let Some(pending) = this.dialogs.similar_after_leave.clone() else {
                return dialog.on_close(on_close);
            };
            let ids = this.pending_similar_ids();
            let roots = this.media_display_roots();
            let muted = cx.theme().muted_foreground;
            let mut rows: Vec<AnyElement> = Vec::new();
            for id in ids {
                let Some(session) = this.session() else { break };
                let Some(chat) = session.chats.get(&id) else {
                    continue;
                };
                let title = chat.title.clone();
                let photo = session
                    .chat_photo_path(ChatId(id))
                    .and_then(|path| sandboxed_display_path(path, &roots));
                rows.push(
                    div()
                        .id(("similar-after-leave", id.unsigned_abs()))
                        .flex()
                        .items_center()
                        .gap_3()
                        .w_full()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .aria_label(title.clone())
                        .tab_index(0)
                        .hover(|style| style.bg(cx.theme().secondary))
                        .child(chat_avatar(&title, photo.as_deref(), 36.))
                        .child(
                            div()
                                .text_sm()
                                .truncate()
                                .child(super::bidi_line::one_line_plain(title)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.close_similar_after_leave(cx);
                            this.select_listed_chat(ChatId(id), window, cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::SimilarAfterLeave,
                                window,
                                cx,
                            );
                        }))
                        .into_any_element(),
                );
            }
            let heading = if pending.title.is_empty() {
                "You left the channel.".to_string()
            } else {
                format!("You left {}.", pending.title)
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().text_color(muted).child(heading))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .px_2()
                                .text_xs()
                                .font_semibold()
                                .text_color(muted)
                                .child("Similar channels"),
                        )
                        .children(rows),
                )
                .child(
                    div().flex().justify_end().child(
                        Button::new("similar-after-leave-close")
                            .label("Close")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_similar_after_leave(cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::SimilarAfterLeave,
                                    window,
                                    cx,
                                );
                            })),
                    ),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Similar channels"))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Similar channels offered right after leaving a channel.
    SimilarAfterLeave => DialogSpec::new(
        7460,
        |app| !app.pending_similar_ids().is_empty(),
        QuillApp::build_similar_after_leave_dialog,
    ),
}

crate::ui::screenshot_demo::register_demos![
    // Fixture/proof surface: a left channel with a loaded similar list.
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-similar-after-leave",
        "screenshot demo — similar channels after leaving"
    )
    .setup(QuillApp::demo_similar_after_leave),
];

impl QuillApp {
    fn demo_similar_after_leave(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let left = ChatId(13);
        let ids: Vec<i64> = session
            .chats
            .keys()
            .copied()
            .filter(|id| *id != left.0)
            .take(4)
            .collect();
        session.users_state.profile_chat_lists.insert(
            (ProfileChatsKind::SimilarChats, left.0),
            ProfileChatsFetch::Loaded(ids),
        );
        self.note_left_channel(left, cx);
        self.connection.status_note = "screenshot demo — similar channels".into();
    }
}

#[cfg(test)]
mod tests {
    use super::suggestions;
    use quill::ids::ChatId;

    #[test]
    fn drops_the_left_channel_repeats_and_unknown_chats() {
        let out = suggestions(ChatId(-5), &[-5, -6, -6, -7, -8], |id| id != -7);
        assert_eq!(out, vec![-6, -8]);
    }

    #[test]
    fn empty_list_offers_nothing() {
        assert!(suggestions(ChatId(-5), &[], |_| true).is_empty());
    }
}

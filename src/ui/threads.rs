//! Channel comments and group reply threads (tdesktop `RepliesWidget`):
//! the comments bar under a channel post, the replies link under a group
//! message, and the thread view's chrome — header actions, the pinned root
//! post bar, the loading / error pane and navigation.

use super::app::QuillApp;
use super::message_text::kit_avatar_element;
use super::nested_click::SwallowPress;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::state::{
    Session, ThreadStatus, comments_bar_label, effective_preview, replies_link_label,
};
use quill::telegram::envelope::{MessageReplyInfo, MessageSender};
use std::path::PathBuf;

/// Which wording and icon the bar under a message uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ReplyBarKind {
    /// A channel post with a discussion group ("N comments").
    Comments,
    /// A group message with replies ("N replies").
    Replies,
}

/// Size of the commenter avatars in the bar.
const BAR_AVATAR: f32 = 20.;

/// The bar at the bottom of a bubble that opens the thread: commenter
/// avatars (or a comment icon), "N comments" / "Leave a comment" /
/// "N replies", an unread dot and a chevron (tdesktop
/// `Message::paintCommentsButton`).
#[allow(clippy::too_many_arguments)]
pub(super) fn reply_bar(
    chat_id: ChatId,
    message_id: MessageId,
    kind: ReplyBarKind,
    info: &MessageReplyInfo,
    outgoing: bool,
    avatars: Vec<(String, Option<PathBuf>)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let label = match kind {
        ReplyBarKind::Comments => comments_bar_label(info.reply_count),
        ReplyBarKind::Replies => replies_link_label(info.reply_count),
    };
    // Incoming bubbles take the link color; outgoing ones sit on the
    // accent fill.
    let fg: Hsla = if outgoing {
        text_on_fill().into()
    } else {
        accent().into()
    };
    let unread = info.has_unread();
    div()
        .id(("reply-bar", message_id.0 as u64))
        .flex()
        .items_center()
        .gap_2()
        .h(px(32.))
        .mt_1()
        .pt_1()
        .border_t_1()
        .border_color(fg.opacity(0.28))
        .text_sm()
        .text_color(fg)
        .cursor_pointer()
        .role(gpui_kit::Role::Button)
        .aria_label(label.clone())
        .tab_index(0)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_thread_view(chat_id, message_id, window, cx);
        }))
        .child(if avatars.is_empty() {
            Icon::new(match kind {
                ReplyBarKind::Comments => gpui_kit::assets::IconName::MessageSquare,
                ReplyBarKind::Replies => gpui_kit::assets::IconName::Reply,
            })
            .size(px(16.))
            .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .children(avatars.iter().enumerate().map(|(ix, (name, photo))| {
                    div()
                        .when(ix > 0, |this| this.ml(px(-6.)))
                        .rounded_full()
                        .border_1()
                        .border_color(if outgoing {
                            accent_strong()
                        } else {
                            bg_bubble_incoming()
                        })
                        .child(kit_avatar_element(name, photo.as_deref(), px(BAR_AVATAR)))
                }))
                .into_any_element()
        })
        .child(div().font_semibold().child(label))
        .when(unread, |this| {
            this.child(div().size(px(7.)).rounded_full().bg(fg))
        })
        .child(div().flex_1())
        .child(Icon::new(gpui_kit::assets::IconName::ChevronRight).size(px(16.)))
        .into_any_element()
}

impl QuillApp {
    /// Open the comment / reply thread of a message: TDLib resolves it
    /// (`getMessageThread`), then the view moves to the thread's chat.
    pub(super) fn open_thread_view(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && live.driver.open_thread(chat_id, message_id).is_err()
        {
            self.status_note = "could not open comments".into();
        }
        cx.notify();
    }

    /// Whether a thread is opening (resolving, switching chat or failed)
    /// without its messages on screen yet.
    pub(super) fn thread_pending(&self) -> bool {
        self.session().is_some_and(Session::thread_unavailable)
    }

    /// Whether the open chat shows a loaded thread.
    pub(super) fn thread_active(&self) -> bool {
        self.session()
            .is_some_and(|s| s.open_chat.and_then(|c| s.thread_for_chat(c)).is_some())
    }

    /// Per-frame thread bookkeeping, run from `render` (it needs the
    /// window): move to the thread's chat once TDLib resolved it, request
    /// the first page, and keep paging while "jump to root" is pending.
    pub(super) fn advance_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if live.driver.thread_needs_start() {
            let switch_to = live
                .driver
                .session
                .thread
                .as_ref()
                .filter(|thread| thread.needs_chat_switch)
                .map(|thread| thread.chat_id);
            match switch_to {
                Some(chat_id) => self.select_chat_with(chat_id, true, window, cx),
                None => {
                    let _ = live.driver.start_thread_in_open_chat();
                    cx.notify();
                }
            }
            return;
        }
        if self.thread_root_jump {
            let complete = live
                .driver
                .session
                .thread
                .as_ref()
                .is_none_or(|thread| thread.history.loaded_complete);
            if complete {
                self.thread_root_jump = false;
                self.history_scroller.update(cx, |state, cx| {
                    state.scroll_to_item(0, cx);
                });
            } else if live.driver.fetch_thread_history().is_ok() {
                cx.notify();
            }
        }
    }

    /// Back to where the thread was opened from.
    pub(super) fn leave_thread_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            if let Some(session) = self.demo_session.as_mut() {
                session.close_thread();
            }
            cx.notify();
            return;
        };
        let origin = live
            .driver
            .session
            .thread
            .as_ref()
            .map(|thread| thread.origin_message_id);
        let back = live.driver.close_thread();
        self.thread_root_jump = false;
        if let Some(chat) = back {
            self.select_listed_chat(chat, window, cx);
        }
        // Land on the post the thread was opened from.
        if let Some(message_id) = origin {
            self.jump_to_replied_message(message_id, cx);
        }
        cx.notify();
    }

    /// "View in chat": leave the thread and show its root in the
    /// discussion group's own history.
    pub(super) fn view_thread_in_chat(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let root = live
            .driver
            .session
            .thread
            .as_ref()
            .map(|thread| MessageId(thread.thread_id));
        live.driver.session.close_thread();
        self.thread_root_jump = false;
        if let Some(root) = root {
            self.jump_to_replied_message(root, cx);
        }
        cx.notify();
    }

    /// Scroll to the thread's root post, loading older replies first when
    /// they are not all loaded yet.
    pub(super) fn jump_to_thread_root(&mut self, cx: &mut Context<Self>) {
        let complete = self
            .session()
            .and_then(|s| s.thread.as_ref())
            .is_none_or(|thread| thread.history.loaded_complete);
        if complete {
            self.history_scroller.update(cx, |state, cx| {
                state.scroll_to_item(0, cx);
            });
        } else {
            self.thread_root_jump = true;
        }
        cx.notify();
    }

    /// The thread header's "back" button.
    pub(super) fn thread_back_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("thread-back")
            .icon(gpui_kit::assets::IconName::ChevronLeft)
            .ghost()
            .tooltip("Back")
            .accessibility_label("Back")
            .on_click(cx.listener(|this, _, window, cx| {
                this.leave_thread_view(window, cx);
            }))
    }

    /// The pinned bar under the header: the thread's root post. A click
    /// scrolls to it.
    pub(super) fn thread_root_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let thread = session.thread_for_chat(session.open_chat?)?;
        let root = thread.root_message()?;
        let title = if thread.is_comments() {
            "Original post"
        } else {
            "Original message"
        };
        let preview = effective_preview(root);
        Some(
            div()
                .id("thread-root-bar")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .cursor_pointer()
                .role(gpui_kit::Role::Button)
                .aria_label("Go to the original post")
                .tab_index(0)
                .on_click(cx.listener(|this, _, _, cx| this.jump_to_thread_root(cx)))
                .child(div().w(px(2.)).h(px(32.)).rounded_full().bg(accent()))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(
                            div()
                                .text_xs()
                                .font_medium()
                                .text_color(accent())
                                .child(title),
                        )
                        .child(div().text_sm().truncate().text_color(text_primary()).child(
                            super::bidi_line::one_line_plain(super::search_ui::one_line_preview(
                                &preview,
                            )),
                        )),
                )
                .child(div().flex_1())
                .child(
                    Button::new("thread-info-toggle")
                        .icon(gpui_kit::assets::IconName::Info)
                        .ghost()
                        .tooltip("Thread info")
                        .accessibility_label("Thread info")
                        // Keep the root bar's own click (jump to the original) out of it.
                        .swallow_press()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.thread_info_open = !this.thread_info_open;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// The pane shown while a thread is opening or failed to open.
    pub(super) fn thread_status_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let status = self
            .session()
            .and_then(|s| s.thread.as_ref())
            .map(|thread| thread.status.clone());
        let failed = match &status {
            Some(ThreadStatus::Failed(message)) => Some(message.clone()),
            _ => None,
        };
        div()
            .id("thread-status-pane")
            .flex()
            .flex_col()
            .flex_1()
            .p_6()
            .gap_3()
            .child(div().font_semibold().child(match &failed {
                Some(_) => "Could not open the thread",
                None => "Opening the thread…",
            }))
            .when_some(failed.clone(), |this, message| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(message),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("thread-pane-back")
                            .label("Back")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.leave_thread_view(window, cx);
                            })),
                    )
                    .when(failed.is_some(), |this| {
                        this.child(Button::new("thread-pane-retry").label("Retry").on_click(
                            cx.listener(|this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.retry_thread();
                                }
                                cx.notify();
                            }),
                        ))
                    }),
            )
            .into_any_element()
    }
}

/// Avatars of a bar's recent repliers: name and sandboxed photo.
pub(super) fn replier_avatars(
    info: &MessageReplyInfo,
    session: Option<&Session>,
    media_roots: &[PathBuf],
) -> Vec<(String, Option<PathBuf>)> {
    let Some(session) = session else {
        return Vec::new();
    };
    info.recent_repliers
        .iter()
        .take(3)
        .map(|sender| {
            let (name, photo) = match sender {
                MessageSender::User { user_id } => (
                    session
                        .user(*user_id)
                        .map(|u| u.display_name())
                        .unwrap_or_default(),
                    session.user_photo_path(*user_id),
                ),
                MessageSender::Chat { chat_id } => (
                    session
                        .chats
                        .get(chat_id)
                        .map(|c| c.title.clone())
                        .unwrap_or_default(),
                    session.chat_photo_path(ChatId(*chat_id)),
                ),
            };
            (
                name,
                photo.and_then(|path| sandboxed_display_path(path, media_roots)),
            )
        })
        .collect()
}

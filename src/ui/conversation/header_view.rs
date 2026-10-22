//! The conversation header's elements: the identity block and the action
//! buttons. `conversation_header` decides what they show; these build it,
//! each in its own function, so no single frame holds every builder step
//! in a debug build (docs/decisions/main-thread-stack.md).

use super::super::pressable::PressableDiv;
use super::*;

/// What the header's identity block shows.
pub(super) struct HeaderIdentity {
    pub(super) title: String,
    pub(super) verify_icon: Option<PathBuf>,
    pub(super) badges: Vec<quill::peer_badge::TitleBadge>,
    /// The chat whose emoji status the badges draw.
    pub(super) status_owner: i64,
    pub(super) status_line: Option<String>,
    pub(super) status_accent: bool,
    pub(super) status_indicator: Option<quill::state::ActivityIndicator>,
    /// The info panel a click on the block opens.
    pub(super) info_target: Option<InfoPanelTarget>,
}

impl QuillApp {
    /// Avatar-less identity: the title with its badges over one status
    /// line; a click opens the chat's info when it has one.
    #[inline(never)]
    pub(super) fn conversation_identity(
        &self,
        identity: HeaderIdentity,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let HeaderIdentity {
            title,
            verify_icon,
            badges,
            status_owner,
            status_line,
            status_accent,
            status_indicator,
            info_target,
        } = identity;
        let title_row = div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .when_some(verify_icon, |this, path| {
                this.child(super::super::bot_verify_icon::bot_verify_icon(path))
            })
            .child(
                div()
                    .font_semibold()
                    .truncate()
                    .child(super::super::bidi_line::one_line_plain(title)),
            )
            .children(badges.into_iter().map(|badge| {
                let emoji = match badge {
                    quill::peer_badge::TitleBadge::EmojiStatus(id) => self.status_glyph(
                        super::super::status_emoji::StatusSurface::Header,
                        status_owner,
                        id,
                        cx,
                    ),
                    _ => None,
                };
                super::super::chat_row::title_badge_element(badge, emoji, cx)
            }));
        div()
            .id("conversation-identity")
            .flex()
            .items_center()
            .gap_3()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(title_row)
                    .when_some(status_line, |this, line| {
                        this.child(conversation_status_line(
                            line,
                            status_accent,
                            status_indicator,
                            cx,
                        ))
                    }),
            )
            .when_some(info_target, |this, target| {
                this.role(gpui_kit::Role::Button)
                    .aria_label(tr(keys::Q_CHAT_OPEN_CONVERSATION_INFORMATION))
                    .tab_index(0)
                    .cursor_pointer()
                    .rounded_md()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_info_panel_target(target, window, cx);
                    }))
            })
            .into_any_element()
    }

    /// The buttons at the header's end: View in Chat, call, search, voice
    /// chat, the info column toggle and the chat menu.
    #[inline(never)]
    pub(super) fn conversation_header_actions(
        &self,
        chat_id: ChatId,
        thread_header: Option<&(String, String, bool)>,
        call_user: Option<i64>,
        info_target: Option<InfoPanelTarget>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Phase C3a: voice-chat affordance for groups/channels — join the
        // live voice chat, or start one when none is live. Signaling only
        // (no audio transport yet).
        let (voice_ok, voice_live) = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|c| {
                let kind_ok = matches!(
                    c.kind,
                    ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. }
                );
                (kind_ok, kind_ok && c.video_chat.is_some())
            })
            .unwrap_or((false, false));
        let voice_ok =
            voice_ok && thread_header.is_none() && self.chat_call_button_visible(chat_id);
        let view_in_chat = thread_header.is_some_and(|(_, _, comments)| *comments);
        let chat_search_open = self.chat_search_is_open();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .when(view_in_chat, |this| {
                this.child(thread_view_in_chat_button(cx))
            })
            .when_some(call_user, |this, user_id| {
                this.child(chat_call_button(user_id, cx))
            })
            .child(chat_find_button(chat_search_open, cx))
            .when(voice_ok, |this| {
                this.child(self.chat_voice_chat_button(chat_id, voice_live, cx))
            })
            // Telegram Desktop's third-column toggle: shows or hides the
            // chat's info panel.
            .when_some(info_target, |this, target| {
                let open = self
                    .session()
                    .is_some_and(|s| s.users_state.open_info_panel == Some(target));
                this.child(chat_info_toggle(target, open, cx))
            })
            .child(self.chat_navigation_menu(cx))
            .into_any_element()
    }
}

/// The status line under the title: activity, presence or a note.
#[inline(never)]
fn conversation_status_line(
    line: String,
    accent: bool,
    indicator: Option<quill::state::ActivityIndicator>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .id("conversation-status")
        .flex()
        .items_center()
        .text_xs()
        .text_color(if accent {
            cx.theme().primary
        } else {
            cx.theme().muted_foreground
        })
        .when_some(indicator, |this, indicator| {
            this.child(super::super::activity_indicator::activity_indicator(
                indicator,
                cx.theme().primary,
                "header-activity".into(),
            ))
        })
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(super::super::bidi_line::one_line_plain(line)),
        )
        .into_any_element()
}

#[inline(never)]
fn thread_view_in_chat_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("thread-view-in-chat")
        .label(tr(keys::Q_CHAT_VIEW_IN_CHAT))
        .ghost()
        .tooltip(tr(keys::Q_CHAT_SHOW_THE_POST_IN_THE))
        .accessibility_label(tr(keys::Q_CHAT_VIEW_IN_CHAT))
        .on_click(cx.listener(|this, _, _, cx| {
            this.view_thread_in_chat(cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn chat_call_button(user_id: i64, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("chat-call")
        .icon(IconName::Phone)
        .ghost()
        .tooltip(tr(keys::Q_CHAT_CALL))
        .accessibility_label(tr(keys::Q_CHAT_CALL))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.start_call_for_user(user_id, false, cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn chat_find_button(open: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("chat-find")
        .icon(if open { IconName::X } else { IconName::Search })
        .ghost()
        .tooltip(if open {
            tr(keys::Q_CHAT_CLOSE_SEARCH)
        } else {
            tr(keys::Q_CHAT_SEARCH_THIS_CHAT)
        })
        .accessibility_label(tr(keys::Q_CHAT_SEARCH_THIS_CHAT))
        .on_click(cx.listener(|this, _, window, cx| {
            if this.chat_search_is_open() {
                this.close_chat_search_ui(window, cx);
            } else {
                this.open_chat_search_ui(window, cx);
            }
        }))
        .into_any_element()
}

#[inline(never)]
fn chat_info_toggle(target: InfoPanelTarget, open: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("chat-info-toggle")
        .icon(IconName::PanelRight)
        .ghost()
        .selected(open)
        .tooltip(if open {
            tr(keys::Q_CHAT_HIDE_INFO)
        } else {
            tr(keys::Q_CHAT_SHOW_INFO)
        })
        .accessibility_label(tr(keys::Q_CHAT_TOGGLE_CHAT_INFO))
        .on_click(cx.listener(move |this, _, window, cx| {
            if open {
                this.close_info_panel(cx);
            } else {
                this.open_info_panel_target(target, window, cx);
            }
        }))
        .into_any_element()
}

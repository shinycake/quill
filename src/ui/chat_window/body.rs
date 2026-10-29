//! The chat window's body, built by `QuillApp` with the window's slot in
//! place: the forum topic column when the chat is a forum, the
//! conversation (header, history, compose field) and the overlays that
//! belong to it (message menu, reaction selector, link popups, media
//! viewer, photo editor, Instant View). No chat list (Telegram Desktop's
//! single-chat window: `SeparateId::hasChatsList()` is false for
//! `SeparateType::Chat`; a `Forum` window shows the topic list where the
//! chat list was).

use super::super::app::QuillApp;
use super::super::conversation::ConversationPart;
use gpui_kit::TestSupportExt;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::ForumColumn;

impl QuillApp {
    /// The window's content for `chat_id`. Runs with the window's slot
    /// swapped in (`open_chat` is `chat_id` there), so the shared
    /// conversation, composer and overlay code serves the window.
    pub(super) fn chat_window_body(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_composer_placeholder(window, cx);
        // The main window's frame upkeep syncs only its own composer.
        self.sync_emoji_words(cx);
        if self.account.passcode.locked {
            // Telegram Desktop's passcode lock covers every window
            // (`Application::lockByPasscode` → `setupPasscodeLock` on each):
            // the same box, unlocking everywhere at once.
            self.passcode_frame(window, cx);
            return div()
                .id("chat-window-locked")
                .relative()
                .flex()
                .flex_1()
                .size_full()
                .child(self.lock_overlay(cx))
                .into_any_element();
        }
        // Rows this window painted last frame are what the user saw.
        self.report_visible_history(window.is_window_active(), cx);
        self.report_visible_sponsored(window.is_window_active());
        self.sync_window_userpic_videos(chat_id);
        let effects = self.emoji_effect_overlay(self.effects_window_active(), cx);
        let forum_column = self.forum_column_layout(window);
        let topic_open = self.session().is_some_and(|s| s.open_topic.is_some());
        // A narrow forum window shows the topics until one is picked.
        let topics_only = forum_column == ForumColumn::Replacing && !topic_open;
        let column = (forum_column != ForumColumn::Hidden
            && !(forum_column == ForumColumn::Replacing && topic_open))
            .then(|| {
                self.forum_column_view(
                    Some(chat_id),
                    crate::ui::shell_layout::ForumPane::Column {
                        chats_button: false,
                    },
                    cx,
                )
            });
        let drop_state = self.visible_drop_state(cx);
        let drop_overlay = drop_state.map(|state| self.drop_zone_overlay(state, cx));
        let top = self
            .conversation(ConversationPart::Top, cx)
            .into_any_element();
        let bottom = self
            .conversation(ConversationPart::Bottom, cx)
            .into_any_element();
        let conversation = div()
            .id("chat-window-history")
            .test_support()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            // Files dragged over the chat: the zones say what each one does
            // (`drop_zones`); a drop outside any zone attaches them.
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    let paths = event.drag(cx).paths().to_vec();
                    this.note_file_drag(&paths, cx);
                }),
            )
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.drop_files_plain(paths.paths(), cx);
            }))
            .child(top)
            .child(
                // The pull to the next channel's note covers the composer.
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .child(bottom)
                    .children(self.pull_hint_overlay(cx)),
            )
            .children(drop_overlay)
            .children(effects);
        let viewer_open = self.viewer.state.is_open();
        div()
            .id("chat-window-body")
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .children(column)
            .when(!topics_only, |this| this.child(conversation))
            // MED1: fullscreen media viewer over the window's content.
            .when(viewer_open, |this| {
                this.child(self.media_viewer_panel(
                    crate::ui::media_viewer::ViewerPart::Inline,
                    window,
                    cx,
                ))
            })
            .children(self.photo_editor_overlay(cx))
            // M1: right-click message context menu.
            .when_some(self.message_ui.menu, |this, menu| {
                this.child(self.message_menu_overlay(menu, cx))
            })
            // The expanded reaction selector, where the menu was.
            .when_some(self.pickers.media_panel.reaction, |this, target| {
                let panel = self.media_panel(cx);
                this.child(
                    div()
                        .id("reaction-selector-layer")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .child(
                            div()
                                .id("reaction-selector-backdrop")
                                .absolute()
                                .inset_0()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_media_panel(cx);
                                })),
                        )
                        .child(
                            anchored()
                                .position(target.position)
                                .snap_to_window_with_margin(px(8.))
                                .child(panel),
                        ),
                )
            })
            // MED4: Instant View reader overlay (above the menu).
            .when_some(self.instant_view_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            // Link menu and tooltip above the reader, whose links open them.
            .when_some(self.link_popup_overlay(cx), |this, overlay| {
                this.child(overlay)
            })
            .when_some(self.link_tooltip_overlay(), |this, overlay| {
                this.child(overlay)
            })
            .into_any_element()
    }
}

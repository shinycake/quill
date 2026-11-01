//! The story viewer's reply row with emoji, stickers, GIFs and voice.
//!
//! Telegram Desktop's story reply field is the full compose control
//! (`media_stories_reply.cpp`: `ComposeControls` with the emoji / sticker /
//! GIF panel and the voice recorder), and a send goes to the poster chat as
//! a reply to the story. Quill reuses the main composer's media panel and
//! voice recorder: while the reply row is open the panel is drawn beside the
//! story, and the sticker, GIF, emoji and recording sends route here instead
//! of to the open chat.

use super::*;
use crate::ui::composer_field::{emoji_token, field_doc};
use crate::ui::media_panel::PanelTab;
use crate::ui::recording::RecordMode;
use crate::ui::recording_state::CaptureFor;
use crate::ui::status_note::StatusNote;
use gpui_base::input::InputContent;
use gpui_kit::assets::IconName;
use quill::lang::{keys, tr, tr_with};
use quill::telegram::requests::{AnimationSend, StickerSend};

/// The story the reply row is open on: `(poster chat, story id)`.
pub(in crate::ui) fn reply_target(
    reply_open: bool,
    current: Option<(ChatId, i32)>,
) -> Option<(ChatId, i32)> {
    if reply_open { current } else { None }
}

/// Whether a media pick belongs to the story reply instead of the open chat.
pub(in crate::ui) fn picks_go_to_story(reply_open: bool, panel_for_story: bool) -> bool {
    reply_open && panel_for_story
}

impl QuillApp {
    /// The story the reply row is replying to.
    pub(in crate::ui) fn story_reply_target(&self) -> Option<(ChatId, i32)> {
        reply_target(
            self.stories.reply_open,
            self.stories
                .viewer
                .current()
                .map(|item| (item.chat_id, item.story_id)),
        )
    }

    /// The emoji / sticker / GIF panel is open for the story reply.
    pub(in crate::ui) fn story_reply_panel_active(&self) -> bool {
        picks_go_to_story(self.stories.reply_open, self.stories.reply_panel)
            && self.pickers.media_panel.open
            && self.pickers.media_panel.reaction.is_none()
            && self.pickers.media_panel.status.is_none()
    }

    /// A recording started from the story reply row is in progress.
    pub(in crate::ui) fn story_reply_recording_active(&self) -> bool {
        self.stories.reply_open && self.recording.purpose() == Some(CaptureFor::StoryReply)
    }

    /// The reply row is done (sent, closed, or the story changed): close
    /// what it opened.
    pub(in crate::ui) fn end_story_reply(&mut self, cx: &mut Context<Self>) {
        if self.story_reply_panel_active() {
            self.close_media_panel(cx);
        }
        if self.story_reply_recording_active()
            || self.recording.opening_for(CaptureFor::StoryReply)
        {
            self.cancel_recording(cx);
        }
        self.stories.reply_open = false;
        self.stories.reply_panel = false;
    }

    pub(super) fn toggle_story_reply_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.story_reply_panel_active() && self.pickers.media_panel.tab == tab {
            self.stories.reply_panel = false;
            self.close_media_panel(cx);
        } else {
            self.stories.reply_panel = true;
            self.open_media_panel(tab, cx);
        }
    }

    /// The reply row: the field with the emoji / GIF buttons and the mic or
    /// Send, or the recording bar while a voice message is recorded.
    pub(in crate::ui) fn story_reply_row(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.story_reply_recording_active() {
            return div()
                .w(px(360.))
                .child(self.record_bar(cx))
                .into_any_element();
        }
        let has_text = !self.stories.reply_input.read(cx).value().trim().is_empty();
        let panel = self.story_reply_panel_active();
        let action = if has_text {
            Button::new("story-reply-send")
                .label(tr(keys::Q_MEDIA_SEND))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.send_story_reply(window, cx);
                }))
                .into_any_element()
        } else {
            let video = self.record_mode() == RecordMode::Video;
            let label = if video {
                tr(keys::Q_MEDIA_RECORD_A_VIDEO_MESSAGE)
            } else {
                tr(keys::Q_MEDIA_RECORD_A_VOICE_MESSAGE)
            };
            // Right-click flips voice / video, as on the composer's mic.
            div()
                .id("story-reply-mic-wrap")
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| this.toggle_record_mode(cx)),
                )
                .child(
                    Button::new("story-reply-mic")
                        .icon(if video {
                            IconName::Video
                        } else {
                            IconName::Mic
                        })
                        .ghost()
                        .tooltip(label)
                        .accessibility_label(label)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.start_story_reply_recording(cx);
                        })),
                )
                .into_any_element()
        };
        div()
            .flex()
            .gap_1()
            .items_center()
            .w(px(360.))
            .child(
                Button::new("story-reply-emoji")
                    .icon(IconName::FaceSlightlySmiling)
                    .ghost()
                    .selected(panel)
                    .tooltip(tr(keys::Q_MEDIA_EMOJI_STICKERS_AND_GIFS))
                    .accessibility_label(tr(keys::Q_MEDIA_EMOJI_STICKERS_AND_GIFS))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_story_reply_panel(PanelTab::Emoji, cx);
                    })),
            )
            .child(
                div().flex_1().min_w_0().child(
                    Textarea::new(&self.stories.reply_input)
                        .aria_label(tr(keys::Q_MEDIA_REPLY_TO_STORY))
                        .token(self.field_token_renderer(&self.stories.reply_input, cx))
                        .h(px(40.)),
                ),
            )
            .child(action)
            .into_any_element()
    }

    /// The shared media panel, drawn to the right of the story while the
    /// reply row has it open.
    pub(in crate::ui) fn story_reply_panel_layer(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.story_reply_panel_active() {
            return None;
        }
        let panel = self.media_panel(cx);
        Some(
            div()
                .id("story-reply-panel")
                .absolute()
                .left(relative(0.5))
                .ml(px(204.))
                .bottom(px(24.))
                .child(crate::ui::anim_layer::occluder(panel))
                .into_any_element(),
        )
    }

    /// A sticker picked in the panel. `true` when it went to the story.
    pub(in crate::ui) fn route_story_reply_sticker(
        &mut self,
        file_id: FileId,
        emoji: &str,
        size: (i32, i32),
        thumb: Option<(FileId, i32, i32)>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((chat_id, story_id)) = self
            .story_reply_panel_active()
            .then(|| self.story_reply_target())
            .flatten()
        else {
            return false;
        };
        // A live story takes text comments only (tdesktop's video stream
        // field has no sticker or GIF sending).
        if self.live_story_shown().is_some() {
            return true;
        }
        if self.premium_sticker_blocked(file_id, cx) {
            return true;
        }
        if self.slow_mode_blocked(chat_id, cx) {
            return true;
        }
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .send_story_reply_sticker(
                    chat_id,
                    story_id,
                    StickerSend {
                        file_id,
                        emoji,
                        width: size.0,
                        height: size.1,
                        thumb,
                        reply_to: None,
                        topic_id: None,
                    },
                )
                .is_ok()
        });
        self.finish_story_reply_media(sent, "sticker", cx);
        true
    }

    /// A saved GIF picked in the panel. `true` when it went to the story.
    pub(in crate::ui) fn route_story_reply_gif(
        &mut self,
        file_id: FileId,
        duration: i32,
        size: (i32, i32),
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((chat_id, story_id)) = self
            .story_reply_panel_active()
            .then(|| self.story_reply_target())
            .flatten()
        else {
            return false;
        };
        // A live story takes text comments only (tdesktop's video stream
        // field has no sticker or GIF sending).
        if self.live_story_shown().is_some() {
            return true;
        }
        if self.slow_mode_blocked(chat_id, cx) {
            return true;
        }
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .send_story_reply_animation(
                    chat_id,
                    story_id,
                    AnimationSend {
                        file_id,
                        duration,
                        width: size.0,
                        height: size.1,
                        reply_to: None,
                        topic_id: None,
                    },
                )
                .is_ok()
        });
        self.finish_story_reply_media(sent, "GIF", cx);
        true
    }

    /// A custom emoji picked in the panel goes into the reply field as one
    /// inline token, like the composer's. `true` when it did.
    pub(in crate::ui) fn route_story_reply_custom_emoji(
        &mut self,
        fallback: &str,
        custom_emoji_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.story_reply_panel_active() {
            return false;
        }
        let fallback = if fallback.is_empty() { "⭐" } else { fallback };
        let text = fallback.to_string();
        let len = text.len();
        let content = InputContent::new(text.clone())
            .with_token(0..len, emoji_token(custom_emoji_id, &text))
            .unwrap_or_else(|_| InputContent::new(text));
        // A live story comment carries custom emoji too (its entities).
        let field = if self.live_story_shown().is_some() {
            self.stories.live.input.clone()
        } else {
            self.stories.reply_input.clone()
        };
        field.update(cx, |input, cx| {
            let range = input.selected_range();
            if input
                .replace_range_with_content(range, content.clone(), window, cx)
                .is_err()
            {
                input.replace(content.text().to_string(), window, cx);
            }
            input.focus(window, cx);
        });
        cx.notify();
        true
    }

    /// The reply field as composer markup: formatting and custom emoji
    /// become entities when it is sent.
    pub(in crate::ui) fn story_reply_markup(&self, cx: &App) -> String {
        field_doc(self.stories.reply_input.read(cx)).to_markup()
    }

    /// An emoji picked in the panel goes into the reply field. `true` when
    /// it did.
    pub(in crate::ui) fn route_story_reply_emoji(
        &mut self,
        emoji: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.story_reply_panel_active() {
            return false;
        }
        let field = if self.live_story_shown().is_some() {
            self.stories.live.input.clone()
        } else {
            self.stories.reply_input.clone()
        };
        field.update(cx, |input, cx| {
            input.replace(emoji, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
        true
    }

    /// Report a media send and close the row after a successful one. `sent`
    /// is `None` outside a live session (the demo).
    fn finish_story_reply_media(&mut self, sent: Option<bool>, what: &str, cx: &mut Context<Self>) {
        match sent {
            Some(true) => {
                self.connection.set_note(StatusNote::silent(tr_with(
                    keys::Q_MEDIA_STORY_REPLY_SENT_2,
                    &[("what", what)],
                )));
                self.end_story_reply(cx);
            }
            Some(false) => {
                self.connection.set_note(StatusNote::error(tr_with(
                    keys::Q_MEDIA_COULD_NOT_SEND_STORY_REPLY_2,
                    &[("what", what)],
                )));
            }
            None => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MEDIA_DEMO_STORY_REPLIES_RUN_WITH,
                )));
            }
        }
        cx.notify();
    }

    /// The mic: record a voice or round video message for the story, in the
    /// composer's current record mode (Telegram Desktop's story reply field
    /// records both).
    pub(in crate::ui) fn start_story_reply_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording_busy() || self.story_reply_target().is_none() {
            return;
        }
        if self.story_reply_panel_active() {
            self.stories.reply_panel = false;
            self.close_media_panel(cx);
        }
        match self.record_mode() {
            RecordMode::Audio => {
                self.with_capture_access(false, Self::begin_story_voice_capture, cx)
            }
            RecordMode::Video => {
                self.with_capture_access(true, Self::begin_story_video_capture, cx)
            }
        }
    }

    fn begin_story_video_capture(&mut self, cx: &mut Context<Self>) {
        self.start_video_capture(CaptureFor::StoryReply, false, cx);
    }

    /// The recording bar's Send for a round video started from the reply
    /// row. `true` when it handled the send; ffmpeg finishes the file off
    /// the main thread.
    pub(in crate::ui) fn story_reply_video_recording(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.story_reply_recording_active() {
            return false;
        }
        let Some((chat_id, story_id)) = self.story_reply_target() else {
            return false;
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return true;
        }
        let Some(capture) = self.recording.take_video() else {
            return true;
        };
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        cx.notify();
        let finished = cx.background_spawn(async move { capture.finish() });
        Self::spawn_main_in(window, cx, async move |this, cx| {
            let result = finished.await;
            let path = result.as_ref().ok().map(|draft| draft.path.clone());
            let updated = this.update_in(cx, |this, _window, cx| {
                let sent = match result {
                    Ok(draft) => {
                        let sent = this.live.as_mut().map(|live| {
                            live.driver
                                .send_story_reply_video_note(chat_id, story_id, &draft)
                                .is_ok()
                        });
                        if sent != Some(true) {
                            let _ = std::fs::remove_file(&draft.path);
                        }
                        sent
                    }
                    Err(err) => {
                        this.connection.set_note(StatusNote::error(err));
                        cx.notify();
                        return;
                    }
                };
                this.finish_story_reply_media(sent, tr(keys::Q_MEDIA_VIDEO_MESSAGE), cx);
            });
            if updated.is_err()
                && let Some(path) = path
            {
                let _ = std::fs::remove_file(path);
            }
        })
        .detach();
        true
    }

    fn begin_story_voice_capture(&mut self, cx: &mut Context<Self>) {
        self.start_voice_capture(CaptureFor::StoryReply, cx);
    }

    /// The recording bar's Send, for a recording started from the reply
    /// row. `true` when it handled the send.
    pub(in crate::ui) fn send_story_reply_voice_recording(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.story_reply_recording_active() {
            return false;
        }
        let Some((chat_id, story_id)) = self.story_reply_target() else {
            return false;
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return true;
        }
        let Some(capture) = self.recording.take_voice() else {
            return true;
        };
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.stop_record_preview();
        let finish = capture.close_microphone();
        cx.notify();
        // The encoder is joined off the UI thread.
        let finished = cx.background_spawn(async move { finish.complete() });
        Self::spawn_main(cx, async move |this, cx| {
            let result = finished.await;
            let path = result.as_ref().ok().map(|draft| draft.path.clone());
            let updated = this.update(cx, |this, cx| {
                let draft = match result {
                    Ok(draft) => draft,
                    Err(err) => {
                        this.connection.set_note(StatusNote::error(err));
                        cx.notify();
                        return;
                    }
                };
                let sent = this
                    .live
                    .as_mut()
                    .map(|live| {
                        live.driver
                            .send_story_reply_voice(chat_id, story_id, &draft)
                            .is_ok()
                    })
                    .or_else(|| this.demo_session.is_none().then_some(false));
                if sent != Some(true) {
                    let _ = std::fs::remove_file(&draft.path);
                }
                this.finish_story_reply_media(sent, tr(keys::Q_MEDIA_VOICE_MESSAGE), cx);
            });
            if updated.is_err()
                && let Some(path) = path
            {
                let _ = std::fs::remove_file(path);
            }
        })
        .detach();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{picks_go_to_story, reply_target};
    use quill::ids::ChatId;

    #[test]
    fn reply_target_needs_the_row_open() {
        let story = Some((ChatId(7), 6));
        assert_eq!(reply_target(true, story), story);
        assert_eq!(reply_target(false, story), None);
        assert_eq!(reply_target(true, None), None);
    }

    #[test]
    fn picks_route_to_the_story_only_for_its_own_panel() {
        // The panel opened from the main composer keeps sending to the chat
        // even if a story is open on top.
        assert!(picks_go_to_story(true, true));
        assert!(!picks_go_to_story(true, false));
        assert!(!picks_go_to_story(false, true));
    }
}

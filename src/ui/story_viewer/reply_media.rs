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
use crate::ui::media_panel::PanelTab;
use gpui_kit::assets::IconName;
use quill::telegram::requests::{AnimationSend, StickerSend};
use quill::voice::VoiceCapture;

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
    }

    /// A voice recording started from the story reply row is in progress.
    pub(in crate::ui) fn story_reply_recording_active(&self) -> bool {
        self.stories.reply_recording && self.stories.reply_open && self.recording_active()
    }

    /// The reply row is done (sent, closed, or the story changed): close
    /// what it opened.
    pub(in crate::ui) fn end_story_reply(&mut self, cx: &mut Context<Self>) {
        if self.story_reply_panel_active() {
            self.close_media_panel(cx);
        }
        if self.story_reply_recording_active() {
            self.cancel_recording(cx);
        }
        self.stories.reply_open = false;
        self.stories.reply_panel = false;
        self.stories.reply_recording = false;
    }

    fn toggle_story_reply_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
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
                .label("Send")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.send_story_reply(window, cx);
                }))
                .into_any_element()
        } else {
            Button::new("story-reply-mic")
                .icon(IconName::Mic)
                .ghost()
                .tooltip("Record a voice message")
                .accessibility_label("Record a voice message")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.start_story_reply_voice(cx);
                }))
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
                    .tooltip("Emoji, stickers and GIFs")
                    .accessibility_label("Emoji, stickers and GIFs")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_story_reply_panel(PanelTab::Emoji, cx);
                    })),
            )
            .child(
                div().flex_1().min_w_0().child(
                    Textarea::new(&self.stories.reply_input)
                        .aria_label("Reply to story")
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
        if self.session().is_some_and(|session| {
            session.sticker_requires_premium(file_id) && !session.my_is_premium()
        }) {
            self.connection.status_note = "Sending this sticker requires Telegram Premium".into();
            cx.notify();
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

    /// An emoji picked in the panel goes into the reply field. `true` when
    /// it did. A custom emoji is inserted as its plain emoji: the reply text
    /// is sent without entities.
    pub(in crate::ui) fn route_story_reply_emoji(
        &mut self,
        emoji: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.story_reply_panel_active() {
            return false;
        }
        self.stories.reply_input.update(cx, |input, cx| {
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
                self.connection.status_note = format!("Story reply sent ({what})");
                self.end_story_reply(cx);
            }
            Some(false) => {
                self.connection.status_note = format!("could not send story reply {what}");
            }
            None => {
                self.connection.status_note = "demo — story replies run with live TDLib".into();
            }
        }
        cx.notify();
    }

    /// The mic: record a voice message for the story (Telegram Desktop's
    /// story reply field records voice and round video; Quill records
    /// voice).
    pub(in crate::ui) fn start_story_reply_voice(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() || self.story_reply_target().is_none() {
            return;
        }
        if self.story_reply_panel_active() {
            self.stories.reply_panel = false;
            self.close_media_panel(cx);
        }
        self.with_capture_access(false, Self::begin_story_voice_capture, cx);
    }

    fn begin_story_voice_capture(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() {
            return;
        }
        match VoiceCapture::start() {
            Ok(capture) => {
                self.recording.voice_capture = Some(capture);
                self.stories.reply_recording = true;
                self.spawn_voice_tick(cx);
                self.connection.status_note = "recording voice reply".into();
            }
            Err(err) => {
                self.connection.status_note = err;
            }
        }
        cx.notify();
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
        let Some(capture) = self.recording.voice_capture.take() else {
            return true;
        };
        self.recording.locked = false;
        self.recording.discard_confirm = false;
        self.stop_record_preview();
        self.stories.reply_recording = false;
        let draft = match capture.finish() {
            Ok(draft) => draft,
            Err(err) => {
                self.connection.status_note = err;
                cx.notify();
                return true;
            }
        };
        let sent = self
            .live
            .as_mut()
            .map(|live| {
                live.driver
                    .send_story_reply_voice(chat_id, story_id, &draft)
                    .is_ok()
            })
            .or_else(|| self.demo_session.is_none().then_some(false));
        self.finish_story_reply_media(sent, "voice message", cx);
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

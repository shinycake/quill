//! composer submit/send/edit/reply/scheduling, command menu state, drafts consumption.
#![deny(clippy::indexing_slicing)]

use super::actions::{
    FormatBlockQuote, FormatBold, FormatClear, FormatItalic, FormatMonospace, FormatSpoiler,
    FormatStrikethrough, FormatUnderline,
};
use super::app::{PaneMode, QuillApp};
use super::demo::demo_media_allowlist;
use super::message_text::rich_block_element;
use super::nested_click::SwallowPress;
use super::scheduled::ScheduleTarget;
use super::*;
use crate::ui::composer_state::DraftClear;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, CommandMenuItem, ComposerAttachment, ComposerEdit, ComposerReplyTo,
    ComposerScheduling, ComposerSnapshot, FormatAction, SendOptions, apply_format_markup,
    begin_edit_keeping_reply, cancel_edit_draft, cancel_edit_keeping_reply, cancel_reply_draft,
    clear_format_markup, command_menu_trigger, filter_command_menu_items,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with, trn};
use quill::schedule::ScheduleKind;
use quill::send_rights::SendKind;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatKind;
use quill::telegram::requests::SelfDestructSend;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
pub(super) fn apply_ready_reply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Got it — quoting you.","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[{"@type":"textEntity","offset":0,"length":5,"type":{"@type":"textEntityTypeBold"}},{"@type":"textEntity","offset":11,"length":8,"type":{"@type":"textEntityTypeItalic"}},{"@type":"textEntity","offset":20,"length":4,"type":{"@type":"textEntityTypeStrikethrough"}}]},"rich_texts":[],"position":0,"is_manual":false},"checklist_task_id":0,"poll_option_id":""}}}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    /// Slice parity:platform-offline-errors — use the submitted schedule,
    /// before the composer resets its one-shot scheduling choice.
    pub(in crate::ui) fn send_started_note(
        &self,
        scheduling: ComposerScheduling,
        online_note: &str,
    ) -> StatusNote {
        let offline = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline());
        let note = quill::composer::send_started_note(offline, scheduling, online_note);
        if offline {
            StatusNote::info(note)
        } else {
            StatusNote::silent(note)
        }
    }

    pub(super) fn submit_composer(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.pane_mode() {
            PaneMode::Connecting => {
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_CHAT_SIGN_IN_BEFORE_SENDING)));
                cx.notify();
            }
            PaneMode::Ready => {
                // An agent driving the window never sends from here: the
                // message goes through the agent write path.
                if quill::agent::ui_driver::guard::injecting() {
                    self.agent_route_send(text, cx);
                    return;
                }
                if self.session().is_some_and(|s| s.is_frozen()) {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_CHAT_YOUR_ACCOUNT_IS_FROZEN_AND,
                    )));
                    cx.notify();
                    return;
                }
                // Re-check the targets at send time: a reply to a deleted
                // message is dropped (the text stays), an edit of one is
                // cancelled and nothing is sent.
                if self.cancel_gone_composer_targets(window, cx) {
                    return;
                }
                if self.composer_ui.pending_edit.is_some() {
                    self.submit_edit(text, window, cx);
                    return;
                }
                if self.submit_suggested_change(&text, window, cx) {
                    return;
                }
                // The tray goes, now or once its renders land: an editor
                // asked for mid-render no longer opens.
                self.viewer.photo_editor_after = None;
                // A photo's edit is still rendering: the Send waits for it,
                // so the unedited file never goes out.
                if self.tray_renders_an_edit() {
                    self.update_pending_send(|send| send.held_send = Some(text));
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_OPS_PREPARING_THE_FILES)));
                    cx.notify();
                    return;
                }
                // B4: the forward bar sends the comment, then the forward.
                if self.forward_bar_here() {
                    self.submit_forward_bar(text, window, cx);
                    return;
                }
                // Sending returns the reader to the latest messages: the
                // position left behind is not where the chat reopens.
                self.forget_scroll_on_return_to_latest();
                self.remember_sent_hashtags(&text);
                // M2: the rich editor sends blocks, not text.
                if self.composer_ui.rich_editor_open {
                    self.submit_rich_composer(text, window, cx);
                    return;
                }
                let plan = self.live.as_ref().map(|live| {
                    let session = &live.driver.session;
                    (
                        session.open_chat,
                        session.view_generation,
                        session
                            .open_chat
                            .and_then(|id| session.chats.get(&id.0).map(|chat| chat.supported())),
                    )
                });
                if let Some((open_chat, view_generation, supported)) = plan {
                    let Some(chat_id) = open_chat else {
                        self.connection
                            .set_note(StatusNote::silent(tr(keys::Q_CHAT_SELECT_A_CHAT_TO_SEND)));
                        cx.notify();
                        return;
                    };
                    if supported != Some(true) {
                        self.connection
                            .set_note(StatusNote::silent(tr(keys::Q_CHAT_THIS_CHAT_TYPE_IS_NOT)));
                        cx.notify();
                        return;
                    }
                    if self.paid_send_waits(chat_id, &text, cx) {
                        return;
                    }
                    // A video's sound track is still being read: wait, so a
                    // soundless one is sent as a GIF.
                    if self.tray_videos_checking() {
                        self.connection.set_note(StatusNote::silent(tr(
                            keys::Q_CHAT_CHECKING_THE_VIDEO_SEND_AGAIN,
                        )));
                        cx.notify();
                        return;
                    }
                    // Photos and videos of an earlier Send are still being
                    // read: this Send is refused for now (it does not
                    // queue), so the messages keep their order.
                    if self.tray_preparing() {
                        self.connection
                            .set_note(StatusNote::silent(tr(keys::Q_OPS_PREPARING_THE_FILES)));
                        cx.notify();
                        return;
                    }
                    // A picture sent as a file is still getting its thumbnail.
                    if self.tray_thumbnails_pending() {
                        self.connection.set_note(StatusNote::silent(tr(
                            keys::Q_CHAT_PREPARING_THE_FILE_SEND_AGAIN,
                        )));
                        cx.notify();
                        return;
                    }
                    let attachments = self.pending_attachments().to_vec();
                    // The viewer's own rights in a group: text needs the
                    // basic right, each attachment its media right.
                    if attachments.is_empty() {
                        if !text.trim().is_empty() && self.deny_send(SendKind::Message, cx) {
                            return;
                        }
                    } else if self.deny_attachments(attachments.iter().map(|a| a.kind), cx) {
                        return;
                    }
                    // A file over the upload limit opens the file size box.
                    if self.deny_oversized_attachments(&attachments, cx) {
                        return;
                    }
                    // Phase B3: self-destruct only leaves the composer on
                    // photo/video attachments; the driver additionally
                    // strips it for non-private chats (TDLib's 400 gate).
                    let self_destruct = attachments
                        .iter()
                        .all(|att| {
                            matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
                        })
                        .then_some(self.pending_send().self_destruct)
                        .flatten();
                    // MED1: the grouping toggle decides album vs separate
                    // sends (TGX `RememberAlbumSetting`).
                    let albumable = ComposerAttachment::albumable(&attachments);
                    let grouped = self.composer_group_media_effective();
                    let reply = self.reply_for_send(cx);
                    let mut snaps = Vec::new();
                    // A repeating message is sent alone (tdesktop limits the
                    // batch to one item with a repeat period).
                    let repeating = matches!(
                        self.pending_send().scheduling,
                        ComposerScheduling::SendAtDate(_)
                    ) && self.pending_send().schedule_repeat != 0;
                    if albumable && grouped && !repeating {
                        // A video turned into a GIF leaves the album and
                        // goes alone (tdesktop `GroupRanges`); paid media
                        // is one message.
                        let groups = if self.paid_stars_in_effect() > 0 {
                            vec![attachments]
                        } else {
                            ComposerAttachment::split_for_send(&attachments, true)
                        };
                        for (i, group) in groups.into_iter().enumerate() {
                            let text = if i == 0 { text.clone() } else { String::new() };
                            let snap = if group.len() >= 2 {
                                ComposerSnapshot::capture_album(
                                    chat_id,
                                    view_generation,
                                    text,
                                    group,
                                )
                            } else {
                                ComposerSnapshot::capture_with_attachment(
                                    chat_id,
                                    view_generation,
                                    text,
                                    group.into_iter().next(),
                                )
                            };
                            snaps.push(
                                snap.with_reply(if i == 0 { reply.clone() } else { None })
                                    .with_self_destruct(self_destruct)
                                    // M1: silent / scheduled / when-online /
                                    // link-preview options ride the snapshot to
                                    // `sendMessage.options`.
                                    .with_send_options(self.composer_post_options())
                                    // MED4: caption-above-media toggle.
                                    .with_caption_above_media(
                                        self.pending_send().caption_above
                                            && !self.open_chat_is_secret(),
                                    ),
                            );
                        }
                    } else if attachments.len() >= 2 {
                        // One message per ungrouped attachment, the
                        // caption riding the first.
                        for (i, att) in attachments.into_iter().enumerate() {
                            snaps.push(
                                ComposerSnapshot::capture_with_attachment(
                                    chat_id,
                                    view_generation,
                                    if i == 0 { text.clone() } else { String::new() },
                                    Some(att),
                                )
                                .with_reply(if i == 0 { reply.clone() } else { None })
                                .with_self_destruct(self_destruct)
                                .with_send_options(self.composer_post_options())
                                // MED4: caption-above-media toggle.
                                .with_caption_above_media(
                                    self.pending_send().caption_above
                                        && !self.open_chat_is_secret(),
                                ),
                            );
                        }
                    } else {
                        snaps.push(
                            ComposerSnapshot::capture_with_attachment(
                                chat_id,
                                view_generation,
                                text,
                                attachments.first().cloned(),
                            )
                            .with_reply(reply)
                            .with_self_destruct(self_destruct)
                            // M1: silent / scheduled / when-online /
                            // link-preview options ride the snapshot to
                            // `sendMessage.options`.
                            .with_send_options(self.composer_post_options())
                            // MED4: caption-above-media toggle.
                            .with_caption_above_media(
                                self.pending_send().caption_above && !self.open_chat_is_secret(),
                            ),
                        );
                    }
                    // The tray's High quality and price choices.
                    let media_options = self.composer_media_options();
                    for snap in &mut snaps {
                        snap.media = media_options;
                    }
                    if snaps.first().is_none_or(ComposerSnapshot::is_empty) {
                        self.connection.set_note(StatusNote::silent(tr(
                            keys::Q_CHAT_TYPE_A_MESSAGE_OR_ATTACH,
                        )));
                        cx.notify();
                        return;
                    }
                    // R8: tdesktop cuts a text over the length limit into
                    // several messages (`ApiWrap::sendMessage` + `CutPart`);
                    // the reply rides the first part only.
                    if let [only] = snaps.as_slice()
                        && only.attachment.is_none()
                        && only.album.is_empty()
                    {
                        let limit = self.text_length_limit();
                        if quill::text_split::units_over_limit(&only.text, limit) > 0 {
                            let parts = quill::text_split::split_markup_text(&only.text, limit);
                            if parts.is_empty() {
                                self.connection.set_note(StatusNote::silent(tr(
                                    keys::Q_CHAT_MESSAGE_TOO_LONG_TO_SEND,
                                )));
                                cx.notify();
                                return;
                            }
                            let template = only.clone();
                            snaps = parts
                                .into_iter()
                                .enumerate()
                                .map(|(i, part)| {
                                    let mut snap = template.clone();
                                    snap.text = part;
                                    if i > 0 {
                                        snap.reply_to = None;
                                    }
                                    snap
                                })
                                .collect();
                        }
                    }
                    // Phase A1: slow-mode gate (centralized in
                    // `slow_mode_blocked`).
                    if self.slow_mode_refuses(chat_id, snaps.len(), cx)
                        || self.slow_mode_blocked(chat_id, cx)
                    {
                        return;
                    }
                    // MED1: ungrouped sends go out one message at a time.
                    // Photos and videos are read first, off the UI thread
                    // (`send_prep.rs`); the send goes out when they are
                    // ready, to this chat, and the composer is cleared
                    // then. Everything else goes now.
                    let Some(slot) = self.send_slot() else {
                        return;
                    };
                    let origin = super::send_prep::SendOrigin::new(
                        slot,
                        chat_id,
                        self.composer_markup(cx),
                        &snaps,
                    );
                    if snaps.iter().any(ComposerSnapshot::needs_media_prep) {
                        self.begin_media_prep(origin, snaps, window, cx);
                    } else {
                        self.deliver_live_send(origin, snaps, window, cx);
                    }
                    return;
                }
                if self.demo_session.is_some() {
                    // Phase A1: the slow-mode gate applies to the demo
                    // session too (fixture-driven countdown, no live
                    // Telegram).
                    let demo_chat = self.demo_session.as_ref().and_then(|s| s.open_chat);
                    if demo_chat.is_some_and(|chat_id| self.slow_mode_blocked(chat_id, cx)) {
                        return;
                    }
                    let attachments = self.pending_attachments().to_vec();
                    if attachments.iter().any(|att| {
                        att.kind == AttachmentKind::VideoNote
                            && quill::video::probe_local_video_note(&att.path).is_err()
                    }) {
                        self.connection
                            .set_note(StatusNote::silent(tr(keys::Q_CHAT_VIDEO_NOTE_MUST_BE_A)));
                        cx.notify();
                        return;
                    }
                    let reply = self.reply_for_send(cx);
                    // MED1: the grouping toggle applies to the demo too.
                    let albumable = ComposerAttachment::albumable(&attachments);
                    let messages = if albumable && self.composer_group_media_effective() {
                        ComposerAttachment::split_for_send(&attachments, true).len()
                    } else {
                        attachments.len().max(1)
                    };
                    if demo_chat
                        .is_some_and(|chat_id| self.slow_mode_refuses(chat_id, messages, cx))
                    {
                        return;
                    }
                    if albumable && self.composer_group_media_effective() {
                        self.apply_demo_album(&text, &attachments, reply.as_ref());
                    } else if attachments.len() >= 2 {
                        for (i, att) in attachments.iter().enumerate() {
                            self.apply_demo_outgoing(
                                if i == 0 { &text } else { "" },
                                Some(att),
                                if i == 0 { reply.as_ref() } else { None },
                            );
                        }
                    } else {
                        self.apply_demo_outgoing(&text, attachments.first(), reply.as_ref());
                    }
                    self.finish_pending_send();
                    self.composer_ui.group_media = None;
                    self.composer_ui.set_reply(None);
                    if let Some(chat_id) = self.demo_session.as_ref().and_then(|s| s.open_chat) {
                        self.forget_local_draft(chat_id);
                    }
                    self.composer
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.connection.set_note(StatusNote::silent(tr(
                        keys::Q_CHAT_DEMO_SEND_APPLIED_LOCALLY_NO,
                    )));
                    cx.notify();
                }
            }
            PaneMode::Synthetic => {
                self.chat.update(cx, |chat, cx| chat.send_text(text, cx));
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                cx.notify();
            }
        }
    }

    /// M2: send the rich editor's blocks via `inputMessageRichMessage`
    /// (schema 1.8.67, line 6084). Same chat/supported/slow-mode gates as
    /// the text path; on failure the editor keeps its text (nothing is
    /// lost) and the error surfaces as a status note — never as success.
    pub(super) fn submit_rich_composer(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cancel_gone_composer_targets(window, cx) {
            return;
        }
        // Callers only open the rich editor with a live session.
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let session = &live.driver.session;
        let (open_chat, supported) = (
            session.open_chat,
            session
                .open_chat
                .and_then(|id| session.chats.get(&id.0).map(|chat| chat.supported())),
        );
        // Premium gate (`premiumFeatureRichMessages`): the editor can only
        // be opened by Premium users, but double-check here too — the
        // server would reject `inputMessageRichMessage` from a non-Premium
        // user and we never want to fake a successful send.
        let premium = session.my_is_premium();
        if !premium {
            match quill::rich::RichDraft::from_markup(&text) {
                Some(rich) => self.refuse_rich_send(&rich, window, cx),
                None => {
                    self.connection.set_note(StatusNote::info(tr(
                        keys::Q_CHAT_RICH_MESSAGES_REQUIRE_TELEGRAM_PREMIUM,
                    )));
                    cx.notify();
                }
            }
            return;
        }
        let Some(chat_id) = open_chat else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_SELECT_A_CHAT_TO_SEND)));
            cx.notify();
            return;
        };
        if supported != Some(true) {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_THIS_CHAT_TYPE_IS_NOT)));
            cx.notify();
            return;
        }
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let mut blocks = quill::rich::markup_to_blocks(&text);
        // M2: an explicitly attached document becomes an inline document
        // block (`pageBlockDocument`) — the file picker's local path, never
        // a TDLib-provided `local.path`.
        if let Some(attachment) = self
            .pending_attachments()
            .iter()
            .find(|attachment| attachment.kind == AttachmentKind::Document)
        {
            blocks.push(quill::rich::RichBlock::Document {
                file_name: attachment.file_name.clone(),
                caption: String::new(),
                local_path: Some(attachment.path.clone()),
            });
        }
        // Attached photos/videos become inline media blocks — every one is
        // converted (photos and videos accumulate, unlike documents), same
        // local-path rule as above.
        for attachment in self.pending_attachments() {
            match attachment.kind {
                AttachmentKind::Photo => blocks.push(quill::rich::RichBlock::Photo {
                    caption: String::new(),
                    local_path: Some(attachment.path.clone()),
                }),
                AttachmentKind::Video => blocks.push(quill::rich::RichBlock::Video {
                    caption: String::new(),
                    local_path: Some(attachment.path.clone()),
                }),
                _ => {}
            }
        }
        // Rich-text max length (same status-note pattern as the caption
        // limit): refuse over-limit drafts before the emptiness check so
        // the note names the limit instead of "type a message".
        if quill::rich::rich_blocks_char_len(&blocks) > quill::rich::RICH_TEXT_MAX_CHARS {
            self.connection.set_note(StatusNote::silent(tr_with(
                keys::Q_CHAT_RICH_MESSAGE_TOO_LONG_MAX,
                &[(
                    "quill_rich_rich_text_max_chars",
                    &(quill::rich::RICH_TEXT_MAX_CHARS).to_string(),
                )],
            )));
            cx.notify();
            return;
        }
        if quill::rich::input_rich_message(&blocks).is_none() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_TYPE_A_MESSAGE_OR_ATTACH,
            )));
            cx.notify();
            return;
        }
        let reply_to = self
            .reply_for_send(cx)
            .and_then(|reply| reply.send_reply(chat_id));
        let options = self.composer_post_options();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match live
            .driver
            .send_rich_snapshot(chat_id, &blocks, reply_to, &options)
        {
            Ok(_) => {
                self.finish_pending_send();
                self.composer_ui.schedule_popup_open = false;
                self.composer_ui.set_reply(None);
                self.composer_ui.rich_editor_open = false;
                self.clear_suggest_bar_after_send(chat_id);
                self.composer_ui.clear_draft_on_success = Some(DraftClear::whole_composer(chat_id));
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.forget_local_draft(chat_id);
                self.connection
                    .set_note(self.send_started_note(options.scheduling, tr(keys::Q_CHAT_SENDING)));
            }
            Err(_) => {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_CHAT_COULD_NOT_SEND_RICH_MESSAGE,
                )));
            }
        }
        cx.notify();
    }

    pub(super) fn attach_dropped_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        // B5: while editing, a drop or paste replaces the message's media
        // (tdesktop `EditCaptionBox` accepts exactly one file).
        if self.composer_ui.pending_edit.is_some() {
            match paths {
                [only]
                    if self
                        .composer_ui
                        .pending_edit
                        .as_ref()
                        .is_some_and(|e| e.allows_replace()) =>
                {
                    self.set_edit_replacement(only, cx);
                }
                _ => {
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_CHAT_DROP_A_SINGLE_FILE_TO)));
                    cx.notify();
                }
            }
            return;
        }
        let Some(appended) = self.update_pending_attachments(|tray| {
            ComposerAttachment::append_dropped_files(tray, paths)
        }) else {
            return;
        };
        self.connection.set_note(match appended {
            Ok(count) => StatusNote::silent(trn(keys::Q_CHAT_ATTACHED_FILES_SEND_TO_UPLOAD, count)),
            Err(note) => StatusNote::error(note),
        });
        self.check_tray_videos(cx);
        cx.notify();
    }

    /// Parity slice (platform-paste-image): on Paste, if the composer has
    /// focus and the clipboard holds an image, attach it as a photo. The kit
    /// Textarea's own paste runs first on the focused element and only handles
    /// text (image-only clipboards insert "" — a no-op); this bubbled handler
    /// then adds the image. No-ops everywhere except the composer so search
    /// boxes and dialogs keep their plain text paste.
    /// Returns whether the paste was taken as attachments.
    pub(super) fn paste_image_from_clipboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.composer.read(cx).focus_handle(cx).is_focused(window) {
            return false;
        }
        // The pasteboard is read directly below, past the platform's
        // clipboard guard: refuse here too, and take the paste so the text
        // paste under it does not run.
        if quill::agent::ui_driver::guard::refuse_clipboard_if_injecting() {
            return true;
        }
        // GPUI's macOS clipboard reads only text and image data; files
        // copied in Finder are file URLs, read from the pasteboard here.
        let copied_files = super::clipboard_files::copied_file_paths();
        if !copied_files.is_empty() {
            self.attach_dropped_files(&copied_files, cx);
            return true;
        }
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        // Files copied in Finder arrive as paths: attach them like a drop.
        let paths = item.entries.iter().find_map(|entry| match entry {
            ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().to_vec()),
            _ => None,
        });
        if let Some(paths) = paths.filter(|paths| !paths.is_empty()) {
            self.attach_dropped_files(&paths, cx);
            return true;
        }
        // Linux file managers: GPUI exposes no `text/uri-list`, but the same
        // files arrive as a text list of `file://` URIs or paths.
        let listed = item.entries.iter().find_map(|entry| match entry {
            ClipboardEntry::String(text) => {
                Some(super::clipboard_files::paths_from_text(text.text()))
            }
            _ => None,
        });
        if let Some(paths) = listed.filter(|paths| !paths.is_empty()) {
            self.attach_dropped_files(&paths, cx);
            return true;
        }
        let image = item.entries.iter().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        });
        let Some(image) = image else {
            return false;
        };
        let extension = match image.format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            ImageFormat::Gif => "gif",
            ImageFormat::Svg => "svg",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Tiff => "tiff",
            ImageFormat::Ico => "ico",
            ImageFormat::Pnm => "pnm",
        };
        match quill::composer::clipboard_image_attachment(&image.bytes, extension) {
            // B5: a pasted image replaces the edited message's media.
            Some(att) if self.composer_ui.pending_edit.is_some() => {
                self.attach_dropped_files(std::slice::from_ref(&att.path), cx);
                return true;
            }
            // Joins the tray as a dropped file does: a photo next to media,
            // a file next to files.
            Some(att) => {
                let added = self.update_pending_attachments(|tray| {
                    ComposerAttachment::append_dropped_files(tray, std::slice::from_ref(&att.path))
                });
                if let Some(added) = added {
                    self.connection.set_note(match added {
                        Ok(_) => StatusNote::silent(tr_with(
                            keys::Q_CHAT_ATTACHED_NAME,
                            &[("name", &att.file_name)],
                        )),
                        Err(note) => StatusNote::error(note),
                    });
                }
            }
            None => {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_PASTE_IMAGE)));
            }
        }
        cx.notify();
        true
    }

    /// Drop one picked file; the batch options reset with the last one.
    pub(super) fn toggle_attachment_spoiler(&mut self, index: usize, cx: &mut Context<Self>) {
        if self
            .update_pending_attachment(index, |attachment| attachment.spoiler = !attachment.spoiler)
            .is_some()
        {
            cx.notify();
        }
    }

    pub(super) fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(removed) = self
            .update_pending_attachments(|tray| (index < tray.len()).then(|| tray.remove(index)))
            .flatten()
        else {
            return;
        };
        // Added again later, the file is read again.
        self.forget_tray_checks(std::slice::from_ref(&removed));
        if self.pending_attachments().is_empty() {
            self.clear_attachment(cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn clear_attachment(&mut self, cx: &mut Context<Self>) {
        self.clear_pending_attachments();
        // MED1: the grouping override belongs to this composer batch.
        self.composer_ui.group_media = None;
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_CHAT_ATTACHMENT_CLEARED)));
        cx.notify();
    }

    /// Phase B3: schema-valid self-destruct choices for
    /// `inputMessagePhoto`/`inputMessageVideo` (`messageSelfDestructType*`,
    /// schema 1.8.67 lines 5915–5918; the runtime validates the timer as
    /// 1..=60 seconds, so the picker offers only Off / 5s / 30s / 1m /
    /// View once — no `1h`/`1d`).
    pub(super) const SELF_DESTRUCT_CHOICES: [Option<SelfDestructSend>; 5] = [
        None,
        Some(SelfDestructSend::Timer(5)),
        Some(SelfDestructSend::Timer(30)),
        Some(SelfDestructSend::Timer(60)),
        Some(SelfDestructSend::Immediately),
    ];

    /// Phase B3: whether the self-destruct picker may appear — a private
    /// (1:1 cloud) chat with a photo/video attachment pending, the only
    /// combination TDLib accepts `self_destruct_type` for (schema 1.8.67
    /// lines 6117/6128 "private chats only"; the driver also strips the
    /// choice for any other chat kind as defense in depth).
    pub(super) fn self_destruct_picker_visible(&self) -> bool {
        let session = match self.session() {
            Some(session) => session,
            None => return false,
        };
        let Some(open) = session.open_chat else {
            return false;
        };
        let is_private = session
            .chats
            .get(&open.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Private { .. }));
        is_private
            && !self.pending_attachments().is_empty()
            && self
                .pending_attachments()
                .iter()
                .all(|att| matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video))
    }

    /// Phase B3: set the composer's self-destruct choice (one of
    /// `SELF_DESTRUCT_CHOICES`), picked from the timer menu.
    pub(super) fn set_composer_self_destruct(
        &mut self,
        next: Option<SelfDestructSend>,
        cx: &mut Context<Self>,
    ) {
        self.update_pending_send(|send| send.self_destruct = next);
        self.connection.set_note(match next {
            None => StatusNote::silent(tr(keys::Q_CHAT_SELF_DESTRUCT_OFF)),
            Some(SelfDestructSend::Timer(secs)) => StatusNote::silent(tr_with(
                keys::Q_CHAT_SELF_DESTRUCT_S,
                &[("secs", &secs.to_string())],
            )),
            Some(SelfDestructSend::Immediately) => {
                StatusNote::silent(tr(keys::Q_CHAT_SELF_DESTRUCT_VIEW_ONCE))
            }
        });
        cx.notify();
    }
}

impl QuillApp {
    /// The composer's `@` suggestions as `(user_id, name, username)`:
    /// group members matching the `@query` being typed.
    pub(super) fn mention_menu_items(&self, cx: &Context<Self>) -> Vec<(i64, String, String)> {
        let text = self.composer.read(cx).value().to_string();
        if self.mention_trigger(cx).is_none() {
            return Vec::new();
        }
        let Some(session) = self.session() else {
            return Vec::new();
        };
        // A lone "@" in an empty message offers the recent inline bots
        // first (tdesktop `FieldAutocomplete`, `getRecentInlineBots`).
        let mut items: Vec<(i64, String, String)> = Vec::new();
        if text.trim() == "@" {
            items.extend(
                session
                    .recent_inline_bots()
                    .iter()
                    .filter_map(|id| session.user(*id))
                    .filter(|user| !user.username.is_empty())
                    .map(|user| (user.id, user.display_name(), user.username.clone())),
            );
        }
        if let Some(search) = session
            .messages
            .mention_search
            .as_ref()
            .filter(|search| Some(search.chat_id) == session.open_chat)
        {
            for user in search.user_ids.iter().filter_map(|id| session.user(*id)) {
                if !items.iter().any(|(id, _, _)| *id == user.id) {
                    items.push((user.id, user.display_name(), user.username.clone()));
                }
            }
        }
        items.truncate(8);
        items
    }

    /// The `@name` token before the caret: its byte range and the name
    /// typed after the `@`. Not over a selection.
    pub(in crate::ui) fn mention_trigger(
        &self,
        cx: &App,
    ) -> Option<(std::ops::Range<usize>, String)> {
        let input = self.composer.read(cx);
        let caret = input.selected_range();
        if !caret.is_empty() {
            return None;
        }
        let text = input.value().to_string();
        let token = quill::composer::mention_trigger(&text, caret.start)?;
        Some((token.range, token.query.to_string()))
    }

    /// Track the `@query` before the caret: search the open group's
    /// members for it, or drop the suggestions when there is none.
    pub(super) fn sync_mention_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let query = self.mention_trigger(cx).map(|(_, query)| query);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let before = live
            .driver
            .session
            .messages
            .mention_search
            .as_ref()
            .map(|s| s.query.clone());
        let _ = live.driver.search_mentions(query.as_deref());
        if text.trim() == "@" {
            let _ = live.driver.maybe_fetch_recent_inline_bots();
        }
        if before != query {
            self.composer_ui.mention_selected = 0;
            cx.notify();
        }
    }

    /// Esc / blur: drop the suggestions. True when they were showing.
    pub(super) fn close_mention_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let showing = !self.mention_menu_items(cx).is_empty();
        if let Some(live) = self.live.as_mut() {
            live.driver.session.messages.mention_search = None;
        }
        if showing {
            cx.notify();
        }
        showing
    }

    /// Up/Down through the suggestions (wrapping). True when consumed.
    pub(super) fn step_mention_menu(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let rows = self.mention_menu_items(cx).len();
        if rows == 0 {
            return false;
        }
        self.composer_ui.mention_selected =
            (self.composer_ui.mention_selected as i32 + delta).rem_euclid(rows as i32) as usize;
        cx.notify();
        true
    }

    /// Enter / Tab: complete the highlighted suggestion. True when consumed.
    pub(super) fn pick_mention_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let items = self.mention_menu_items(cx);
        if items.is_empty() {
            return false;
        }
        let index = self.composer_ui.mention_selected.min(items.len() - 1);
        self.pick_mention_index(index, window, cx);
        true
    }

    pub(super) fn pick_mention_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((user_id, name, username)) = self.mention_menu_items(cx).into_iter().nth(index)
        else {
            return;
        };
        // A user without a username becomes a mention tag over their first
        // name (codex:composer-input), sent as a mention-name entity.
        let first = self
            .session()
            .and_then(|s| s.user(user_id))
            .map(|user| user.first_name.clone())
            .unwrap_or_default();
        let shown = super::composer_field::mention_name(&first, &name);
        self.insert_composer_mention(user_id, &shown, &username, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        if let Some(live) = self.live.as_mut() {
            live.driver.session.messages.mention_search = None;
        }
        let completed = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&completed);
        self.composer_ui.mention_selected = 0;
        cx.notify();
    }
}

mod attach_picker;
mod bar_clicks;
mod cancel_edit_confirm;
mod command_menu;
mod copy_inline_text;
mod field_placeholder;
mod send_bot_command;
mod toggle_composer_silent;

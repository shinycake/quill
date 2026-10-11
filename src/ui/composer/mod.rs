//! composer submit/send/edit/reply/scheduling, command menu state, drafts consumption.

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
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, CommandMenuItem, ComposerAttachment, ComposerEdit, ComposerReplyTo,
    ComposerScheduling, ComposerSnapshot, FormatAction, SendOptions, apply_format_markup,
    begin_edit_keeping_reply, cancel_edit_draft, cancel_edit_keeping_reply, cancel_reply_draft,
    clear_format_markup, command_menu_trigger, filter_command_menu_items,
    strip_command_menu_trigger,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::schedule::ScheduleKind;
use quill::send_rights::SendKind;
use quill::state::{Session, effective_preview};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatKind;
use quill::telegram::requests::SelfDestructSend;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
pub(super) fn apply_ready_reply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Got it — quoting you.","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]},"position":0,"is_manual":false},"checklist_task_id":0,"poll_option_id":""}}}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    /// Slice parity:platform-offline-errors — use the submitted schedule,
    /// before the composer resets its one-shot scheduling choice.
    fn send_started_note(&self, scheduling: ComposerScheduling, online_note: &str) -> String {
        let offline = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline());
        quill::composer::send_started_note(offline, scheduling, online_note).into()
    }

    pub(super) fn submit_composer(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.pane_mode() {
            PaneMode::Connecting => {
                self.connection.status_note = "sign in before sending".into();
                cx.notify();
            }
            PaneMode::Ready => {
                if self.session().is_some_and(|s| s.is_frozen()) {
                    self.connection.status_note =
                        "Your account is frozen and can't send messages.".into();
                    cx.notify();
                    return;
                }
                if self.composer_ui.pending_edit.is_some() {
                    self.submit_edit(text, window, cx);
                    return;
                }
                // B4: the forward bar sends the comment, then the forward.
                if self.forward_bar_here() {
                    self.submit_forward_bar(text, window, cx);
                    return;
                }
                self.remember_sent_hashtags(&text);
                // M2: the rich editor sends blocks, not text.
                if self.composer_ui.rich_editor_open {
                    self.submit_rich_composer(text, window, cx);
                    return;
                }
                if self.live.is_some() {
                    let plan = {
                        let session = &self.live.as_ref().expect("live").driver.session;
                        (
                            session.open_chat,
                            session.view_generation,
                            session.open_chat.and_then(|id| {
                                session.chats.get(&id.0).map(|chat| chat.supported())
                            }),
                        )
                    };
                    let (open_chat, view_generation, supported) = plan;
                    let Some(chat_id) = open_chat else {
                        self.connection.status_note = "select a chat to send".into();
                        cx.notify();
                        return;
                    };
                    if supported != Some(true) {
                        self.connection.status_note = "this chat type is not supported yet".into();
                        cx.notify();
                        return;
                    }
                    let attachments = self.composer_ui.pending_attachments.clone();
                    // The viewer's own rights in a group: text needs the
                    // basic right, each attachment its media right.
                    if attachments.is_empty() {
                        if !text.trim().is_empty() && self.deny_send(SendKind::Message, cx) {
                            return;
                        }
                    } else if self.deny_attachments(attachments.iter().map(|a| a.kind), cx) {
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
                        .then_some(self.composer_ui.self_destruct)
                        .flatten();
                    // MED1: the grouping toggle decides album vs separate
                    // sends (TGX `RememberAlbumSetting`).
                    let albumable = attachments.len() >= 2
                        && attachments.iter().all(|att| {
                            matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
                        });
                    let grouped = self.composer_group_media_effective();
                    let mut snaps = Vec::new();
                    if albumable && grouped {
                        snaps.push(
                            ComposerSnapshot::capture_album(
                                chat_id,
                                view_generation,
                                text,
                                attachments,
                            )
                            .with_reply(self.composer_ui.pending_reply.clone())
                            .with_self_destruct(self_destruct)
                            // M1: silent / scheduled / when-online /
                            // link-preview options ride the snapshot to
                            // `sendMessage.options`.
                            .with_send_options(self.composer_send_options())
                            // MED4: caption-above-media toggle.
                            .with_caption_above_media(
                                self.composer_ui.caption_above && !self.open_chat_is_secret(),
                            ),
                        );
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
                                .with_reply(if i == 0 {
                                    self.composer_ui.pending_reply.clone()
                                } else {
                                    None
                                })
                                .with_self_destruct(self_destruct)
                                .with_send_options(self.composer_send_options())
                                // MED4: caption-above-media toggle.
                                .with_caption_above_media(
                                    self.composer_ui.caption_above && !self.open_chat_is_secret(),
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
                            .with_reply(self.composer_ui.pending_reply.clone())
                            .with_self_destruct(self_destruct)
                            // M1: silent / scheduled / when-online /
                            // link-preview options ride the snapshot to
                            // `sendMessage.options`.
                            .with_send_options(self.composer_send_options())
                            // MED4: caption-above-media toggle.
                            .with_caption_above_media(
                                self.composer_ui.caption_above && !self.open_chat_is_secret(),
                            ),
                        );
                    }
                    if snaps.first().is_none_or(ComposerSnapshot::is_empty) {
                        self.connection.status_note = "type a message or attach a file".into();
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
                                self.connection.status_note = "message too long to send".into();
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
                    if self.slow_mode_blocked(chat_id, cx) {
                        return;
                    }
                    // MED1: ungrouped sends go out one message at a time.
                    let mut result = self
                        .live
                        .as_mut()
                        .expect("live")
                        .driver
                        .send_snapshot(&snaps[0]);
                    for snap in &snaps[1..] {
                        if result.is_err() {
                            break;
                        }
                        result = self.live.as_mut().expect("live").driver.send_snapshot(snap);
                    }
                    let scheduling = snaps[0].send_options.scheduling;
                    match result {
                        Ok(_) => {
                            self.composer_ui.pending_attachments.clear();
                            // MED1: the grouping override was consumed —
                            // the next composer follows the pref again.
                            self.composer_ui.group_media = None;
                            // Phase B3: the timer choice was consumed by the
                            // snapshot — reset the picker for the next send.
                            self.composer_ui.self_destruct = None;
                            // MED4: the caption-above choice was consumed
                            // too — reset for the next send.
                            self.composer_ui.caption_above = false;
                            // M1: a scheduling choice is one-shot (the next
                            // send goes immediately unless re-scheduled).
                            self.composer_ui.scheduling = ComposerScheduling::None;
                            self.composer_ui.schedule_popup_open = false;
                            self.composer_ui.pending_reply = None;
                            self.composer_ui.clear_draft_on_success = Some(chat_id);
                            self.composer
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            self.forget_local_draft(chat_id);
                            self.connection.status_note =
                                self.send_started_note(scheduling, "sending…");
                        }
                        Err(quill::connect::ConnectSendError::CaptionTooLong { limit }) => {
                            // MED4: runtime `message_caption_length_max`
                            // refusal — the counter already warned; this
                            // names the limit.
                            self.connection.status_note =
                                format!("caption too long (max {limit} characters)");
                        }
                        Err(_) => {
                            let video_unreadable = snaps
                                .iter()
                                .flat_map(|snap| snap.attachment.iter().chain(snap.album.iter()))
                                .any(|att| {
                                    att.kind == AttachmentKind::Video
                                        && quill::video::probe_local_video(&att.path).is_err()
                                });
                            let note_unreadable = snaps
                                .iter()
                                .filter_map(|snap| snap.attachment.as_ref())
                                .any(|att| {
                                    att.kind == AttachmentKind::VideoNote
                                        && quill::video::probe_local_video_note(&att.path).is_err()
                                });
                            self.connection.status_note = if note_unreadable {
                                "video note must be a square clip (max 60s, 640px)".into()
                            } else if video_unreadable {
                                "could not read video duration or size".into()
                            } else {
                                "could not send message".into()
                            };
                        }
                    }
                    cx.notify();
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
                    let attachments = self.composer_ui.pending_attachments.clone();
                    if attachments.iter().any(|att| {
                        att.kind == AttachmentKind::VideoNote
                            && quill::video::probe_local_video_note(&att.path).is_err()
                    }) {
                        self.connection.status_note =
                            "video note must be a square clip (max 60s, 640px)".into();
                        cx.notify();
                        return;
                    }
                    let reply = self.composer_ui.pending_reply.clone();
                    // MED1: the grouping toggle applies to the demo too.
                    let albumable = attachments.len() >= 2
                        && attachments.iter().all(|att| {
                            matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
                        });
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
                    self.composer_ui.pending_attachments.clear();
                    self.composer_ui.group_media = None;
                    self.composer_ui.pending_reply = None;
                    if let Some(chat_id) = self.demo_session.as_ref().and_then(|s| s.open_chat) {
                        self.forget_local_draft(chat_id);
                    }
                    self.composer
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.connection.status_note =
                        "demo send applied locally (no live Telegram)".into();
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
        let (open_chat, supported) = {
            let session = &self.live.as_ref().expect("live").driver.session;
            (
                session.open_chat,
                session
                    .open_chat
                    .and_then(|id| session.chats.get(&id.0).map(|chat| chat.supported())),
            )
        };
        // Premium gate (`premiumFeatureRichMessages`): the editor can only
        // be opened by Premium users, but double-check here too — the
        // server would reject `inputMessageRichMessage` from a non-Premium
        // user and we never want to fake a successful send.
        let premium = self
            .live
            .as_ref()
            .expect("live")
            .driver
            .session
            .my_is_premium();
        if !premium {
            self.connection.status_note = "Rich messages require Telegram Premium".into();
            cx.notify();
            return;
        }
        let Some(chat_id) = open_chat else {
            self.connection.status_note = "select a chat to send".into();
            cx.notify();
            return;
        };
        if supported != Some(true) {
            self.connection.status_note = "this chat type is not supported yet".into();
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
            .composer_ui
            .pending_attachments
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
        for attachment in &self.composer_ui.pending_attachments {
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
            self.connection.status_note = format!(
                "rich message too long (max {} characters)",
                quill::rich::RICH_TEXT_MAX_CHARS
            );
            cx.notify();
            return;
        }
        if quill::rich::input_rich_message(&blocks).is_none() {
            self.connection.status_note = "type a message or attach a file".into();
            cx.notify();
            return;
        }
        let reply_to = self
            .composer_ui
            .pending_reply
            .as_ref()
            .and_then(|reply| reply.send_reply(chat_id));
        let options = self.composer_send_options();
        match self
            .live
            .as_mut()
            .expect("live")
            .driver
            .send_rich_snapshot(chat_id, &blocks, reply_to, &options)
        {
            Ok(_) => {
                self.composer_ui.pending_attachments.clear();
                // M1: a scheduling choice is one-shot.
                self.composer_ui.scheduling = ComposerScheduling::None;
                self.composer_ui.schedule_popup_open = false;
                self.composer_ui.pending_reply = None;
                self.composer_ui.rich_editor_open = false;
                self.composer_ui.clear_draft_on_success = Some(chat_id);
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.forget_local_draft(chat_id);
                self.connection.status_note =
                    self.send_started_note(options.scheduling, "sending…");
            }
            Err(_) => {
                self.connection.status_note = "could not send rich message".into();
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
                    self.connection.status_note =
                        "Drop a single file to replace the attachment.".into();
                    cx.notify();
                }
            }
            return;
        }
        self.connection.status_note = match ComposerAttachment::append_dropped_files(
            &mut self.composer_ui.pending_attachments,
            paths,
        ) {
            Ok(count) => format!("Attached {count} files. Send to upload."),
            Err(note) => note.into(),
        };
        cx.notify();
    }

    /// The attach menu's "Photo or video" / "File": Telegram Desktop's
    /// "Choose Files" dialog. Picked photos and videos join the album;
    /// anything else, or everything when `as_files`, goes as files.
    /// `QUILL_ATTACH_PHOTO` / `QUILL_ATTACH_FILE` skip the dialog for live
    /// testing; the offline demo attaches its fixtures instead.
    pub(super) fn pick_attachments(&mut self, as_files: bool, cx: &mut Context<Self>) {
        let env_key = if as_files {
            "QUILL_ATTACH_FILE"
        } else {
            "QUILL_ATTACH_PHOTO"
        };
        let preset = std::env::var_os(env_key).map(PathBuf::from).or_else(|| {
            (self.live.is_none() && self.demo_session.is_some()).then(|| {
                demo_media_allowlist().join(if as_files {
                    "demo-notes.txt"
                } else {
                    "demo-thumb.png"
                })
            })
        });
        if let Some(path) = preset {
            self.attach_picked(vec![path], as_files, cx);
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Choose Files".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await {
                let _ = this.update(cx, |this, cx| this.attach_picked(paths, as_files, cx));
            }
        })
        .detach();
    }

    fn attach_picked(&mut self, paths: Vec<PathBuf>, as_files: bool, cx: &mut Context<Self>) {
        self.attach_dropped_files(&paths, cx);
        if as_files {
            ComposerAttachment::set_send_as_files(&mut self.composer_ui.pending_attachments, true);
        }
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
            Some(att) => {
                let name = att.file_name.clone();
                let before = self.composer_ui.pending_attachments.len();
                ComposerAttachment::push_attachment(&mut self.composer_ui.pending_attachments, att);
                self.connection.status_note =
                    if self.composer_ui.pending_attachments.len() == before {
                        format!("album is full ({before})")
                    } else {
                        format!("attached {name}")
                    };
            }
            None => {
                self.connection.status_note = "could not paste image".into();
            }
        }
        cx.notify();
        true
    }

    /// Drop one picked file; the batch options reset with the last one.
    pub(super) fn toggle_attachment_spoiler(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(attachment) = self.composer_ui.pending_attachments.get_mut(index) {
            attachment.spoiler = !attachment.spoiler;
            cx.notify();
        }
    }

    pub(super) fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.composer_ui.pending_attachments.len() {
            return;
        }
        self.composer_ui.pending_attachments.remove(index);
        if self.composer_ui.pending_attachments.is_empty() {
            self.clear_attachment(cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn clear_attachment(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.pending_attachments.clear();
        self.composer_ui.self_destruct = None;
        // MED1: the grouping override belongs to this composer batch.
        self.composer_ui.group_media = None;
        self.connection.status_note = "attachment cleared".into();
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
            && !self.composer_ui.pending_attachments.is_empty()
            && self
                .composer_ui
                .pending_attachments
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
        self.composer_ui.self_destruct = next;
        self.connection.status_note = match next {
            None => "self-destruct off".into(),
            Some(SelfDestructSend::Timer(secs)) => format!("self-destruct: {secs}s"),
            Some(SelfDestructSend::Immediately) => "self-destruct: view once".into(),
        };
        cx.notify();
    }

    /// Silent send as the next message will go: the manual toggle, or the
    /// chat's `default_disable_notification` unless the user turned that
    /// off for this chat.
    pub(super) fn composer_effective_silent(&self) -> bool {
        let chat = self.session().and_then(|s| s.open_chat).map(|c| c.0);
        let chat_default =
            chat.is_some_and(|id| self.session().is_some_and(|s| s.sync.is_default_silent(id)));
        quill::state::effective_silent(
            self.composer_ui.silent,
            chat_default,
            chat.is_some() && self.composer_ui.loud_chat == chat,
        )
    }
}

impl QuillApp {
    /// The composer's `@` suggestions as `(user_id, name, username)`:
    /// group members matching the `@query` being typed.
    pub(super) fn mention_menu_items(&self, cx: &Context<Self>) -> Vec<(i64, String, String)> {
        let text = self.composer.read(cx).value().to_string();
        if quill::composer::mention_trigger(&text).is_none() {
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

    /// Track the `@query` at the composer's end: search the open group's
    /// members for it, or drop the suggestions when there is none.
    pub(super) fn sync_mention_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let query = quill::composer::mention_trigger(&text).map(str::to_string);
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

mod copy_inline_text;
mod toggle_composer_silent;

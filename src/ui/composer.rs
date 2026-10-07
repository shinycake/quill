//! composer submit/send/edit/reply/scheduling, command menu state, drafts consumption.

use super::app::{PaneMode, QuillApp};
use super::demo::demo_media_allowlist;
use super::message_text::rich_block_element;
use super::scheduled::format_schedule_delay;
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
use quill::state::{Session, effective_preview, unix_ms_now};
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
        // parity:platform-spellcheck: the draft is going away — session
        // ignores don't carry to the next draft.
        self.spellchecker.clear_ignored();
        self.spellcheck_open = false;
        match self.pane_mode() {
            PaneMode::Connecting => {
                self.status_note = "sign in before sending".into();
                cx.notify();
            }
            PaneMode::Ready => {
                if self.pending_edit.is_some() {
                    self.submit_edit(text, window, cx);
                    return;
                }
                // M2: the rich editor sends blocks, not text.
                if self.rich_editor_open {
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
                        self.status_note = "select a chat to send".into();
                        cx.notify();
                        return;
                    };
                    if supported != Some(true) {
                        self.status_note = "this chat type is not supported yet".into();
                        cx.notify();
                        return;
                    }
                    let attachments = self.pending_attachments.clone();
                    // Phase B3: self-destruct only leaves the composer on
                    // photo/video attachments; the driver additionally
                    // strips it for non-private chats (TDLib's 400 gate).
                    let self_destruct = attachments
                        .iter()
                        .all(|att| {
                            matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
                        })
                        .then_some(self.composer_self_destruct)
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
                            .with_reply(self.pending_reply.clone())
                            .with_self_destruct(self_destruct)
                            // M1: silent / scheduled / when-online /
                            // link-preview options ride the snapshot to
                            // `sendMessage.options`.
                            .with_send_options(self.composer_send_options())
                            // MED4: caption-above-media toggle.
                            .with_caption_above_media(
                                self.composer_caption_above && !self.open_chat_is_secret(),
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
                                    self.pending_reply.clone()
                                } else {
                                    None
                                })
                                .with_self_destruct(self_destruct)
                                .with_send_options(self.composer_send_options())
                                // MED4: caption-above-media toggle.
                                .with_caption_above_media(
                                    self.composer_caption_above && !self.open_chat_is_secret(),
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
                            .with_reply(self.pending_reply.clone())
                            .with_self_destruct(self_destruct)
                            // M1: silent / scheduled / when-online /
                            // link-preview options ride the snapshot to
                            // `sendMessage.options`.
                            .with_send_options(self.composer_send_options())
                            // MED4: caption-above-media toggle.
                            .with_caption_above_media(
                                self.composer_caption_above && !self.open_chat_is_secret(),
                            ),
                        );
                    }
                    if snaps.first().is_none_or(ComposerSnapshot::is_empty) {
                        self.status_note = "type a message or attach a file".into();
                        cx.notify();
                        return;
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
                            self.pending_attachments.clear();
                            // MED1: the grouping override was consumed —
                            // the next composer follows the pref again.
                            self.composer_group_media = None;
                            // Phase B3: the timer choice was consumed by the
                            // snapshot — reset the picker for the next send.
                            self.composer_self_destruct = None;
                            // MED4: the caption-above choice was consumed
                            // too — reset for the next send.
                            self.composer_caption_above = false;
                            // M1: a scheduling choice is one-shot (the next
                            // send goes immediately unless re-scheduled).
                            self.composer_scheduling = ComposerScheduling::None;
                            self.schedule_popup_open = false;
                            self.pending_reply = None;
                            self.clear_draft_on_success = Some(chat_id);
                            self.composer
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            self.forget_local_draft(chat_id);
                            self.status_note = self.send_started_note(scheduling, "sending…");
                        }
                        Err(quill::connect::ConnectSendError::CaptionTooLong { limit }) => {
                            // MED4: runtime `message_caption_length_max`
                            // refusal — the counter already warned; this
                            // names the limit.
                            self.status_note =
                                format!("caption too long (max {limit} characters)").into();
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
                            self.status_note = if note_unreadable {
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
                    let attachments = self.pending_attachments.clone();
                    if attachments.iter().any(|att| {
                        att.kind == AttachmentKind::VideoNote
                            && quill::video::probe_local_video_note(&att.path).is_err()
                    }) {
                        self.status_note =
                            "video note must be a square clip (max 60s, 640px)".into();
                        cx.notify();
                        return;
                    }
                    let reply = self.pending_reply.clone();
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
                    self.pending_attachments.clear();
                    self.composer_group_media = None;
                    self.pending_reply = None;
                    if let Some(chat_id) = self.demo_session.as_ref().and_then(|s| s.open_chat) {
                        self.forget_local_draft(chat_id);
                    }
                    self.composer
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.status_note = "demo send applied locally (no live Telegram)".into();
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
            self.status_note = "Rich messages require Telegram Premium".into();
            cx.notify();
            return;
        }
        let Some(chat_id) = open_chat else {
            self.status_note = "select a chat to send".into();
            cx.notify();
            return;
        };
        if supported != Some(true) {
            self.status_note = "this chat type is not supported yet".into();
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
        for attachment in &self.pending_attachments {
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
            self.status_note = format!(
                "rich message too long (max {} characters)",
                quill::rich::RICH_TEXT_MAX_CHARS
            )
            .into();
            cx.notify();
            return;
        }
        if quill::rich::input_rich_message(&blocks).is_none() {
            self.status_note = "type a message or attach a file".into();
            cx.notify();
            return;
        }
        let reply_to = self
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
                self.pending_attachments.clear();
                // M1: a scheduling choice is one-shot.
                self.composer_scheduling = ComposerScheduling::None;
                self.schedule_popup_open = false;
                self.pending_reply = None;
                self.rich_editor_open = false;
                self.clear_draft_on_success = Some(chat_id);
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.forget_local_draft(chat_id);
                self.status_note = self.send_started_note(options.scheduling, "sending…");
            }
            Err(_) => {
                self.status_note = "could not send rich message".into();
            }
        }
        cx.notify();
    }

    pub(super) fn attach_dropped_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        self.status_note =
            match ComposerAttachment::append_dropped_files(&mut self.pending_attachments, paths) {
                Ok(count) => format!("Attached {count} files. Send to upload."),
                Err(note) => note.into(),
            };
        cx.notify();
    }

    pub(super) fn attach_local(&mut self, kind: AttachmentKind, cx: &mut Context<Self>) {
        // Phase S1: round video notes need secret-chat layer ≥ 66 (TGX
        // `chatSupportsRoundVideos`); an older peer client gets the
        // `SecretChatFeatureUnsupported` notice instead of a broken send.
        if matches!(kind, AttachmentKind::VideoNote)
            && let Some((name, layer)) = self.open_secret_chat_peer_layer()
            && layer < 66
        {
            self.status_note = format!(
                "{name}'s Telegram client doesn't support this feature. \
                 They need to install an update first."
            );
            cx.notify();
            return;
        }
        // Explicit user action → pick. Prefer QUILL_ATTACH_PHOTO / QUILL_ATTACH_FILE /
        // QUILL_ATTACH_VIDEO when set (live testing); otherwise the demo fixtures
        // under docs/screenshots.
        // Never read paths from TDLib JSON for send.
        let env_key = match kind {
            AttachmentKind::Photo => "QUILL_ATTACH_PHOTO",
            AttachmentKind::Document => "QUILL_ATTACH_FILE",
            AttachmentKind::Video => "QUILL_ATTACH_VIDEO",
            AttachmentKind::VideoNote => "QUILL_ATTACH_VIDEO_NOTE",
        };
        let path = std::env::var_os(env_key)
            .map(PathBuf::from)
            .unwrap_or_else(|| match kind {
                AttachmentKind::Photo => demo_media_allowlist().join("demo-thumb.png"),
                AttachmentKind::Document => demo_media_allowlist().join("demo-notes.txt"),
                AttachmentKind::Video => demo_media_allowlist().join("demo-clip.mp4"),
                AttachmentKind::VideoNote => demo_media_allowlist().join("demo-video-note.mp4"),
            });
        match ComposerAttachment::pick(&path, kind) {
            Some(att) => {
                let name = att.file_name.clone();
                let before = self.pending_attachments.len();
                ComposerAttachment::push_attachment(&mut self.pending_attachments, att);
                self.status_note = if self.pending_attachments.len() == before {
                    format!("album is full ({before})")
                } else {
                    format!("attached {name}")
                };
            }
            None => {
                self.status_note = "could not attach file".into();
            }
        }
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
            Some(att) => {
                let name = att.file_name.clone();
                let before = self.pending_attachments.len();
                ComposerAttachment::push_attachment(&mut self.pending_attachments, att);
                self.status_note = if self.pending_attachments.len() == before {
                    format!("album is full ({before})")
                } else {
                    format!("attached {name}")
                };
            }
            None => {
                self.status_note = "could not paste image".into();
            }
        }
        cx.notify();
        true
    }

    /// Drop one picked file; the batch options reset with the last one.
    pub(super) fn toggle_attachment_spoiler(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(attachment) = self.pending_attachments.get_mut(index) {
            attachment.spoiler = !attachment.spoiler;
            cx.notify();
        }
    }

    pub(super) fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.pending_attachments.len() {
            return;
        }
        self.pending_attachments.remove(index);
        if self.pending_attachments.is_empty() {
            self.clear_attachment(cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn clear_attachment(&mut self, cx: &mut Context<Self>) {
        self.pending_attachments.clear();
        self.composer_self_destruct = None;
        // MED1: the grouping override belongs to this composer batch.
        self.composer_group_media = None;
        self.status_note = "attachment cleared".into();
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
            && !self.pending_attachments.is_empty()
            && self
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
        self.composer_self_destruct = next;
        self.status_note = match next {
            None => "self-destruct off".into(),
            Some(SelfDestructSend::Timer(secs)) => format!("self-destruct: {secs}s"),
            Some(SelfDestructSend::Immediately) => "self-destruct: view once".into(),
        };
        cx.notify();
    }

    /// M1: the composer's `messageSendOptions` for the next send.
    pub(super) fn composer_send_options(&self) -> SendOptions {
        SendOptions {
            disable_notification: self.composer_silent,
            scheduling: self.composer_scheduling,
            link_preview_disabled: self.composer_preview_disabled,
            link_preview_above_text: self.composer_preview_above,
            link_preview_media: self.composer_preview_media,
            // The driver overrides this for secret chats at send time.
            is_secret: false,
            ..SendOptions::default()
        }
    }

    /// M1: apply a formatting action to the composer selection (or insert
    /// the marker pair at the cursor when the selection is empty).
    pub(super) fn apply_composer_format(
        &mut self,
        action: FormatAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let (new_text, new_selection) = apply_format_markup(&text, range, &action);
        self.composer.update(cx, |input, cx| {
            input.set_value(&new_text, window, cx);
            input.set_selected_range(new_selection, cx);
        });
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// M1: strip formatting markers in the composer selection (whole text
    /// when the selection is empty), keeping the inner text.
    pub(super) fn clear_composer_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let new_text = clear_format_markup(&text, range);
        self.composer.update(cx, |input, cx| {
            input.set_value(&new_text, window, cx);
        });
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// M1: schedule the next send `secs` from now
    /// (`messageSchedulingStateSendAtDate`; `repeat_period` stays 0 —
    /// premium-only, never surfaced).
    pub(super) fn schedule_send_in(&mut self, secs: i64, cx: &mut Context<Self>) {
        let send_date = unix_ms_now() as i64 / 1000 + secs;
        self.composer_scheduling = ComposerScheduling::SendAtDate(send_date);
        self.schedule_popup_open = false;
        self.status_note = format!("scheduled in {}", format_schedule_delay(secs));
        cx.notify();
    }

    /// M1: load the chat's scheduled sends and open the dialog.
    pub(super) fn open_scheduled_dialog(&mut self, cx: &mut Context<Self>) {
        self.schedule_popup_open = false;
        if let Some(live) = self.live.as_mut() {
            if let Some(chat_id) = live.driver.session.open_chat {
                match live.driver.get_chat_scheduled_messages(chat_id) {
                    Ok(_) => self.status_note = "loading scheduled messages…".into(),
                    Err(_) => self.status_note = "could not load scheduled messages".into(),
                }
            }
        }
        self.scheduled_dialog_open = true;
        cx.notify();
    }

    /// M1: delete a scheduled send (`deleteMessages`, revoke false —
    /// scheduled messages are not in history, so this goes through the
    /// driver's scheduled-message path, not the confirm dialog).
    pub(super) fn delete_scheduled_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(chat_id) = live.driver.session.open_chat else {
            return;
        };
        match live.driver.delete_scheduled_message(chat_id, message_id) {
            Ok(_) => {
                live.driver
                    .session
                    .scheduled_messages
                    .retain(|m| m.id != message_id);
                self.status_note = "scheduled message deleted".into();
            }
            Err(_) => self.status_note = "could not delete scheduled message".into(),
        }
        cx.notify();
    }

    /// M1: the composer's formatting menu (Telegram X `InputView` format
    /// menu / tdesktop markdown behavior): bold, italic, underline,
    /// strikethrough, inline code, code block, spoiler, quote, link, and
    /// clear-formatting. Formatting applies to the textarea selection via
    /// `apply_format_markup`; the send path converts markup to TDLib
    /// `textEntities` (`parse_format_markup`).
    pub(super) fn format_menu_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        Button::new("composer-format-menu")
            .icon(IconName::ALargeSmall)
            .ghost()
            .tooltip("Formatting")
            .accessibility_label("Formatting")
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, _, _| {
                for (name, action) in [
                    ("Bold", FormatAction::Bold),
                    ("Italic", FormatAction::Italic),
                    ("Underline", FormatAction::Underline),
                    ("Strikethrough", FormatAction::Strikethrough),
                    ("Inline code", FormatAction::Code),
                    ("Code block", FormatAction::Pre),
                    ("Spoiler", FormatAction::Spoiler),
                    ("Block quote", FormatAction::BlockQuote),
                    ("Insert link", FormatAction::Link(String::new())),
                ] {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(name).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.apply_composer_format(action.clone(), window, cx)
                        });
                    }));
                }
                let owner = owner.clone();
                menu.separator()
                    .item(
                        PopupMenuItem::new("Clear formatting").on_click(move |_, window, cx| {
                            let _ =
                                owner.update(cx, |this, cx| this.clear_composer_format(window, cx));
                        }),
                    )
            })
    }

    /// Send options for the next message (right-click on Send, like the
    /// official desktop client): silent send, scheduling, link previews.
    pub(super) fn send_options_menu(
        owner: WeakEntity<Self>,
        menu: gpui_kit::component::menu::PopupMenu,
        cx: &App,
    ) -> gpui_kit::component::menu::PopupMenu {
        let Some(app) = owner.upgrade() else {
            return menu;
        };
        let (silent, preview_off, scheduled) = {
            let app = app.read(cx);
            (
                app.composer_silent,
                app.composer_preview_disabled,
                !matches!(app.composer_scheduling, ComposerScheduling::None),
            )
        };
        let toggle_silent = owner.clone();
        let schedule = owner.clone();
        let toggle_preview = owner;
        menu.item(
            PopupMenuItem::new("Send without sound")
                .checked(silent)
                .on_click(move |_, _, cx| {
                    let _ = toggle_silent.update(cx, |this, cx| {
                        this.composer_silent = !this.composer_silent;
                        cx.notify();
                    });
                }),
        )
        .item(
            PopupMenuItem::new(if scheduled {
                "Change schedule…"
            } else {
                "Schedule message…"
            })
            .on_click(move |_, _, cx| {
                let _ = schedule.update(cx, |this, cx| {
                    this.schedule_popup_open = true;
                    cx.notify();
                });
            }),
        )
        .item(
            PopupMenuItem::new("Link preview")
                .checked(!preview_off)
                .on_click(move |_, _, cx| {
                    let _ = toggle_preview.update(cx, |this, cx| {
                        this.composer_preview_disabled = !this.composer_preview_disabled;
                        cx.notify();
                    });
                }),
        )
    }

    /// Chips for send options that differ from the default (silent,
    /// scheduled, previews off) — visible state for an otherwise hidden
    /// menu, each clearable in place — plus the rich-editor entry point.
    /// `None` when there is nothing to show, so the composer stays one row.
    pub(super) fn composer_options_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let chip = |id: &'static str, label: String, cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_1()
                .pl_2()
                .pr_1()
                .h(px(22.))
                .rounded_full()
                .bg(cx.theme().secondary)
                .text_xs()
                .text_color(cx.theme().secondary_foreground)
                .child(label)
        };
        let mut row = div()
            .id("composer-options")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .px_1()
            .pb_1();
        let mut any = false;
        if self.composer_silent {
            any = true;
            row = row.child(
                chip("chip-silent", "Silent".into(), cx).child(
                    Button::new("chip-silent-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .xsmall()
                        .ghost()
                        .accessibility_label("Send with sound")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.composer_silent = false;
                            cx.notify();
                        })),
                ),
            );
        }
        let schedule_label = match self.composer_scheduling {
            ComposerScheduling::None => None,
            ComposerScheduling::SendAtDate(date) => Some(format!(
                "Scheduled · {}",
                super::message_text::format_unix_date_time(i64::from(date))
            )),
            ComposerScheduling::SendWhenOnline => Some("When online".to_string()),
        };
        if let Some(label) = schedule_label {
            any = true;
            row = row.child(
                chip("chip-schedule", label, cx)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.schedule_popup_open = !this.schedule_popup_open;
                        cx.notify();
                    }))
                    .child(
                        Button::new("chip-schedule-clear")
                            .icon(gpui_kit::assets::IconName::X)
                            .xsmall()
                            .ghost()
                            .accessibility_label("Send now")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.composer_scheduling = ComposerScheduling::None;
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    ),
            );
        }
        if self.composer_preview_disabled {
            any = true;
            row = row.child(
                chip("chip-preview", "No link preview".into(), cx).child(
                    Button::new("chip-preview-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .xsmall()
                        .ghost()
                        .accessibility_label("Show link preview")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.composer_preview_disabled = false;
                            cx.notify();
                        })),
                ),
            );
        }
        // M2: the rich editor opens via ⛶ after typing more than 3 lines
        // (anniversary post). The button hides again while the editor is
        // open (a ✕ close button takes its place in the editor bar).
        // Premium gate (`premiumFeatureRichMessages`, schema 1.8.67 line
        // 8160 — "The ability to send rich messages"): non-Premium users
        // get the button but tapping it explains the requirement instead
        // of opening the editor.
        if !self.rich_editor_open && self.composer.read(cx).value().lines().count() > 3 {
            row = row.child(
                Button::new("rich-editor-open")
                    .label("⛶ Rich editor")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let premium = this
                            .live
                            .as_ref()
                            .is_some_and(|live| live.driver.session.my_is_premium());
                        if premium {
                            this.rich_editor_open = true;
                            this.status_note = "rich editor — markup becomes blocks".into();
                        } else {
                            this.status_note = "Rich messages require Telegram Premium".into();
                        }
                        cx.notify();
                    })),
            );
            any = true;
        }
        any.then(|| row.into_any_element())
    }

    /// Slice msg-richtext-ai-tools: run one AI action against the open
    /// chat's composer draft. `send` issues the driver request; the
    /// answer (or a TDLib error) lands through the session drain —
    /// never silent, never fake success.
    fn run_ai_composer_action(
        &mut self,
        cx: &mut Context<Self>,
        working_note: &str,
        send: impl FnOnce(
            &mut quill::connect::LiveConnect,
            ChatId,
            &str,
        ) -> Result<quill::ids::RequestId, quill::connect::ConnectSendError>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        if text.trim().is_empty() {
            self.status_note = "type something first — the AI works on the draft".into();
        } else if let Some(live) = self.live.as_mut()
            && let Some(chat_id) = live.driver.session.open_chat
        {
            self.status_note = match send(live, chat_id, &text) {
                Ok(_) => working_note.into(),
                Err(_) => "AI tools unavailable here".into(),
            };
        } else {
            self.status_note = "AI tools unavailable here".into();
        }
        cx.notify();
    }

    /// M2: rich editor bar — block buttons append markup templates to the
    /// composer text; below them a live preview renders the parsed blocks
    /// with the same block renderer as history. The ✕ button closes the
    /// editor (the text stays, so nothing is lost).
    pub(super) fn rich_editor_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut buttons = div()
            .id("rich-editor-blocks")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1();
        for (id, label, template) in [
            ("rich-block-h1", "H1", "# "),
            ("rich-block-h2", "H2", "## "),
            ("rich-block-list", "\u{2022} List", "- "),
            ("rich-block-check", "\u{2611} Check", "[] "),
            ("rich-block-details", "\u{25be} Details", ">> "),
            ("rich-block-divider", "\u{2014} Divider", "---\n"),
        ] {
            buttons = buttons.child(Button::new(id).label(label).ghost().on_click(cx.listener(
                move |this, _, window, cx| {
                    this.composer.update(cx, |input, cx| {
                        let mut value = input.value().to_string();
                        if !value.is_empty() && !value.ends_with('\n') {
                            value.push('\n');
                        }
                        value.push_str(template);
                        input.set_value(&value, window, cx);
                    });
                    cx.notify();
                },
            )));
        }
        // Slice msg-richtext-ai-tools: AI actions on the draft. "Fix"
        // runs `fixTextWithAi` (replaces the draft with the fixed text);
        // "Rewrite" runs `composeTextWithAi` with the honest defaults
        // (no translation, current style, no emoji); "Fix rich" and
        // "Rewrite rich" parse the draft to blocks with the same markup
        // parser as the preview and run `fixRichMessageWithAi` /
        // `composeRichMessageWithAi` on them; "Create" treats the
        // draft as the prompt for `createRichMessageWithAi` and the
        // created blocks replace the draft.
        buttons = buttons.child(Button::new("rich-ai-fix").label("✨ Fix").ghost().on_click(
            cx.listener(move |this, _, _, cx| {
                this.run_ai_composer_action(cx, "AI fixing the text…", |live, chat_id, text| {
                    live.driver.fix_text_with_ai(chat_id, text)
                });
            }),
        ));
        buttons = buttons.child(
            Button::new("rich-ai-rewrite")
                .label("✨ Rewrite")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(cx, "AI rewriting…", |live, chat_id, text| {
                        live.driver.compose_text_with_ai(chat_id, text)
                    });
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-create")
                .label("✨ Create")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        "AI creating from the prompt…",
                        |live, chat_id, text| {
                            live.driver.create_rich_message_with_ai(chat_id, text)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-fix-rich")
                .label("✨ Fix rich")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        "AI fixing the blocks…",
                        |live, chat_id, text| {
                            let blocks = quill::rich::preview_blocks(text);
                            live.driver.fix_rich_message_with_ai(chat_id, &blocks)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-rewrite-rich")
                .label("✨ Rewrite rich")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        "AI rewriting the blocks…",
                        |live, chat_id, text| {
                            let blocks = quill::rich::preview_blocks(text);
                            live.driver.compose_rich_message_with_ai(chat_id, &blocks)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-editor-close")
                .label("\u{2715}")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.rich_editor_open = false;
                    cx.notify();
                })),
        );
        let mut bar = div()
            .id("rich-editor-bar")
            .flex()
            .flex_col()
            .gap_1()
            .px_1()
            .py_1()
            .child(buttons);
        // Live preview of the parsed blocks (editor blocks never carry
        // buttons, so the callback ids are unused).
        let text = self.composer.read(cx).value().to_string();
        let blocks = quill::rich::preview_blocks(&text);
        if blocks.iter().any(|block| {
            !matches!(
                block,
                quill::rich::RichBlock::Empty | quill::rich::RichBlock::Unsupported { .. }
            )
        }) {
            let empty: std::collections::HashSet<(i64, u64, u64, bool)> =
                std::collections::HashSet::new();
            let mut preview = div()
                .id("rich-editor-preview")
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(bg_canvas())
                // Cap the preview so a long draft can't squeeze the history
                // to zero height on short windows.
                .max_h(px(320.))
                .overflow_y_scroll();
            for (index, block) in blocks.iter().enumerate() {
                if let Some(child) = rich_block_element(
                    index,
                    block,
                    (0, 0),
                    ChatId(0),
                    MessageId(0),
                    &empty,
                    // Settings → Appearance: message font size.
                    self.msg_font(),
                    cx,
                ) {
                    preview = preview.child(child);
                }
            }
            bar = bar.child(preview);
        }
        bar
    }

    /// M1: schedule picker popup above the composer (duration presets +
    /// send-when-online, mirroring tdesktop's "Schedule message" options).
    /// M1 fix-up: "When contact comes online" is offered only in private
    /// (1:1) chats — `messageSchedulingStateSendWhenOnline` is
    /// private-chats-only (schema 1.8.67 line 5905).
    pub(super) fn schedule_popup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("schedule-popup")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(accent())
                    .child("Schedule message"),
            );
        for (id, label, secs) in [
            ("schedule-1h", "In 1 hour", 3600),
            ("schedule-8h", "In 8 hours", 8 * 3600),
            ("schedule-24h", "In 24 hours", 24 * 3600),
        ] {
            panel = panel.child(Button::new(id).label(label).ghost().on_click(cx.listener(
                move |this, _, _, cx| {
                    this.schedule_send_in(secs, cx);
                },
            )));
        }
        // M1 fix-up: private chats only (see `open_chat_is_private`).
        if self.open_chat_is_private() {
            panel = panel.child(
                Button::new("schedule-when-online")
                    .label("When contact comes online")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.composer_scheduling = ComposerScheduling::SendWhenOnline;
                        this.schedule_popup_open = false;
                        this.status_note = "will send when the contact is online".into();
                        cx.notify();
                    })),
            );
        }
        panel = panel
            .child(
                Button::new("schedule-clear")
                    .label("Send now (clear schedule)")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.composer_scheduling = ComposerScheduling::None;
                        this.schedule_popup_open = false;
                        this.status_note = "schedule cleared".into();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("schedule-view")
                    .label("View scheduled messages")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_scheduled_dialog(cx);
                    })),
            );
        panel
    }

    /// M1: start a reply to a message from the context menu — the reply
    /// header targets the message; the typed text stays untouched.
    pub(super) fn begin_reply_from_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preview = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .map(|message| effective_preview(message))
            .unwrap_or_default();
        self.begin_reply_to(
            ComposerReplyTo::new(chat_id, message_id, preview),
            window,
            cx,
        );
    }

    /// M1: unpin every pinned message in the chat (`unpinAllChatMessages`,
    /// TDLib 1.8.67, `schema/td_api.tl:13565`) — the pinned-bar "Unpin
    /// all" action.
    pub(super) fn unpin_all_messages(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.unpin_all_chat_messages(chat_id) {
                Ok(_) => "unpinning all…".into(),
                Err(_) => "could not unpin all".into(),
            };
        } else {
            self.status_note = "no live connection".into();
        }
        cx.notify();
    }

    /// M1: retry a failed send (`resendMessages`, TDLib 1.8.67,
    /// `schema/td_api.tl:12251`). Offered only for rows the reducer
    /// marked `failed` **and** `can_retry` — TDLib does not allow every
    /// failed send to be retried. A failed `resendMessages` surfaces in
    /// the status note via `Session::resend_error`.
    pub(super) fn retry_failed_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.resend_failed_message(chat_id, message_id) {
                Ok(_) => self.send_started_note(ComposerScheduling::None, "retrying send…"),
                Err(_) => "could not retry".into(),
            };
        } else {
            self.status_note = "no live connection".into();
        }
        cx.notify();
    }

    /// M1: copy a public share link for a message (`getMessageLink`,
    /// TDLib 1.8.67, `schema/td_api.tl:12064`). The parsed
    /// `messageLink.link` lands in `session.message_link_result`; the
    /// per-frame pump copies it to the clipboard.
    pub(super) fn share_message_link(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.get_message_link(chat_id, message_id) {
                Ok(_) => "fetching message link…".into(),
                Err(_) => "could not get message link".into(),
            };
        } else {
            self.status_note = "no live connection".into();
        }
        cx.notify();
    }

    /// Phase B3: label for the picker button (`⏱` cycle affordance).
    pub(super) fn self_destruct_button_label(&self) -> String {
        match self.composer_self_destruct {
            None => "Off".to_string(),
            Some(SelfDestructSend::Timer(secs)) => format!("{secs}s"),
            Some(SelfDestructSend::Immediately) => "View once".to_string(),
        }
    }

    /// Phase 3.3: recompute the `/` command menu from the composer text.
    /// Called on every composer event (`Change` path) and after
    /// programmatic `set_value` writes, which suppress `Change`. Opens
    /// when the text ends with a `/`-led token at a word boundary and the
    /// open chat's bot has commands; closes otherwise (non-bot chat, no
    /// commands, invalid trigger, empty composer).
    pub(super) fn sync_command_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        // Bots slice: the `@bot` inline trigger owns the composer start —
        // the `/` menu never competes with the inline-results dropdown.
        let inline_active = quill::composer::inline_query_trigger(&text).is_some();
        let triggered = command_menu_trigger(&text).is_some() && !inline_active;
        let open_chat = self.session().and_then(|session| session.open_chat);
        let has_items = open_chat.is_some_and(|chat_id| {
            self.session()
                .map(|session| !session.command_menu_items(chat_id).is_empty())
                .unwrap_or(false)
        });
        let open = triggered && has_items;
        if open == self.command_menu_open {
            return;
        }
        self.command_menu_open = open;
        self.command_menu_selected = 0;
        cx.notify();
    }

    /// Phase 3.3: Esc / blur / selection / chat-switch dismissal. Returns
    /// true when the menu was open (the key interceptor swallows the
    /// keystroke only then).
    pub(super) fn close_command_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.command_menu_open {
            return false;
        }
        self.command_menu_open = false;
        self.command_menu_selected = 0;
        cx.notify();
        true
    }

    /// Phase 3.3: Up/Down highlight. Returns true when the menu consumed
    /// the key (open with rows to move between); the selection wraps.
    pub(super) fn step_command_menu(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let rows = self
            .command_menu_state(cx)
            .map(|(_, items)| items.len())
            .unwrap_or(0);
        if !self.command_menu_open || rows == 0 {
            return false;
        }
        self.command_menu_selected =
            (self.command_menu_selected as i32 + delta).rem_euclid(rows as i32) as usize;
        cx.notify();
        true
    }

    /// Phase 3.3: current menu rows — (typed prefix, prefix-filtered
    /// items). `None` when the menu is closed, the composer has no `/`
    /// trigger, the open chat's bot has no commands, or nothing matches.
    pub(super) fn command_menu_state(
        &self,
        cx: &Context<Self>,
    ) -> Option<(String, Vec<CommandMenuItem>)> {
        if !self.command_menu_open {
            return None;
        }
        let text = self.composer.read(cx).value().to_string();
        let prefix = command_menu_trigger(&text)?;
        let chat_id = self.session()?.open_chat?;
        let items = self.session()?.command_menu_items(chat_id);
        if items.is_empty() {
            return None;
        }
        let filtered: Vec<CommandMenuItem> = filter_command_menu_items(&items, prefix)
            .into_iter()
            .cloned()
            .collect();
        if filtered.is_empty() {
            return None;
        }
        Some((prefix.to_string(), filtered))
    }

    /// Phase 3.3: Enter with the menu open picks the highlighted row.
    /// Returns true when Enter was consumed.
    pub(super) fn pick_command_menu_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        if !self.command_menu_open || items.is_empty() {
            return false;
        }
        let index = self.command_menu_selected.min(items.len() - 1);
        self.pick_command_menu_index(index, window, cx);
        true
    }

    /// Phase 3.3: tap / Enter pick. The partial `/`-token is replaced via
    /// the 3.1 insert helper, then the menu closes (the next `Change`
    /// reopens it if a trigger token remains).
    pub(super) fn pick_command_menu_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        let Some(item) = items.get(index) else {
            return;
        };
        let command = item.command.clone();
        let current = self.composer.read(cx).value().to_string();
        let base = strip_command_menu_trigger(&current).unwrap_or(current.as_str());
        // Trailing space (tdesktop behavior): without it, the `/`-token
        // trigger still matches `/command`, the menu reopens on the next
        // Enter and consumes it in a no-op loop — Enter could never send.
        let next = format!(
            "{} ",
            quill::composer::insert_bot_command_text(base, &command).trim_end()
        );
        self.composer.update(cx, |input, cx| {
            input.set_value(next, window, cx);
        });
        self.close_command_menu(cx);
    }

    /// A clicked command is sent at once (Telegram Desktop's bot command
    /// list): the composer's `/`-token is dropped, any other text stays.
    pub(super) fn send_command_menu_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        let Some(item) = items.get(index) else {
            return;
        };
        let command = format!("/{}", item.command.trim_start_matches('/'));
        let current = self.composer.read(cx).value().to_string();
        let rest = strip_command_menu_trigger(&current)
            .unwrap_or(current.as_str())
            .to_string();
        self.close_command_menu(cx);
        self.composer.update(cx, |input, cx| {
            input.set_value(rest.trim_end(), window, cx);
            input.focus(window, cx);
        });
        self.submit_composer(command, window, cx);
    }

    /// Phase 3.2: `inlineKeyboardButtonTypeCopyText` — copy to the clipboard.
    pub(super) fn copy_inline_text(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        self.status_note = "copied".into();
        cx.notify();
    }

    pub(super) fn begin_reply_to(
        &mut self,
        reply: ComposerReplyTo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_edit.is_some() {
            self.clear_edit(window, cx);
        }
        self.pending_reply = Some(reply);
        self.note_open_draft(true, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.status_note = "replying".into();
        cx.notify();
    }

    /// Up in the composer: start editing the last own message when the
    /// composer is focused and empty and nothing else uses the key.
    pub(super) fn try_edit_last_message(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.composer.read(cx).focus_handle(cx).is_focused(window)
            || !self.composer.read(cx).value().is_empty()
            || self.pending_edit.is_some()
            || !self.pending_attachments.is_empty()
            || self.command_menu_open
            || !self.mention_menu_items(cx).is_empty()
        {
            return false;
        }
        let Some(edit) = self
            .session()
            .and_then(|s| s.open_chat.and_then(|chat| s.last_editable_message(chat)))
        else {
            return false;
        };
        self.begin_edit(edit, window, cx);
        true
    }

    pub(super) fn begin_edit(
        &mut self,
        edit: ComposerEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_attachments.clear();
        let current = self.composer.read(cx).value().to_string();
        // Flush while the reply is still set so a reply-only draft is not wiped.
        self.note_open_draft(false, cx);
        let reply = self.pending_reply.take();
        let (edit, field, saved, stashed) = begin_edit_keeping_reply(current, edit, reply);
        self.pending_edit = Some(edit);
        self.saved_edit_draft = saved;
        self.saved_edit_reply = stashed;
        self.composer.update(cx, |input, cx| {
            input.set_value(&field, window, cx);
            input.focus(window, cx);
        });
        self.sync_composer_typing(&field);
        self.status_note = "editing".into();
        cx.notify();
    }

    pub(super) fn clear_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // tdesktop cancelEditMessage → applyDraft(): restore normal draft.
        let saved = std::mem::take(&mut self.saved_edit_draft);
        let stashed = self.saved_edit_reply.take();
        let (_, restored) = cancel_edit_draft(self.pending_edit.take(), saved);
        let (restored, reply) = cancel_edit_keeping_reply(restored, stashed);
        self.pending_reply = reply;
        self.composer
            .update(cx, |input, cx| input.set_value(&restored, window, cx));
        self.sync_composer_typing(&restored);
        self.status_note = "edit cancelled".into();
        cx.notify();
    }

    pub(super) fn finish_edit_restore_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saved = std::mem::take(&mut self.saved_edit_draft);
        let reply = self.saved_edit_reply.take();
        self.pending_edit = None;
        self.pending_reply = reply;
        self.composer
            .update(cx, |input, cx| input.set_value(&saved, window, cx));
    }

    pub(super) fn submit_edit(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.pending_edit.clone() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .edit_snapshot(&edit, &text);
            match result {
                Ok(_) => {
                    self.finish_edit_restore_draft(window, cx);
                    self.status_note =
                        self.send_started_note(ComposerScheduling::None, "saving edit…");
                }
                Err(_) => {
                    self.status_note = "could not edit message".into();
                }
            }
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_edit(&edit, text.trim());
            self.finish_edit_restore_draft(window, cx);
            self.status_note = "demo edit applied locally (no live Telegram)".into();
            cx.notify();
        }
    }

    pub(super) fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        self.status_note = "delete cancelled".into();
        cx.notify();
    }

    pub(super) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.pending_delete.take() else {
            return;
        };
        self.begin_vanish(confirm.chat_id, &[confirm.message_id]);
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .delete_confirmed(&confirm);
            self.status_note = match result {
                Ok(_) => "deleting…".into(),
                Err(_) => "could not delete message".into(),
            };
        } else if self.demo_session.is_some() {
            self.apply_demo_delete(confirm.chat_id, confirm.message_id);
            self.status_note = "demo delete applied locally (no live Telegram)".into();
        }
        cx.notify();
    }

    pub(super) fn clear_reply(&mut self, cx: &mut Context<Self>) {
        // tdesktop FieldHeader Escape / replyCancelled: header only — keep typed text.
        self.pending_reply = cancel_reply_draft(self.pending_reply.take(), String::new()).0;
        self.note_open_draft(true, cx);
        self.status_note = "reply cancelled".into();
        cx.notify();
    }

    /// Sticker/GIF/voice sends consume the composer reply. Drop it from the UI
    /// and from the stored draft, and keep any unsent text.
    pub(super) fn consume_sent_reply(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.pending_reply = None;
        if self.pending_edit.is_some() {
            self.saved_edit_reply = None;
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        self.save_chat_draft(chat_id, &text, None, false, cx);
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
        let Some(search) = session
            .mention_search
            .as_ref()
            .filter(|search| Some(search.chat_id) == session.open_chat)
        else {
            return Vec::new();
        };
        search
            .user_ids
            .iter()
            .filter_map(|id| session.user(*id))
            .take(8)
            .map(|user| (user.id, user.display_name(), user.username.clone()))
            .collect()
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
            .mention_search
            .as_ref()
            .map(|s| s.query.clone());
        let _ = live.driver.search_mentions(query.as_deref());
        if before != query {
            self.mention_selected = 0;
            cx.notify();
        }
    }

    /// Esc / blur: drop the suggestions. True when they were showing.
    pub(super) fn close_mention_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let showing = !self.mention_menu_items(cx).is_empty();
        if let Some(live) = self.live.as_mut() {
            live.driver.session.mention_search = None;
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
        self.mention_selected =
            (self.mention_selected as i32 + delta).rem_euclid(rows as i32) as usize;
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
        let index = self.mention_selected.min(items.len() - 1);
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
        let text = self.composer.read(cx).value().to_string();
        let completed = quill::composer::complete_mention(&text, user_id, &username, &name);
        self.composer.update(cx, |input, cx| {
            input.set_value(&completed, window, cx);
            input.focus(window, cx);
        });
        if let Some(live) = self.live.as_mut() {
            live.driver.session.mention_search = None;
        }
        self.sync_composer_typing(&completed);
        self.mention_selected = 0;
        cx.notify();
    }
}

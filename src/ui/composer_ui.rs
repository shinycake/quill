//! composer panels: GIF/sticker pickers, quote-reply/edit banners, link-preview chips, caption bar.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo, find_urls};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::effective_content;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
pub(super) fn apply_ready_stickers(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let loaded = demo_file_json(41, &thumb_path, true);
    let pending = demo_file_json(43, "", false);
    let history = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":401,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatPng"}},"width":128,"height":128,"file":{loaded}}},"sticker":{pending}}}}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&history, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.stickers.open = true;
    session.stickers.loading_sets = true;
    let sets_extra = session.request(RequestPurpose::GetInstalledStickerSets, None);
    let sets = format!(
        r#"{{"@type":"stickerSets","@extra":"{}","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Demo stickers","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":2,"covers":[]}}]}}"#,
        sets_extra.0
    );
    if let Some(owned) = copy_and_parse(&sets, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.mark_sticker_set_loading();
    let set_extra = session.request(RequestPurpose::GetStickerSet, None);
    let smile = demo_file_json(41, &thumb_path, true);
    let wave = demo_file_json(44, "", false);
    let set = format!(
        r#"{{"@type":"stickerSet","@extra":"{}","id":"77","title":"Demo stickers","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatPng"}},"width":128,"height":128,"file":{smile}}},"sticker":{pending}}},{{"@type":"sticker","id":"9002","set_id":"77","width":512,"height":512,"emoji":"👋","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{wave}}}],"emojis":[]}}"#,
        set_extra.0
    );
    if let Some(owned) = copy_and_parse(&set, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    /// Phase 3.3: the `/` command menu popup above the composer.
    /// Bot-specific commands first, then the global (`getCommands`)
    /// section when both exist. Tap inserts; Up/Down/Enter via the key
    /// interceptor; Esc / blur / outside tap dismisses.
    pub(super) fn command_menu_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, items) = self.command_menu_state(cx)?;
        let selected = self.command_menu_selected.min(items.len() - 1);
        let has_specific = items.iter().any(|item| !item.global);
        let has_global = items.iter().any(|item| item.global);
        let show_headers = has_specific && has_global;
        let mut list = div()
            .id("command-menu")
            .flex()
            .flex_col()
            .mx_4()
            .mb_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);
        let mut global_header_shown = false;
        for (index, item) in items.iter().enumerate() {
            if show_headers && item.global && !global_header_shown {
                global_header_shown = true;
                list = list.child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Global"),
                );
            }
            let name = item.command.clone();
            let label = if item.description.is_empty() {
                format!("/{name}")
            } else {
                format!("/{name} — {}", item.description)
            };
            let highlighted = index == selected;
            // Ephemeral-command icon (Telegram blog "Ephemeral Bot
            // Messages": ephemeral commands are marked with a special
            // icon in the bot menu) — eye-off, muted, trailing: the
            // command's result is only visible to the sender.
            let mut row = div()
                .id(("command-menu-item", index as u64))
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .pressable(cx.theme())
                .when(highlighted, |this| this.bg(cx.theme().selection))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pick_command_menu_index(index, window, cx);
                }))
                .child(div().text_sm().flex_1().min_w_0().child(label));
            if item.is_ephemeral {
                row = row.child(
                    Icon::new(IconName::EyeOff)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                );
            }
            list = list.child(row);
        }
        Some(list.into_any_element())
    }

    /// Slice G1: open the partial-quote dialog for a text message.
    pub(super) fn open_quote_reply_dialog(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let full_text = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| {
                Self::message_copyable_text(effective_content(
                    &message.content,
                    message.ephemeral.as_ref(),
                ))
            })
            .unwrap_or_default();
        if full_text.trim().is_empty() {
            self.status_note = "only text messages can be quoted".into();
            cx.notify();
            return;
        }
        self.quote_reply_dialog = Some(QuoteReplyDialog::new(
            window, cx, chat_id, message_id, full_text,
        ));
        cx.notify();
    }

    pub(super) fn close_quote_reply_dialog(&mut self, cx: &mut Context<Self>) {
        self.quote_reply_dialog = None;
        cx.notify();
    }

    /// Slice G1: validate the trimmed quote against the original text
    /// and set the composer's reply with its UTF-16 offset. A quote
    /// that is no longer a verbatim substring keeps the dialog open
    /// with an explanatory note.
    pub(super) fn submit_quote_reply_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.quote_reply_dialog.take() else {
            return;
        };
        let quote_text = dialog.input.read(cx).value();
        let quote_text = quote_text.trim();
        match quill::composer::quote_position(&dialog.full_text, quote_text) {
            Some(position) => {
                let preview = dialog.full_text.chars().take(80).collect::<String>();
                let reply = quill::composer::ComposerReplyTo::with_quote(
                    dialog.chat_id,
                    dialog.message_id,
                    preview,
                    quill::composer::QuoteSelection {
                        text: quote_text.to_string(),
                        position,
                    },
                );
                self.begin_reply_to(reply, window, cx);
                self.status_note = "quoting part of the message".into();
            }
            None => {
                self.quote_reply_dialog = Some(dialog);
                self.status_note = "the quote must be an unedited part of the message".into();
            }
        }
        cx.notify();
    }

    pub(super) fn gif_panel_open(&self) -> bool {
        self.session().is_some_and(|session| session.gifs.open)
    }

    pub(super) fn toggle_gif_panel(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() {
            self.cancel_recording(cx);
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
            return;
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.open_gif_panel() {
                Ok(_) => "GIFs".into(),
                Err(_) => "could not open GIFs".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.gifs.open = true;
            self.status_note = "GIFs".into();
        }
        cx.notify();
    }

    pub(super) fn close_gif_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_gif_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.gifs.close();
        }
        self.status_note = "GIFs closed".into();
        cx.notify();
    }

    pub(super) fn send_gif_pick(
        &mut self,
        file_id: FileId,
        duration: i32,
        width: i32,
        height: i32,
        cx: &mut Context<Self>,
    ) {
        let chat_id = self.session().and_then(|session| session.open_chat);
        let Some(chat_id) = chat_id else {
            return;
        };
        // Phase A1: slow-mode gate applies to GIF sends too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply = self
            .pending_reply
            .as_ref()
            .filter(|reply| reply.chat_id == chat_id)
            .map(|reply| quill::telegram::SendReply {
                message_id: reply.message_id,
                quote: reply
                    .quote
                    .as_ref()
                    .map(|quote| (quote.text.clone(), quote.position)),
            });
        let sent = if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.send_animation(
                chat_id,
                quill::telegram::requests::AnimationSend {
                    file_id,
                    duration,
                    width,
                    height,
                    reply_to: reply,
                    // Parity slice 4: the driver addresses the open topic
                    // from the session; the UI passes no topic.
                    topic_id: None,
                },
            ) {
                Ok(_) => "sending GIF".into(),
                Err(_) => "could not send GIF".into(),
            };
            self.status_note == "sending GIF"
        } else if self.demo_session.is_some() {
            self.apply_demo_gif(chat_id, file_id, duration, width, height, reply);
            self.status_note = "demo GIF applied locally (no live Telegram)".into();
            true
        } else {
            false
        };
        if sent {
            self.consume_sent_reply(chat_id, cx);
        }
        cx.notify();
    }

    pub(super) fn sticker_panel_open(&self) -> bool {
        self.session().is_some_and(|session| session.stickers.open)
    }

    pub(super) fn toggle_sticker_panel(&mut self, cx: &mut Context<Self>) {
        if self.recording_active() {
            self.cancel_recording(cx);
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
            return;
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.open_sticker_panel() {
                Ok(_) => "stickers".into(),
                Err(_) => "could not open stickers".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.open = true;
            self.status_note = "stickers".into();
        }
        cx.notify();
    }

    pub(super) fn close_sticker_panel(&mut self, cx: &mut Context<Self>) {
        self.sticker_settings_open = false;
        if let Some(live) = self.live.as_mut() {
            live.driver.close_sticker_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.close();
        }
        self.status_note = "stickers closed".into();
        cx.notify();
    }

    pub(super) fn select_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.select_sticker_set(set_id) {
                Ok(_) => "sticker set".into(),
                Err(_) => "could not open sticker set".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.select_sticker_set(set_id);
            self.status_note = "sticker set".into();
        }
        cx.notify();
    }

    pub(super) fn send_sticker_pick(
        &mut self,
        file_id: FileId,
        emoji: String,
        width: i32,
        height: i32,
        thumb: Option<(FileId, i32, i32)>,
        cx: &mut Context<Self>,
    ) {
        let chat_id = self.session().and_then(|session| session.open_chat);
        let Some(chat_id) = chat_id else {
            return;
        };
        // Phase A1: slow-mode gate applies to sticker sends too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply = self
            .pending_reply
            .as_ref()
            .filter(|reply| reply.chat_id == chat_id)
            .map(|reply| quill::telegram::SendReply {
                message_id: reply.message_id,
                quote: reply
                    .quote
                    .as_ref()
                    .map(|quote| (quote.text.clone(), quote.position)),
            });
        let sent = if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.send_sticker(
                chat_id,
                quill::telegram::requests::StickerSend {
                    file_id,
                    emoji: &emoji,
                    width,
                    height,
                    thumb,
                    reply_to: reply,
                    // Parity slice 4: the driver addresses the open topic
                    // from the session; the UI passes no topic.
                    topic_id: None,
                },
            ) {
                Ok(_) => "sticker sent".into(),
                Err(_) => "could not send sticker".into(),
            };
            self.status_note == "sticker sent"
        } else if self.demo_session.is_some() {
            self.apply_demo_sticker(chat_id, &emoji, file_id, reply);
            self.status_note = "sticker sent".into();
            true
        } else {
            false
        };
        if sent {
            self.consume_sent_reply(chat_id, cx);
        }
        cx.notify();
    }

    /// kit Phase 2 (redo): quote reply hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_quote_reply_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::QuoteReply, |this, _, cx| {
                this.close_quote_reply_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).title("Quote part of message");
            let Some(dialog_state) = this.quote_reply_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Trim the text below to the part you want to quote"),
                )
                .child(
                    div()
                        .flex_1()
                        .child(Textarea::new(&dialog_state.input).h(px(120.))),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-quote-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_quote_reply_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::QuoteReply, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-quote-submit")
                        .label("Quote reply")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_quote_reply_dialog(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::QuoteReply, window, cx);
                        })),
                );
            dialog
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    pub(super) fn gif_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self
            .session()
            .map(|session| session.gifs.clone())
            .unwrap_or_default();
        let files = self
            .session()
            .map(|session| session.files.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let mut grid = div().id("gif-grid").flex().flex_wrap().gap_2();
        for (index, animation) in panel.animations.iter().enumerate() {
            let file_id = animation.file_id;
            let duration = animation.duration;
            let width = animation.width;
            let height = animation.height;
            let display_id = animation.thumb_file_id.filter(|id| id.0 != 0);
            let path = display_id.and_then(|id| {
                files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
            });
            let label = if animation.file_name.is_empty() {
                "GIF".to_string()
            } else {
                animation.file_name.clone()
            };
            let cell_id = format!("gif-pick-{index}-{file_id}", file_id = file_id.0);
            let cell = if let Some(path) = path {
                img(path)
                    .id(SharedString::from(cell_id.clone()))
                    .w(px(96.))
                    .h(px(72.))
                    .rounded_md()
                    .object_fit(ObjectFit::Cover)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_gif_pick(file_id, duration, width, height, cx);
                    }))
                    .with_fallback({
                        let label = label.clone();
                        move || {
                            div()
                                .w(px(96.))
                                .h(px(72.))
                                .rounded_md()
                                .bg(accent_strong())
                                .text_color(text_on_fill())
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(label.clone())
                                .into_any_element()
                        }
                    })
                    .into_any_element()
            } else {
                div()
                    .id(SharedString::from(cell_id))
                    .w(px(96.))
                    .h(px(72.))
                    .rounded_md()
                    .bg(accent_strong())
                    .text_color(text_on_fill())
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_gif_pick(file_id, duration, width, height, cx);
                    }))
                    .child(label)
                    .into_any_element()
            };
            grid = grid.child(cell);
        }
        let status = if panel.loading {
            "Loading saved GIFs…"
        } else if panel.failed {
            "Could not load saved GIFs."
        } else if panel.animations.is_empty() {
            "No saved GIFs."
        } else {
            "Tap a GIF to send it."
        };
        div()
            .id("gif-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("GIFs"))
                    .child(
                        Button::new("close-gif-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_gif_panel(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            .child(grid)
    }

    pub(super) fn composer_edit_banner(
        &self,
        edit: &ComposerEdit,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let preview = edit.original_text.clone();
        let kind = match edit.kind {
            quill::composer::ComposerEditKind::Text => "Editing message",
            quill::composer::ComposerEditKind::Caption => "Editing caption",
        };
        div()
            .id("composer-edit-header")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(warning())
            .bg(bg_subtle())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(warning())
                            .child(kind),
                    )
                    .child(div().text_sm().text_color(text_primary()).child(preview)),
            )
            .child(
                Button::new("cancel-edit")
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_edit(window, cx);
                    })),
            )
    }

    /// MED4b: debounced `getLinkPreview` prefetch for the detected-URL
    /// chip. Fires 500ms after the URL settles (schema: "Do not call this
    /// function too often"; TGX rate-limits the same call at 400ms).
    /// Live sessions only — the screenshot demo injects its preview.
    pub(super) fn maybe_prefetch_link_preview(&mut self, cx: &mut Context<Self>) {
        // Live sessions only — the screenshot demo injects its preview,
        // and without a driver every frame would spawn a no-op timer.
        if self.live.is_none() {
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let Some(url) = find_urls(&text).into_iter().next() else {
            // No URL: drop any stale preview so the chip never shows a
            // preview for a URL that's no longer there.
            if let Some(live) = self.live.as_mut()
                && live.driver.session.composer_preview.is_some()
            {
                live.driver.session.composer_preview = None;
            }
            return;
        };
        // TGX never prefetches a disabled preview.
        if self.composer_preview_disabled {
            return;
        }
        let already = self
            .live
            .as_ref()
            .and_then(|live| live.driver.session.composer_preview.as_ref())
            .is_some_and(|p| p.url == url);
        if already {
            return;
        }
        self.composer_preview_token = self.composer_preview_token.wrapping_add(1);
        let token = self.composer_preview_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            this.update(cx, |this, cx| {
                if this.composer_preview_token != token {
                    return;
                }
                // Re-check the URL survived the quiet window; a newer
                // keystroke schedules its own timer.
                let text = this.composer.read(cx).value().to_string();
                if find_urls(&text).into_iter().next().as_deref() != Some(url.as_str()) {
                    return;
                }
                if this.composer_preview_disabled {
                    return;
                }
                if let Some(live) = this.live.as_mut()
                    && live.driver.request_composer_link_preview(&url).is_err()
                {
                    this.status_note = "couldn't load link preview".into();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// MED4b: TGX `LinkPreview.hasMedia` — the chip's size toggle needs
    /// *some* media to resize (photo, embedded player, album strip).
    pub(super) fn preview_has_media(preview: &quill::telegram::envelope::LinkPreview) -> bool {
        preview.photo.is_some()
            || !matches!(
                preview.kind,
                quill::telegram::envelope::LinkPreviewKind::Plain
            )
    }

    /// MED4: detected-URL chip for send-time link-preview controls
    /// (schema 1.8.67 `linkPreviewOptions`, :2237). Shows the detected
    /// URL, the debounced `getLinkPreview` prefetch (title/description,
    /// "Getting link info…" while loading, "No preview" on 404), and —
    /// TGX (`MessagesController.onRequestToggleLargeMedia` /
    /// `onRequestToggleShowAbove`) — the large/small media toggle (only
    /// when the preview offers large media) and the above/below-text
    /// toggle. The choices ride the next send's
    /// `inputMessageText.link_preview_options`.
    pub(super) fn preview_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.pending_attachments.is_empty() || self.pending_edit.is_some() {
            return None;
        }
        let text = self.composer.read(cx).value().to_string();
        let first = find_urls(&text).into_iter().next()?;
        // Live prefetch state; the screenshot demo injects its own into
        // the demo session.
        let stored = self
            .session()
            .and_then(|s| s.composer_preview.clone())
            .filter(|p| p.url == first);
        let fetched: Option<quill::telegram::envelope::LinkPreview> =
            stored.as_ref().and_then(|p| p.preview.clone()).flatten();
        let preview_line: Option<String> = match &stored {
            // Loaded: title/description (TGX `LinkPreview.getForcedTitle`
            // falls back site → title the same way).
            Some(s) => match &s.preview {
                Some(Some(p)) => {
                    let title = if p.title.is_empty() {
                        p.site_name.clone()
                    } else {
                        p.title.clone()
                    };
                    let title = if title.is_empty() {
                        p.display_url.clone()
                    } else {
                        title
                    };
                    let glyph = if Self::preview_has_media(p) {
                        "🖼 "
                    } else {
                        ""
                    };
                    let mut line = format!("{glyph}{title}");
                    if !p.description.is_empty() {
                        let desc: String = p.description.chars().take(80).collect();
                        line.push_str(&format!(" — {desc}"));
                    }
                    Some(line)
                }
                // TDLib 404: "no link info" (TGX
                // `LinkPreview.isNotFound`) — honest, never a fake card.
                Some(None) => Some("No preview for this link".to_string()),
                // Request in flight.
                None => Some("Getting link info…".to_string()),
            },
            None => None,
        };
        // TGX `LinkPreview.toggleLargeMedia`: the size toggle only
        // exists when the preview offers large media.
        let size_toggle = !self.composer_preview_disabled
            && fetched
                .as_ref()
                .is_some_and(|p| p.has_large_media && Self::preview_has_media(p));
        // The preview's own default; the toggle flips relative to the
        // *current* effective size (TGX `LinkPreview.toggleLargeMedia`).
        let preview_default_large = fetched
            .as_ref()
            .map(|p| p.show_large_media)
            .unwrap_or(false);
        let effective_large = self
            .composer_preview_media
            .effective_large(preview_default_large);
        let mut row = div()
            .id("composer-preview-chip")
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .child(
                div()
                    .text_xs()
                    .text_color(accent())
                    .child(format!("🔗 {first}")),
            );
        if let Some(line) = preview_line {
            row = row.child(div().text_xs().text_color(text_muted()).child(line));
        }
        row = row.child(
            Button::new("composer-preview-chip-toggle")
                .label(if self.composer_preview_disabled {
                    "Preview off"
                } else {
                    "Preview on"
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.composer_preview_disabled = !this.composer_preview_disabled;
                    cx.notify();
                })),
        );
        if !self.composer_preview_disabled {
            if size_toggle {
                row = row.child(
                    Button::new("composer-preview-chip-size")
                        .label(if effective_large {
                            "Media: large"
                        } else {
                            "Media: small"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let before = this
                                .composer_preview_media
                                .effective_large(preview_default_large);
                            this.composer_preview_media =
                                this.composer_preview_media.toggle(before);
                            let after = this
                                .composer_preview_media
                                .effective_large(preview_default_large);
                            this.status_note = format!(
                                "link preview media: {}",
                                if after { "large" } else { "small" }
                            );
                            cx.notify();
                        })),
                );
            }
            row = row.child(
                Button::new("composer-preview-chip-above")
                    .label(if self.composer_preview_above {
                        "Above text"
                    } else {
                        "Below text"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.composer_preview_above = !this.composer_preview_above;
                        // TGX hint strings (`LinkPreviewShowAbove` /
                        // `LinkPreviewShowBelow`).
                        this.status_note = if this.composer_preview_above {
                            "Link preview will appear above the text".into()
                        } else {
                            "Link preview will appear below the text".into()
                        };
                        cx.notify();
                    })),
            );
        }
        Some(row.into_any_element())
    }

    /// caption…" affordance, the caption-above-media toggle
    /// (`show_caption_above_media`, schema 1.8.67 lines 6117/6128, only
    /// for photo/video), and the `n / max` counter from the runtime
    /// `message_caption_length_max` option. Shown while attachments are
    /// pending or a caption is being edited.
    pub(super) fn caption_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing_caption = self
            .pending_edit
            .as_ref()
            .is_some_and(|edit| matches!(edit.kind, quill::composer::ComposerEditKind::Caption));
        if self.pending_attachments.is_empty() && !editing_caption {
            return None;
        }
        let captionable = editing_caption
            || self.pending_attachments.iter().any(|att| {
                matches!(
                    att.kind,
                    quill::composer::AttachmentKind::Photo | quill::composer::AttachmentKind::Video
                )
            });
        let above = if editing_caption {
            self.pending_edit
                .as_ref()
                .is_some_and(|edit| edit.caption_above)
        } else {
            self.composer_caption_above
        };
        let text_len = self.composer.read(cx).value().chars().count();
        let limit = self
            .live
            .as_ref()
            .map(|live| live.driver.session.message_caption_length_max)
            .unwrap_or(1024);
        let over = text_len as i64 > i64::from(limit.max(0));
        Some(
            div()
                .id("composer-caption-bar")
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child("Add a caption…"),
                        )
                        .when(captionable && !self.open_chat_is_secret(), |this| {
                            this.child(
                                Button::new("composer-caption-above")
                                    .label(if above {
                                        "Caption: above"
                                    } else {
                                        "Caption: below"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(edit) = this.pending_edit.as_mut() {
                                            edit.caption_above = !edit.caption_above;
                                        } else {
                                            this.composer_caption_above =
                                                !this.composer_caption_above;
                                        }
                                        cx.notify();
                                    })),
                            )
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if over { danger_bright() } else { text_muted() })
                        .child(format!("{text_len} / {limit}")),
                )
                .into_any_element(),
        )
    }

    pub(super) fn delete_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let confirm = self.pending_delete.clone();
        let scope_label = match confirm.as_ref() {
            // M1: only own outgoing messages offer the for-everyone toggle
            // (`deleteMessages.revoke`, schema 1.8.67 lines 6228–6229);
            // incoming deletes are always for-me.
            Some(c) if c.can_revoke => {
                if c.revoke {
                    "Deletes for everyone"
                } else {
                    "Deletes for me"
                }
            }
            _ => "Deletes for me",
        };
        div()
            .id("delete-confirm")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(danger())
            .bg(danger_bg())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(danger())
                            .child("Delete this message?"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_primary())
                            .child(scope_label.to_string()),
                    ),
            )
            .when(confirm.as_ref().is_some_and(|c| c.can_revoke), |this| {
                this.child(
                    Button::new("delete-toggle-scope")
                        .label(if confirm.as_ref().is_some_and(|c| c.revoke) {
                            "For me"
                        } else {
                            "For everyone"
                        })
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(confirm) = this.pending_delete.as_mut() {
                                confirm.revoke = !confirm.revoke;
                                this.status_note = if confirm.revoke {
                                    "delete: for everyone".into()
                                } else {
                                    "delete: for me".into()
                                };
                            }
                            cx.notify();
                        })),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("cancel-delete")
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_delete(cx);
                            })),
                    )
                    .child(
                        Button::new("confirm-delete")
                            .label("Delete")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete(cx);
                            })),
                    ),
            )
    }

    pub(super) fn composer_reply_banner(
        &self,
        reply: &ComposerReplyTo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let preview = reply.preview.clone();
        // Slice G1: show the quoted part when the reply carries one.
        let quote_label = reply
            .quote
            .as_ref()
            .map(|quote| format!("❝{}❞", quote.text));
        div()
            .id("composer-reply-quote")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_subtle())
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
                            .child("Replying to"),
                    )
                    .child(div().text_sm().text_color(text_primary()).child(preview))
                    .when_some(quote_label, |this, label| {
                        this.child(div().text_xs().text_color(text_muted()).child(label))
                    }),
            )
            .child(
                Button::new("cancel-reply")
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clear_reply(cx);
                    })),
            )
    }
}

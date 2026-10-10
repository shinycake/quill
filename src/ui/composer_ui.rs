//! Composer command menus, quote/reply/edit banners, link previews and captions.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::attachment::{
    Attachment, AttachmentActions, AttachmentContent, AttachmentDescription, AttachmentGroup,
    AttachmentMedia, AttachmentTitle,
};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, ComposerAttachment, ComposerEdit, ComposerReplyTo, LinkPreviewChoice,
    PreviewMediaSize, find_urls,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::effective_content;
use quill::telegram::requests::SelfDestructSend;
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

pub(super) fn apply_ready_sticker_playback(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let root = super::demo::demo_media_allowlist();
    session.stickers.stickers.clear();
    for (id, name, format) in [
        (44, "demo-sticker.tgs", "stickerFormatTgs"),
        (45, "demo-sticker.webm", "stickerFormatWebm"),
    ] {
        let file = demo_file_json(id, &root.join(name).to_string_lossy(), true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"{id}","set_id":"77","width":128,"height":128,"emoji":"😀","format":{{"@type":"{format}"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}}}}}}}}"#
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
        session
            .stickers
            .stickers
            .push(quill::telegram::envelope::StickerItem {
                custom_emoji_id: None,
                id: i64::from(id),
                set_id: 77,
                emoji: "😀".into(),
                width: 128,
                height: 128,
                format: if id == 44 {
                    quill::telegram::envelope::StickerFormat::Tgs
                } else {
                    quill::telegram::envelope::StickerFormat::Webm
                },
                file_id: FileId(id),
                thumb_file_id: None,
                thumb_width: 0,
                thumb_height: 0,
                requires_premium: false,
            });
    }
    for id in [101, 102, 103, 401] {
        if let Some(history) = session.histories.get_mut(&11) {
            history.messages.remove(&id);
        }
    }
    session.media_prefs.loop_animated_stickers =
        std::env::var("QUILL_DEMO_LOOP_STICKERS").as_deref() != Ok("off");
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
            // A compact popup that scrolls (Telegram Desktop), not a
            // list covering the whole chat.
            .max_h(px(320.))
            .overflow_y_scroll()
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
                .role(gpui_kit::Role::Button)
                .aria_label(label.clone())
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .when(highlighted, |this| this.bg(cx.theme().selection))
                // On press, not click: the press moves focus off the
                // composer, whose blur closes this menu before a click
                // could land. Telegram Desktop sends a clicked command.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        window.prevent_default();
                        this.send_command_menu_index(index, window, cx);
                    }),
                )
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

    /// Quote & Reply (Telegram Desktop): reply quoting the part of the
    /// message selected in the history. The quote must be verbatim message
    /// text; its UTF-16 offset is located in the full text.
    pub(super) fn begin_quote_reply(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        quote: &str,
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
        let quote = quote.trim();
        let Some(position) = quill::composer::quote_position(&full_text, quote) else {
            // Not verbatim text of the message (it shouldn't happen for a
            // selection); reply to the whole message instead.
            self.begin_reply_from_message(chat_id, message_id, window, cx);
            return;
        };
        let preview = full_text.chars().take(80).collect::<String>();
        let reply = quill::composer::ComposerReplyTo::with_quote(
            chat_id,
            message_id,
            preview,
            quill::composer::QuoteSelection {
                text: quote.to_string(),
                position,
            },
        );
        self.begin_reply_to(reply, window, cx);
        cx.notify();
    }

    pub(super) fn gif_panel_open(&self) -> bool {
        self.session().is_some_and(|session| session.gifs.open)
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
        if self.deny_send(quill::send_rights::SendKind::Gifs, cx) {
            return;
        }
        let reply = self
            .pending_reply
            .as_ref()
            .and_then(|reply| reply.send_target(chat_id));
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
        if self.session().is_some_and(|session| {
            session.sticker_requires_premium(file_id) && !session.my_is_premium()
        }) {
            self.status_note = "Sending this sticker requires Telegram Premium".into();
            cx.notify();
            return;
        }
        let chat_id = self.session().and_then(|session| session.open_chat);
        let Some(chat_id) = chat_id else {
            return;
        };
        // Phase A1: slow-mode gate applies to sticker sends too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        if self.deny_send(quill::send_rights::SendKind::Stickers, cx) {
            return;
        }
        let reply = self
            .pending_reply
            .as_ref()
            .and_then(|reply| reply.send_target(chat_id));
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
        // An edited photo or video shows its small preview, as tdesktop's
        // field header does.
        let thumb = self.bar_thumbnail(edit.chat_id, edit.message_id, cx);
        // B5: tdesktop `EditCaptionBox` "Replace attachment".
        let replace_button = edit.allows_replace().then(|| {
            Button::new("composer-edit-replace")
                .icon(gpui_kit::assets::IconName::Paperclip)
                .label("Replace")
                .ghost()
                .small()
                .tooltip("Replace attachment")
                .accessibility_label("Replace attachment")
                .on_click(cx.listener(|this, _, _, cx| this.pick_edit_replacement(cx)))
                .into_any_element()
        });
        composer_context_bar_rich(
            "composer-edit-header",
            gpui_kit::assets::IconName::Pencil,
            accent().into(),
            kind,
            div()
                .text_sm()
                .truncate()
                .text_color(cx.theme().muted_foreground)
                .child(super::bidi_line::one_line_plain(
                    super::search_ui::one_line_preview(&preview),
                ))
                .into_any_element(),
            thumb,
            replace_button,
            Button::new("cancel-edit")
                .icon(gpui_kit::assets::IconName::X)
                .ghost()
                .small()
                .tooltip("Cancel editing")
                .accessibility_label("Cancel editing")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.clear_edit(window, cx);
                })),
            cx,
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
        let Some(url) = self.chosen_preview_url(&text) else {
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
        if self.preview_choice().disabled {
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
                if this.chosen_preview_url(&text).as_deref() != Some(url.as_str()) {
                    return;
                }
                if this.preview_choice().disabled {
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

    /// B5: the link-preview choices in effect: the edited text message's
    /// while editing (tdesktop seeds the edit draft with the message's own
    /// `WebPageDraft`), otherwise the next send's.
    pub(super) fn preview_choice(&self) -> LinkPreviewChoice {
        match self.pending_edit.as_ref() {
            Some(edit) => edit.link_preview,
            None => LinkPreviewChoice {
                disabled: self.composer_preview_disabled,
                above_text: self.composer_preview_above,
                media: self.composer_preview_media,
                link_index: self.composer_preview_link,
            },
        }
    }

    pub(super) fn set_preview_choice(&mut self, choice: LinkPreviewChoice) {
        match self.pending_edit.as_mut() {
            Some(edit) => edit.link_preview = choice,
            None => {
                self.composer_preview_disabled = choice.disabled;
                self.composer_preview_above = choice.above_text;
                self.composer_preview_media = choice.media;
                self.composer_preview_link = choice.link_index;
            }
        }
    }

    /// The link the preview is generated from: the one picked in the
    /// options menu, else the first (clamped to the links present).
    pub(super) fn chosen_preview_url(&self, text: &str) -> Option<String> {
        let urls = find_urls(text);
        let index = self.preview_choice().link_index;
        urls.get(index).or_else(|| urls.last()).cloned()
    }

    /// B5: tdesktop's "Link Preview Settings" popover (history_view_draft_options):
    /// click a link to generate its preview, Move Up/Down, Shrink/Enlarge
    /// the media, Do Not Preview. Maps to `linkPreviewOptions`
    /// (`url`, `show_above_text`, `force_small_media` / `force_large_media`,
    /// `is_disabled`).
    fn link_options_menu(
        owner: WeakEntity<Self>,
        mut menu: gpui_kit::component::menu::PopupMenu,
        urls: Vec<String>,
        chosen: usize,
        choice: LinkPreviewChoice,
        size_toggle: Option<(bool, &'static str)>,
    ) -> gpui_kit::component::menu::PopupMenu {
        fn apply(
            owner: &WeakEntity<QuillApp>,
            cx: &mut App,
            note: Option<&'static str>,
            change: impl FnOnce(&mut LinkPreviewChoice),
        ) {
            let _ = owner.update(cx, |this, cx| {
                let mut choice = this.preview_choice();
                change(&mut choice);
                this.set_preview_choice(choice);
                if let Some(note) = note {
                    this.status_note = note.into();
                }
                cx.notify();
            });
        }
        menu = menu.label("Link Preview Settings");
        if urls.len() > 1 {
            for (index, url) in urls.iter().enumerate() {
                let owner = owner.clone();
                let shown: String = if url.chars().count() > 48 {
                    format!("{}…", url.chars().take(47).collect::<String>())
                } else {
                    url.clone()
                };
                menu = menu.item(
                    PopupMenuItem::new(shown)
                        .checked(index == chosen && !choice.disabled)
                        .on_click(move |_, _, cx| {
                            apply(&owner, cx, None, |c| {
                                c.link_index = index;
                                c.disabled = false;
                            });
                        }),
                );
            }
            menu = menu.separator();
        }
        let disabled = choice.disabled;
        if !disabled {
            let above = choice.above_text;
            let toggle = owner.clone();
            menu = menu.item(
                PopupMenuItem::new(if above { "Move Down" } else { "Move Up" }).on_click(
                    move |_, _, cx| {
                        apply(
                            &toggle,
                            cx,
                            Some(if above {
                                "Link preview will appear below the text"
                            } else {
                                "Link preview will appear above the text"
                            }),
                            |c| c.above_text = !above,
                        );
                    },
                ),
            );
            if let Some((large, noun)) = size_toggle {
                let resize = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(format!(
                        "{} {noun}",
                        if large { "Shrink" } else { "Enlarge" }
                    ))
                    .on_click(move |_, _, cx| {
                        apply(&resize, cx, None, |c| {
                            c.media = if large {
                                PreviewMediaSize::ForceSmall
                            } else {
                                PreviewMediaSize::ForceLarge
                            };
                        });
                    }),
                );
            }
        }
        let off = owner;
        menu.item(
            PopupMenuItem::new(if disabled {
                "Show Preview"
            } else {
                "Do Not Preview"
            })
            .on_click(move |_, _, cx| {
                apply(&off, cx, None, |c| c.disabled = !disabled);
            }),
        )
    }

    /// MED4: detected-URL chip for send-time link-preview controls
    /// (schema 1.8.67 `linkPreviewOptions`, :2237). Shows the chosen
    /// URL, the debounced `getLinkPreview` prefetch (title/description,
    /// "Getting link info…" while loading, "No preview" on 404) and one
    /// "Link options" popover (B5, tdesktop's draft options): choose the
    /// link, move the preview above/below the text, shrink/enlarge its
    /// media (only when it offers large media) or remove it. The choices
    /// ride the next send's, or the text edit's,
    /// `inputMessageText.link_preview_options`.
    pub(super) fn preview_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing_text = self
            .pending_edit
            .as_ref()
            .is_some_and(|edit| matches!(edit.kind, quill::composer::ComposerEditKind::Text));
        if !self.pending_attachments.is_empty() || (self.pending_edit.is_some() && !editing_text) {
            return None;
        }
        let text = self.composer.read(cx).value().to_string();
        let urls = find_urls(&text);
        let first = self.chosen_preview_url(&text)?;
        let choice = self.preview_choice();
        let chosen = urls.iter().position(|u| *u == first).unwrap_or(0);
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
        let size_toggle = (!choice.disabled)
            .then_some(fetched.as_ref())
            .flatten()
            .filter(|p| p.has_large_media && Self::preview_has_media(p))
            .map(|p| {
                // The toggle flips relative to the *current* effective size.
                let large = choice.media.effective_large(p.show_large_media);
                let noun = if matches!(
                    p.kind,
                    quill::telegram::envelope::LinkPreviewKind::EmbeddedPlayer { .. }
                ) {
                    "Video"
                } else {
                    "Photo"
                };
                (large, noun)
            });
        let owner = cx.entity().downgrade();
        let mut row = div()
            .id("composer-preview-chip")
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(accent())
                    .child(format!("🔗 {first}")),
            );
        if choice.disabled {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(text_muted())
                    .child("Preview off"),
            );
        } else if let Some(line) = preview_line {
            row = row.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(text_muted())
                    .child(line),
            );
        }
        row = row.child(
            Button::new("composer-preview-options")
                .label("Link options")
                .ghost()
                .small()
                .tooltip("Link Preview Settings")
                .accessibility_label("Link Preview Settings")
                .dropdown_menu(move |menu, _, _| {
                    Self::link_options_menu(
                        owner.clone(),
                        menu,
                        urls.clone(),
                        chosen,
                        choice,
                        size_toggle,
                    )
                }),
        );
        Some(row.into_any_element())
    }

    /// B5: the staged replacement file while editing media: its name,
    /// tdesktop's "Send as a document" (outside albums) and the spoiler
    /// option for photos and videos, and a way to drop it again.
    pub(super) fn edit_replacement_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.pending_edit.as_ref()?;
        let replacement = edit.media_edit.replacement.as_ref()?;
        let as_file_toggle = edit.can_toggle_as_file(replacement);
        let spoiler_toggle = matches!(
            replacement.kind,
            quill::composer::EditMediaKind::Photo | quill::composer::EditMediaKind::Video
        );
        let spoiler = replacement.spoiler;
        let as_file = replacement.kind == quill::composer::EditMediaKind::Document;
        Some(
            div()
                .id("composer-edit-replacement")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .child(
                    Icon::new(IconName::Paperclip)
                        .size(px(14.))
                        .text_color(accent()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(text_muted())
                        .child(format!(
                            "Replacing attachment with {}",
                            replacement.file_name
                        )),
                )
                .when(as_file_toggle, |this| {
                    this.child(
                        Checkbox::new("composer-edit-replace-as-file")
                            .label("Send as a document")
                            .checked(as_file)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_edit_replace_as_file(cx);
                            })),
                    )
                })
                .when(spoiler_toggle, |this| {
                    this.child(
                        Checkbox::new("composer-edit-replace-spoiler")
                            .label("Spoiler")
                            .checked(spoiler)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_edit_replace_spoiler(cx);
                            })),
                    )
                })
                .child(
                    Button::new("composer-edit-replace-clear")
                        .icon(IconName::X)
                        .ghost()
                        .xsmall()
                        .tooltip("Keep the original attachment")
                        .accessibility_label("Keep the original attachment")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_edit_replacement(cx);
                        })),
                )
                .into_any_element(),
        )
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
        // Documents and music have no caption position; photos, videos and
        // GIFs do (`show_caption_above_media`), also after a replacement.
        let captionable = self
            .pending_edit
            .as_ref()
            .is_some_and(|edit| editing_caption && edit.caption_position_applies())
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
                                Checkbox::new("composer-caption-above")
                                    .label("Caption above media")
                                    .checked(above)
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

    /// R8: tdesktop `CharactersLimitLabel` — only shown once the text is
    /// over `message_text_length_max`, as a red "−N" (units to remove) while
    /// editing. A new message over the limit is not an error (it is sent as
    /// several messages), so the label says how many.
    pub(super) fn text_limit_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.pending_attachments.is_empty() {
            return None;
        }
        let editing = self.pending_edit.as_ref();
        if editing.is_some_and(|edit| !matches!(edit.kind, quill::composer::ComposerEditKind::Text))
        {
            return None;
        }
        let limit = self.text_length_limit();
        let value = self.composer_markup(cx);
        let over = quill::text_split::units_over_limit(value.trim(), limit);
        if over == 0 {
            return None;
        }
        let (label, color) = if editing.is_some() {
            (format!("\u{2212}{}", over.min(999)), danger_bright())
        } else {
            let parts = quill::text_split::split_markup_text(value.trim(), limit).len();
            (format!("Will be sent as {parts} messages"), text_muted())
        };
        Some(
            div()
                .id("composer-text-limit")
                .flex()
                .justify_end()
                .px_3()
                .py_1()
                .child(div().text_xs().text_color(color).child(label))
                .into_any_element(),
        )
    }

    pub(super) fn delete_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let confirm = self.pending_delete.clone();
        let can_revoke = confirm.as_ref().is_some_and(|c| c.can_revoke);
        let revoke = confirm.as_ref().is_some_and(|c| c.revoke);
        composer_context_bar(
            "delete-confirm",
            gpui_kit::assets::IconName::Trash,
            danger().into(),
            "Delete message?",
            if can_revoke && revoke {
                "It will be deleted for everyone in this chat.".to_string()
            } else {
                "It will be deleted for you only.".to_string()
            },
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(can_revoke, |this| {
                        this.child(
                            // M1: only own outgoing messages offer the
                            // for-everyone option (`deleteMessages.revoke`).
                            Checkbox::new("delete-for-everyone")
                                .label("Delete for everyone")
                                .checked(revoke)
                                .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                    if let Some(confirm) = this.pending_delete.as_mut() {
                                        confirm.revoke = on;
                                    }
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Button::new("confirm-delete")
                            .label("Delete")
                            .danger()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete(cx);
                            })),
                    )
                    .into_any_element(),
            ),
            Button::new("cancel-delete")
                .icon(gpui_kit::assets::IconName::X)
                .ghost()
                .small()
                .tooltip("Cancel")
                .accessibility_label("Cancel delete")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.cancel_delete(cx);
                })),
            cx,
        )
    }

    pub(super) fn composer_reply_banner(
        &self,
        reply: &ComposerReplyTo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Telegram Desktop: "Reply to {sender}" over the message preview,
        // custom emoji included.
        let message = self
            .session()
            .and_then(|s| s.histories.get(&reply.chat_id.0))
            .and_then(|h| h.messages.get(&reply.message_id.0))
            .cloned();
        let sender = message
            .as_ref()
            .and_then(|m| m.sender)
            .and_then(|sender| self.session().map(|s| (s, sender)))
            .and_then(|(session, sender)| match sender {
                quill::telegram::envelope::MessageSender::User { user_id } => {
                    session.user(user_id).map(|u| u.display_name())
                }
                quill::telegram::envelope::MessageSender::Chat { chat_id } => {
                    session.chats.get(&chat_id).map(|c| c.title.clone())
                }
            });
        // A message from another chat names that chat.
        let from_chat = (self.open_chat_id() != Some(reply.chat_id))
            .then(|| {
                self.session()
                    .and_then(|s| s.chats.get(&reply.chat_id.0))
                    .map(|chat| chat.title.clone())
            })
            .flatten();
        let title = quill::reply_options::reply_bar_title(sender.as_deref(), from_chat.as_deref());
        // Slice G1: show the quoted part when the reply carries one.
        let preview: AnyElement = match (&reply.quote, message.as_ref().map(|m| &m.content)) {
            (Some(quote), _) => div()
                .text_sm()
                .truncate()
                .child(super::bidi_line::one_line_plain(
                    super::search_ui::one_line_preview(&format!("❝{}❞", quote.text)),
                ))
                .into_any_element(),
            (None, Some(quill::telegram::envelope::MessageContent::Text(text))) => {
                let emoji = self.custom_emoji_images(&text.entities, cx);
                div()
                    .text_sm()
                    .child(super::chatlist_style::chat_list_preview_line(
                        None,
                        &text.text,
                        &text.entities,
                        &emoji,
                        cx,
                    ))
                    .into_any_element()
            }
            _ => div()
                .text_sm()
                .truncate()
                .child(super::bidi_line::one_line_plain(
                    super::search_ui::one_line_preview(&reply.preview),
                ))
                .into_any_element(),
        };
        composer_context_bar_rich(
            "composer-reply-quote",
            gpui_kit::assets::IconName::Reply,
            accent().into(),
            title,
            preview,
            self.bar_thumbnail(reply.chat_id, reply.message_id, cx),
            Some(self.reply_options_button(reply, cx).into_any_element()),
            Button::new("cancel-reply")
                .icon(gpui_kit::assets::IconName::X)
                .ghost()
                .small()
                .tooltip("Cancel reply")
                .accessibility_label("Cancel reply")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.clear_reply(cx);
                })),
            cx,
        )
    }
}

/// Slim bar above the composer for what the next send will do (reply,
/// edit, delete confirm): icon, accent rule, title and one-line preview,
/// optional trailing controls, and a close button.
#[allow(clippy::too_many_arguments)]
fn composer_context_bar(
    id: &'static str,
    icon: gpui_kit::assets::IconName,
    color: Hsla,
    title: impl Into<SharedString>,
    preview: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    close: Button,
    cx: &App,
) -> impl IntoElement {
    let preview: SharedString = preview.into();
    composer_context_bar_rich(
        id,
        icon,
        color,
        title,
        div()
            .text_sm()
            .truncate()
            .text_color(cx.theme().muted_foreground)
            .child(super::bidi_line::one_line_plain(
                super::search_ui::one_line_preview(&preview),
            ))
            .into_any_element(),
        None,
        trailing,
        close,
        cx,
    )
}

/// [`composer_context_bar`] with a rendered preview (custom emoji, quotes)
/// and, for replied-to or edited media, a small rounded thumbnail
/// (`st::historyReplyPreview`) ahead of the text.
#[allow(clippy::too_many_arguments)]
fn composer_context_bar_rich(
    id: &'static str,
    icon: gpui_kit::assets::IconName,
    color: Hsla,
    title: impl Into<SharedString>,
    preview: AnyElement,
    thumb: Option<AnyElement>,
    trailing: Option<AnyElement>,
    close: Button,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .py_1()
        .child(Icon::new(icon).size(px(18.)).text_color(color))
        .child(
            div()
                .flex()
                .items_center()
                // `st::msgReplyBarSkip`.
                .gap(px(10.))
                .flex_1()
                .min_w_0()
                .pl_2()
                .border_l_2()
                .border_color(color)
                .children(thumb)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(color)
                                .truncate()
                                .child(super::bidi_line::one_line_plain(title.into())),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_color(cx.theme().muted_foreground)
                                .child(preview),
                        ),
                ),
        )
        .when_some(trailing, |this, trailing| this.child(trailing))
        .child(close)
}

impl QuillApp {
    /// The paperclip: a menu of what can be attached (Telegram Desktop's
    /// attach button), instead of a permanent row of text buttons.
    pub(super) fn attach_menu_button(
        &self,
        polls_allowed: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let gifs_open = self.gif_panel_open();
        // B15: "Checklist" is offered only to Premium accounts in chats
        // where checklists can be sent (`PeerData::canCreateTodoLists`).
        let checklists_allowed = self.checklist_creation_allowed();
        // The dice the server offers (`updateDiceEmojis`).
        let dice = self
            .session()
            .map(|s| s.sync.dice_menu())
            .unwrap_or_default();
        // Mini apps: the attachment menu bots that open in this chat
        // (`updateAttachmentMenuBots`, filtered like tdesktop's
        // `PeerMatchesTypes`).
        let attach_bots = self.attachment_menu_bots_for_open_chat();
        Button::new("composer-attach")
            .icon(IconName::Paperclip)
            .ghost()
            .tooltip("Attach")
            .accessibility_label("Attach")
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, window, cx| {
                for (label, icon, as_files) in [
                    ("Photo or video", IconName::Image, false),
                    ("File", IconName::File, true),
                ] {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).icon(icon).on_click(
                        move |_, _, cx| {
                            let _ =
                                owner.update(cx, |this, cx| this.pick_attachments(as_files, cx));
                        },
                    ));
                }
                // Telegram Desktop records round videos from the record
                // button; this item starts one too, so it is easy to find.
                let video_owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new("Video message")
                        .icon(IconName::Video)
                        .on_click(move |_, _, cx| {
                            let _ = video_owner
                                .update(cx, |this, cx| this.start_video_note_recording(cx));
                        }),
                );
                let poll_owner = owner.clone();
                let checklist_owner = owner.clone();
                let gif_owner = owner.clone();
                let contact_owner = owner.clone();
                let location_owner = owner.clone();
                let mut menu = menu.separator();
                if checklists_allowed {
                    menu = menu.item(
                        PopupMenuItem::new("Checklist")
                            .icon(IconName::CircleCheck)
                            .on_click(move |_, window, cx| {
                                let _ = checklist_owner
                                    .update(cx, |this, cx| this.open_checklist_dialog(window, cx));
                            }),
                    );
                }
                menu.item(
                    PopupMenuItem::new(if polls_allowed {
                        "Poll"
                    } else {
                        "Polls are restricted here"
                    })
                    .icon(IconName::ChartBar)
                    .disabled(!polls_allowed)
                    .on_click(move |_, window, cx| {
                        let _ = poll_owner.update(cx, |this, cx| this.open_poll_dialog(window, cx));
                    }),
                )
                .item(PopupMenuItem::new("Contact").icon(IconName::User).on_click(
                    move |_, window, cx| {
                        let _ = contact_owner
                            .update(cx, |this, cx| this.open_share_contact_panel(window, cx));
                    },
                ))
                .item(PopupMenuItem::new("Location").icon(IconName::Map).on_click(
                    move |_, window, cx| {
                        let _ = location_owner
                            .update(cx, |this, cx| this.open_share_location_panel(window, cx));
                    },
                ))
                .map(|mut menu| {
                    if attach_bots.is_empty() {
                        return menu;
                    }
                    menu = menu.separator();
                    for (bot_id, name) in attach_bots.iter().cloned() {
                        let owner = owner.clone();
                        menu = menu.item(PopupMenuItem::new(name).icon(IconName::Bot).on_click(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.open_attachment_menu_bot(bot_id, cx)
                                });
                            },
                        ));
                    }
                    menu
                })
                .submenu("Dice", window, cx, {
                    let owner = owner.clone();
                    let dice = dice.clone();
                    move |mut sub, _, _| {
                        for emoji in &dice {
                            let owner = owner.clone();
                            let rolled = emoji.clone();
                            sub = sub.item(PopupMenuItem::new(format!("Roll {emoji}")).on_click(
                                move |_, _, cx| {
                                    let _ =
                                        owner.update(cx, |this, cx| this.roll_dice(&rolled, cx));
                                },
                            ));
                        }
                        sub
                    }
                })
                .item(
                    PopupMenuItem::new("GIFs")
                        .icon(IconName::SquarePlay)
                        .checked(gifs_open)
                        .on_click(move |_, _, cx| {
                            let _ = gif_owner.update(cx, |this, cx| {
                                this.toggle_media_panel(super::media_panel::PanelTab::Gifs, cx)
                            });
                        }),
                )
            })
    }

    /// Picked files waiting to be sent: removable cards, plus the album and
    /// self-destruct options that apply to them.
    pub(super) fn composer_attachment_tray(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut group =
            AttachmentGroup::new("composer-attachments").with_edge_fade(cx.theme().background);
        for (index, attachment) in self.pending_attachments.iter().enumerate() {
            let (icon, kind) = match attachment.kind {
                AttachmentKind::Photo => (IconName::Image, "Photo"),
                AttachmentKind::Video => (IconName::Film, "Video"),
                AttachmentKind::Document => (IconName::File, "File"),
                AttachmentKind::VideoNote => (IconName::Video, "Video message"),
            };
            // Photos and videos show as preview tiles (the photo itself),
            // files as compact rows — Telegram Desktop's send box shows the
            // picture rather than the file name.
            let is_media = matches!(
                attachment.kind,
                AttachmentKind::Photo | AttachmentKind::Video
            );
            // An image sent as a file still previews as the picture.
            let image_file = attachment.kind == AttachmentKind::Document
                && quill::composer::media_kind_for(&attachment.path) == Some(AttachmentKind::Photo);
            let media = match attachment.kind {
                AttachmentKind::Photo => AttachmentMedia::new().src(attachment.path.clone()),
                _ if image_file => AttachmentMedia::new().src(attachment.path.clone()),
                _ => AttachmentMedia::new().child(Icon::new(icon)),
            };
            let spoiler = attachment.spoiler;
            let is_photo = attachment.kind == AttachmentKind::Photo;
            let card = Attachment::new()
                .id(("composer-attach-item", index as u64))
                .tooltip(attachment.file_name.clone());
            let card = if is_media {
                card.axis(Axis::Vertical)
            } else {
                card.small()
            };
            group = group.child(
                card.media(media)
                    .content(
                        AttachmentContent::new()
                            .title(AttachmentTitle::new(attachment.file_name.clone()))
                            .description(AttachmentDescription::new(kind)),
                    )
                    .on_remove(cx.listener(move |this, _, _, cx| {
                        this.remove_attachment(index, cx);
                    }))
                    // Telegram Desktop's "Hide with spoiler" on a photo or
                    // video in the send box.
                    .when(is_media, |card| {
                        card.actions(
                            AttachmentActions::new()
                                .when(is_photo, |actions| {
                                    actions.child(
                                        Button::new(("composer-attach-edit", index as u64))
                                            .icon(IconName::Pencil)
                                            .xsmall()
                                            .ghost()
                                            .tooltip("Edit")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_photo_editor(index, cx);
                                            })),
                                    )
                                })
                                .child(
                                    Button::new(("composer-attach-spoiler", index as u64))
                                        .icon(IconName::EyeOff)
                                        .xsmall()
                                        .ghost()
                                        .selected(spoiler)
                                        .tooltip(if spoiler {
                                            "Remove spoiler"
                                        } else {
                                            "Hide with spoiler"
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_attachment_spoiler(index, cx);
                                        })),
                                ),
                        )
                    }),
            );
        }
        let several = self.pending_attachments.len() >= 2;
        let grouped = self.composer_group_media_effective();
        // Telegram Desktop's "Send without compression".
        let can_files = ComposerAttachment::can_send_as_files(&self.pending_attachments);
        let as_files = can_files
            && self
                .pending_attachments
                .iter()
                .all(|attachment| attachment.kind == AttachmentKind::Document);
        let remember = self
            .session()
            .map(|s| s.media_prefs.remember_media_grouping)
            .unwrap_or(false);
        let timer = self.self_destruct_picker_visible().then(|| {
            let owner = cx.entity().downgrade();
            let current = self.composer_self_destruct;
            Button::new("self-destruct-menu")
                .icon(IconName::Timer)
                .label(self.self_destruct_button_label())
                .ghost()
                .small()
                .tooltip("Self-destruct timer")
                .dropdown_menu(move |mut menu, _, _| {
                    for choice in QuillApp::SELF_DESTRUCT_CHOICES {
                        let owner = owner.clone();
                        let label = match choice {
                            None => "Off".to_string(),
                            Some(SelfDestructSend::Timer(secs)) if secs < 60 => {
                                format!("{secs} seconds")
                            }
                            Some(SelfDestructSend::Timer(secs)) => {
                                format!("{} minute", secs / 60)
                            }
                            Some(SelfDestructSend::Immediately) => "View once".to_string(),
                        };
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(choice == current)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.set_composer_self_destruct(choice, cx);
                                    });
                                }),
                        );
                    }
                    menu
                })
        });
        div()
            .id("composer-attach-tray")
            .flex()
            .flex_col()
            .gap_2()
            .child(group)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_4()
                    .text_sm()
                    .when(can_files, |row| {
                        row.child(
                            Checkbox::new("composer-send-as-files")
                                .label("Send without compression")
                                .checked(as_files)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    ComposerAttachment::set_send_as_files(
                                        &mut this.pending_attachments,
                                        !as_files,
                                    );
                                    cx.notify();
                                })),
                        )
                    })
                    .when(several, |row| {
                        row.child(
                            Checkbox::new("composer-group-media")
                                .label("Send as album")
                                .checked(grouped)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_composer_group_media(cx);
                                })),
                        )
                        .child(
                            Checkbox::new("composer-remember-grouping")
                                .label("Remember choice")
                                .checked(remember)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_remember_media_grouping(cx);
                                })),
                        )
                    })
                    .children(timer)
                    .child(div().flex_1())
                    .child(
                        Button::new("clear-attach")
                            .label(if several { "Remove all" } else { "Remove" })
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.clear_attachment(cx))),
                    ),
            )
    }
}

impl QuillApp {
    /// The `@` suggestions above the composer: avatar, name, @username.
    pub(super) fn mention_menu_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let items = self.mention_menu_items(cx);
        if items.is_empty() {
            return None;
        }
        let selected = self.mention_selected.min(items.len() - 1);
        let mut list = div()
            .id("mention-menu")
            .role(gpui_kit::Role::ListBox)
            .aria_label("Mention suggestions")
            .flex()
            .flex_col()
            .max_w(px(440.))
            .mx_4()
            .mb_2()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md();
        for (index, (user_id, name, username)) in items.into_iter().enumerate() {
            let photo = self
                .session()
                .and_then(|s| s.user_photo_path(user_id))
                .and_then(|path| {
                    quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
                });
            let label = if username.is_empty() {
                name.clone()
            } else {
                format!("{name} @{username}")
            };
            list = list.child(
                div()
                    .id(("mention-item", index as u64))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .role(gpui_kit::Role::ListBoxOption)
                    .aria_label(label)
                    .cursor_pointer()
                    .when(index == selected, |this| this.bg(cx.theme().selection))
                    .hover(|style| style.bg(cx.theme().accent))
                    // On press: the composer's blur closes this menu
                    // before a click could land.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            this.pick_mention_index(index, window, cx);
                        }),
                    )
                    .child(super::message_text::kit_avatar_element(
                        &name,
                        photo.as_deref(),
                        px(28.),
                    ))
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(name)),
                    )
                    .when(!username.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(format!("@{username}")),
                        )
                    }),
            );
        }
        Some(list.into_any_element())
    }
}

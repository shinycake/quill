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
    session.stickers.stickers.open = true;
    session.stickers.stickers.loading_sets = true;
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
    session.stickers.stickers.stickers.clear();
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
    session.settings.media_prefs.loop_animated_stickers =
        std::env::var("QUILL_DEMO_LOOP_STICKERS").as_deref() != Ok("off");
}

impl QuillApp {
    /// Phase 3.3: the `/` command menu popup above the composer.
    /// Bot-specific commands first, then the global (`getCommands`)
    /// section when both exist. Tap inserts; Up/Down/Enter via the key
    /// interceptor; Esc / blur / outside tap dismisses.
    pub(super) fn command_menu_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, items) = self.command_menu_state(cx)?;
        let selected = self.composer_ui.command_menu_selected.min(items.len() - 1);
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
        self.session()
            .is_some_and(|session| session.stickers.gifs.open)
    }

    pub(super) fn close_gif_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_gif_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.gifs.close();
        }
        self.connection.status_note = "GIFs closed".into();
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
            .composer_ui
            .pending_reply
            .as_ref()
            .and_then(|reply| reply.send_target(chat_id));
        let sent = if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.send_animation(
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
            self.connection.status_note == "sending GIF"
        } else if self.demo_session.is_some() {
            self.apply_demo_gif(chat_id, file_id, duration, width, height, reply);
            self.connection.status_note = "demo GIF applied locally (no live Telegram)".into();
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
        self.session()
            .is_some_and(|session| session.stickers.stickers.open)
    }

    pub(super) fn close_sticker_panel(&mut self, cx: &mut Context<Self>) {
        self.settings.sticker_settings_open = false;
        if let Some(live) = self.live.as_mut() {
            live.driver.close_sticker_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.close();
        }
        self.connection.status_note = "stickers closed".into();
        cx.notify();
    }

    pub(super) fn select_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.select_sticker_set(set_id) {
                Ok(_) => "sticker set".into(),
                Err(_) => "could not open sticker set".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.select_sticker_set(set_id);
            self.connection.status_note = "sticker set".into();
        }
        cx.notify();
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
        for (index, attachment) in self.composer_ui.pending_attachments.iter().enumerate() {
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
        let several = self.composer_ui.pending_attachments.len() >= 2;
        let grouped = self.composer_group_media_effective();
        // Telegram Desktop's "Send without compression".
        let can_files =
            ComposerAttachment::can_send_as_files(&self.composer_ui.pending_attachments);
        let as_files = can_files
            && self
                .composer_ui
                .pending_attachments
                .iter()
                .all(|attachment| attachment.kind == AttachmentKind::Document);
        let remember = self
            .session()
            .map(|s| s.settings.media_prefs.remember_media_grouping)
            .unwrap_or(false);
        let timer = self.self_destruct_picker_visible().then(|| {
            let owner = cx.entity().downgrade();
            let current = self.composer_ui.self_destruct;
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
                                        &mut this.composer_ui.pending_attachments,
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
        let selected = self.composer_ui.mention_selected.min(items.len() - 1);
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

mod send_sticker_pick;

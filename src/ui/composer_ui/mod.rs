//! Composer command menus, quote/reply/edit banners, link previews and captions.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::pressable::PressableDiv;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::TestSupportExt;
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
use quill::lang::{keys, tr, tr_with};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
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
                premium_animation: None,
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
        let quote = quote.trim();
        let Some(selection) = self
            .message_quotable(chat_id, message_id)
            .and_then(|text| text.quote_for(quote))
        else {
            // Not verbatim text of the message (it shouldn't happen for a
            // selection); reply to the whole message instead.
            self.begin_reply_from_message(chat_id, message_id, window, cx);
            return;
        };
        let reply = quill::composer::ComposerReplyTo::with_quote(chat_id, message_id, selection);
        self.begin_reply_to(reply, window, cx);
        cx.notify();
    }

    pub(super) fn gif_panel_open(&self) -> bool {
        self.session()
            .is_some_and(|session| session.stickers.gifs.open)
    }

    pub(super) fn close_gif_panel(&mut self, cx: &mut Context<Self>) {
        // The GIFs are a tab of the media panel: closing them closes it.
        if self.close_media_panel(cx) {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.close_gif_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.gifs.close();
        }
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_CHAT_GIFS_CLOSED)));
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
        if self.route_story_reply_gif(file_id, duration, (width, height), cx) {
            return;
        }
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
        let reply = self.send_reply_for(chat_id, cx);
        let sent = if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_animation(
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
            );
            let ok = result.is_ok();
            self.connection.set_note(if ok {
                StatusNote::silent(tr(keys::Q_CHAT_SENDING_GIF))
            } else {
                StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SEND_GIF))
            });
            ok
        } else if self.demo_session.is_some() {
            self.apply_demo_gif(chat_id, file_id, duration, width, height, reply);
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_GIF_APPLIED_LOCALLY_NO,
            )));
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
        if self.settings.sticker_settings_open {
            self.close_stickers_manager(cx);
            return;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.close_sticker_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.close();
        }
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_CHAT_STICKERS_CLOSED)));
        cx.notify();
    }

    pub(super) fn select_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.select_sticker_set(set_id) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_STICKER_SET)),
                    Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_OPEN_STICKER_SET)),
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            session.select_sticker_set(set_id);
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_STICKER_SET)));
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
        None,
        cx,
    )
}

/// What the body of a context bar does when clicked
/// (`quill::composer::bar_click_action`).
pub(super) struct BarClicks {
    pub(super) kind: quill::composer::BarKind,
    pub(super) run: std::rc::Rc<dyn Fn(quill::composer::BarAction, &mut Window, &mut App)>,
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
    clicks: Option<BarClicks>,
    cx: &App,
) -> impl IntoElement {
    use quill::composer::{BarClick, bar_click_action};
    let body = div()
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
        );
    // The icon and text are one target; the trailing buttons are not.
    let target = div()
        .id((id, 1usize))
        .test_support()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_3()
        .child(Icon::new(icon).size(px(18.)).text_color(color))
        .child(body)
        .when_some(clicks, |this, clicks| {
            let (kind, run) = (clicks.kind, clicks.run);
            let on_right = run.clone();
            this.cursor_pointer()
                .on_click(move |event, window, cx| {
                    let click = if event.modifiers().secondary(cx) {
                        BarClick::LeftModified
                    } else {
                        BarClick::Left
                    };
                    run(bar_click_action(kind, click), window, cx);
                })
                .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                    on_right(bar_click_action(kind, BarClick::Right), window, cx);
                })
        });
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .py_1()
        .child(target)
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
            .tooltip(tr(keys::Q_CHAT_ATTACH))
            .accessibility_label(tr(keys::Q_CHAT_ATTACH))
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
                    (tr(keys::Q_CHAT_PHOTO_OR_VIDEO), IconName::Image, false),
                    (tr(keys::Q_CHAT_FILE), IconName::File, true),
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
                    PopupMenuItem::new(tr(keys::IN_DLG_VIDEO_MESSAGE))
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
                        PopupMenuItem::new(tr(keys::Q_CHAT_CHECKLIST))
                            .icon(IconName::CircleCheck)
                            .on_click(move |_, window, cx| {
                                let _ = checklist_owner
                                    .update(cx, |this, cx| this.open_checklist_dialog(window, cx));
                            }),
                    );
                }
                menu.item(
                    PopupMenuItem::new(if polls_allowed {
                        tr(keys::POLLS_MENU_ITEM)
                    } else {
                        tr(keys::Q_CHAT_POLLS_ARE_RESTRICTED_HERE)
                    })
                    .icon(IconName::ChartBar)
                    .disabled(!polls_allowed)
                    .on_click(move |_, window, cx| {
                        let _ = poll_owner.update(cx, |this, cx| this.open_poll_dialog(window, cx));
                    }),
                )
                .item(
                    PopupMenuItem::new(tr(keys::Q_CHAT_CONTACT))
                        .icon(IconName::User)
                        .on_click(move |_, window, cx| {
                            let _ = contact_owner
                                .update(cx, |this, cx| this.open_share_contact_panel(window, cx));
                        }),
                )
                .item(
                    PopupMenuItem::new(tr(keys::LOCATION_TITLE))
                        .icon(IconName::Map)
                        .on_click(move |_, _, cx| {
                            let _ =
                                location_owner.update(cx, |this, cx| this.open_location_picker(cx));
                        }),
                )
                .map(|mut menu| {
                    if attach_bots.is_empty() {
                        return menu;
                    }
                    menu = menu.separator();
                    for entry in attach_bots.iter().cloned() {
                        let owner = owner.clone();
                        let bot_id = entry.bot_id;
                        let item = PopupMenuItem::new(entry.name);
                        let item = match entry.icon_svg {
                            Some(svg) => item.icon(Icon::default().data(&svg)),
                            None => item.icon(IconName::Bot),
                        };
                        menu = menu.item(item.on_click(move |_, _, cx| {
                            let _ = owner
                                .update(cx, |this, cx| this.open_attachment_menu_bot(bot_id, cx));
                        }));
                    }
                    menu
                })
                .submenu(tr(keys::SR_MESSAGE_COLUMN_DICE), window, cx, {
                    let owner = owner.clone();
                    let dice = dice.clone();
                    move |mut sub, _, _| {
                        for emoji in &dice {
                            let owner = owner.clone();
                            let rolled = emoji.clone();
                            sub = sub.item(
                                PopupMenuItem::new(tr_with(
                                    keys::Q_CHAT_ROLL,
                                    &[("emoji", &emoji.to_string())],
                                ))
                                .on_click(move |_, _, cx| {
                                    let _ =
                                        owner.update(cx, |this, cx| this.roll_dice(&rolled, cx));
                                }),
                            );
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
        for (index, attachment) in self.pending_attachments().iter().enumerate() {
            let (icon, kind) = match attachment.kind {
                AttachmentKind::Photo => (IconName::Image, tr(keys::Q_CHAT_PHOTO)),
                AttachmentKind::Video => (IconName::Film, tr(keys::Q_CHAT_VIDEO)),
                AttachmentKind::Document => (IconName::File, tr(keys::Q_CHAT_FILE)),
                AttachmentKind::VideoNote => (IconName::Video, tr(keys::IN_DLG_VIDEO_MESSAGE)),
            };
            // Photos and videos show as preview tiles (the photo itself),
            // files as compact rows — Telegram Desktop's send box shows the
            // picture rather than the file name.
            let is_media = matches!(
                attachment.kind,
                AttachmentKind::Photo | AttachmentKind::Video
            );
            // An image sent as a file still previews as the picture; a
            // HEIC, AVIF or other picture the tile can't read shows the
            // thumbnail made for sending (`send_box_thumbnails.rs`).
            let image_file = attachment.kind == AttachmentKind::Document
                && quill::composer::media_kind_for(&attachment.path) == Some(AttachmentKind::Photo);
            let file_thumbnail = (attachment.kind == AttachmentKind::Document)
                .then_some(attachment.extras.file_thumbnail.as_ref())
                .flatten();
            // Like tdesktop, only the first album's worth of files decode
            // a preview; the rest show their kind.
            let previewed = index < quill::album::ALBUM_MAX_ITEMS;
            let media = match (attachment.kind, file_thumbnail) {
                _ if !previewed => AttachmentMedia::new().child(Icon::new(icon)),
                (AttachmentKind::Photo, _) => AttachmentMedia::new().src(attachment.path.clone()),
                (_, Some(thumb)) => AttachmentMedia::new().src(thumb.path.clone()),
                _ if image_file => AttachmentMedia::new().src(attachment.path.clone()),
                _ => AttachmentMedia::new().child(Icon::new(icon)),
            };
            let spoiler = attachment.spoiler;
            let is_photo = attachment.kind == AttachmentKind::Photo;
            let rendering = attachment.extras.photo_render.is_some();
            // "Edit video" and "Cover" on a video, no spoiler on paid media
            // (`send_box_options.rs`).
            let video_actions = self.video_card_actions(index, attachment, cx);
            let paid_price = self.tray_paid_price_set();
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
                                            .map(|button| {
                                                // Its edit is still being saved.
                                                if rendering {
                                                    button
                                                        .icon(spinner::Spinner::new())
                                                        .debug_selector(move || {
                                                            format!(
                                                                "composer-attach-rendering-{index}"
                                                            )
                                                        })
                                                } else {
                                                    button.icon(IconName::Pencil)
                                                }
                                            })
                                            .xsmall()
                                            .ghost()
                                            .tooltip(tr(keys::Q_CHAT_EDIT))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_photo_editor(index, cx);
                                            })),
                                    )
                                })
                                .children(video_actions)
                                .when(!paid_price, |actions| {
                                    actions.child(
                                        Button::new(("composer-attach-spoiler", index as u64))
                                            .icon(IconName::EyeOff)
                                            .xsmall()
                                            .ghost()
                                            .selected(spoiler)
                                            .tooltip(if spoiler {
                                                tr(keys::Q_CHAT_REMOVE_SPOILER)
                                            } else {
                                                tr(keys::Q_CHAT_HIDE_WITH_SPOILER)
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.toggle_attachment_spoiler(index, cx);
                                            })),
                                    )
                                }),
                        )
                    }),
            );
        }
        let several = self.pending_attachments().len() >= 2;
        let grouped = self.composer_group_media_effective();
        // Telegram Desktop's "Send without compression".
        let can_files = ComposerAttachment::can_send_as_files(self.pending_attachments());
        let as_files = can_files
            && self
                .pending_attachments()
                .iter()
                .all(|attachment| attachment.kind == AttachmentKind::Document);
        let remember = self
            .session()
            .map(|s| s.settings.media_prefs.remember_media_grouping)
            .unwrap_or(false);
        let timer = self.self_destruct_picker_visible().then(|| {
            let owner = cx.entity().downgrade();
            let current = self.pending_send().self_destruct;
            Button::new("self-destruct-menu")
                .icon(IconName::Timer)
                .label(self.self_destruct_button_label())
                .ghost()
                .small()
                .tooltip(tr(keys::Q_CHAT_SELF_DESTRUCT_TIMER))
                .dropdown_menu(move |mut menu, _, _| {
                    for choice in QuillApp::SELF_DESTRUCT_CHOICES {
                        let owner = owner.clone();
                        let label = match choice {
                            None => tr(keys::Q_CHAT_OFF).to_string(),
                            Some(SelfDestructSend::Timer(secs)) if secs < 60 => {
                                format!("{secs} seconds")
                            }
                            Some(SelfDestructSend::Timer(secs)) => {
                                format!("{} minute", secs / 60)
                            }
                            Some(SelfDestructSend::Immediately) => {
                                tr(keys::Q_CHAT_VIEW_ONCE).to_string()
                            }
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
                                .label(tr(keys::Q_CHAT_SEND_WITHOUT_COMPRESSION))
                                .checked(as_files)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.update_pending_attachments(|tray| {
                                        ComposerAttachment::set_send_as_files(tray, !as_files)
                                    });
                                    // Back to media: read the videos again.
                                    this.composer_ui.send_box.sound_checked.clear();
                                    this.check_tray_videos(cx);
                                    cx.notify();
                                })),
                        )
                    })
                    .when(several, |row| {
                        row.child(
                            Checkbox::new("composer-group-media")
                                .label(tr(keys::Q_CHAT_SEND_AS_ALBUM))
                                .checked(grouped)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_composer_group_media(cx);
                                })),
                        )
                        .child(
                            Checkbox::new("composer-remember-grouping")
                                .label(tr(keys::Q_CHAT_REMEMBER_CHOICE))
                                .checked(remember)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_remember_media_grouping(cx);
                                })),
                        )
                    })
                    .children(self.send_box_option_controls(cx))
                    .children(timer)
                    .child(div().flex_1())
                    .child(
                        Button::new("clear-attach")
                            .label(if several {
                                tr(keys::Q_CHAT_REMOVE_ALL)
                            } else {
                                tr(keys::BOX_REMOVE)
                            })
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
            .aria_label(tr(keys::Q_CHAT_MENTION_SUGGESTIONS))
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

mod command_menu_view;
mod send_sticker_pick;
mod text_limit;
pub(in crate::ui) use text_limit::TextLimitMemo;

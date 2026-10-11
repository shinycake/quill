//! Methods moved out of `composer_ui.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn send_sticker_pick(
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
            self.connection.status_note = "Sending this sticker requires Telegram Premium".into();
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
            .composer_ui
            .pending_reply
            .as_ref()
            .and_then(|reply| reply.send_target(chat_id));
        let sent = if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.send_sticker(
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
            self.connection.status_note == "sticker sent"
        } else if self.demo_session.is_some() {
            self.apply_demo_sticker(chat_id, &emoji, file_id, reply);
            self.connection.status_note = "sticker sent".into();
            true
        } else {
            false
        };
        if sent {
            self.consume_sent_reply(chat_id, cx);
        }
        cx.notify();
    }

    pub(in crate::ui) fn composer_edit_banner(
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
    pub(in crate::ui) fn maybe_prefetch_link_preview(&mut self, cx: &mut Context<Self>) {
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
                && live.driver.session.messages.composer_preview.is_some()
            {
                live.driver.session.messages.composer_preview = None;
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
            .and_then(|live| live.driver.session.messages.composer_preview.as_ref())
            .is_some_and(|p| p.url == url);
        if already {
            return;
        }
        self.composer_ui.preview_token = self.composer_ui.preview_token.wrapping_add(1);
        let token = self.composer_ui.preview_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            this.update(cx, |this, cx| {
                if this.composer_ui.preview_token != token {
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
                    this.connection.status_note = "couldn't load link preview".into();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// MED4b: TGX `LinkPreview.hasMedia` — the chip's size toggle needs
    /// *some* media to resize (photo, embedded player, album strip).
    pub(in crate::ui) fn preview_has_media(
        preview: &quill::telegram::envelope::LinkPreview,
    ) -> bool {
        preview.photo.is_some()
            || !matches!(
                preview.kind,
                quill::telegram::envelope::LinkPreviewKind::Plain
            )
    }

    /// B5: the link-preview choices in effect: the edited text message's
    /// while editing (tdesktop seeds the edit draft with the message's own
    /// `WebPageDraft`), otherwise the next send's.
    pub(in crate::ui) fn preview_choice(&self) -> LinkPreviewChoice {
        match self.composer_ui.pending_edit.as_ref() {
            Some(edit) => edit.link_preview,
            None => LinkPreviewChoice {
                disabled: self.composer_ui.preview_disabled,
                above_text: self.composer_ui.preview_above,
                media: self.composer_ui.preview_media,
                link_index: self.composer_ui.preview_link,
            },
        }
    }

    pub(in crate::ui) fn set_preview_choice(&mut self, choice: LinkPreviewChoice) {
        match self.composer_ui.pending_edit.as_mut() {
            Some(edit) => edit.link_preview = choice,
            None => {
                self.composer_ui.preview_disabled = choice.disabled;
                self.composer_ui.preview_above = choice.above_text;
                self.composer_ui.preview_media = choice.media;
                self.composer_ui.preview_link = choice.link_index;
            }
        }
    }

    /// The link the preview is generated from: the one picked in the
    /// options menu, else the first (clamped to the links present).
    pub(in crate::ui) fn chosen_preview_url(&self, text: &str) -> Option<String> {
        let urls = find_urls(text);
        let index = self.preview_choice().link_index;
        urls.get(index).or_else(|| urls.last()).cloned()
    }

    /// B5: tdesktop's "Link Preview Settings" popover (history_view_draft_options):
    /// click a link to generate its preview, Move Up/Down, Shrink/Enlarge
    /// the media, Do Not Preview. Maps to `linkPreviewOptions`
    /// (`url`, `show_above_text`, `force_small_media` / `force_large_media`,
    /// `is_disabled`).
    pub(super) fn link_options_menu(
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
                    this.connection.status_note = note.into();
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
    pub(in crate::ui) fn preview_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing_text = self
            .composer_ui
            .pending_edit
            .as_ref()
            .is_some_and(|edit| matches!(edit.kind, quill::composer::ComposerEditKind::Text));
        if !self.composer_ui.pending_attachments.is_empty()
            || (self.composer_ui.pending_edit.is_some() && !editing_text)
        {
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
            .and_then(|s| s.messages.composer_preview.clone())
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
    pub(in crate::ui) fn edit_replacement_chip(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let edit = self.composer_ui.pending_edit.as_ref()?;
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
    pub(in crate::ui) fn caption_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing_caption =
            self.composer_ui.pending_edit.as_ref().is_some_and(|edit| {
                matches!(edit.kind, quill::composer::ComposerEditKind::Caption)
            });
        if self.composer_ui.pending_attachments.is_empty() && !editing_caption {
            return None;
        }
        // Documents and music have no caption position; photos, videos and
        // GIFs do (`show_caption_above_media`), also after a replacement.
        let captionable = self
            .composer_ui
            .pending_edit
            .as_ref()
            .is_some_and(|edit| editing_caption && edit.caption_position_applies())
            || self.composer_ui.pending_attachments.iter().any(|att| {
                matches!(
                    att.kind,
                    quill::composer::AttachmentKind::Photo | quill::composer::AttachmentKind::Video
                )
            });
        let above = if editing_caption {
            self.composer_ui
                .pending_edit
                .as_ref()
                .is_some_and(|edit| edit.caption_above)
        } else {
            self.composer_ui.caption_above
        };
        let text_len = self.composer.read(cx).value().chars().count();
        let limit = self
            .live
            .as_ref()
            .map(|live| live.driver.session.messages.message_caption_length_max)
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
                                        if let Some(edit) = this.composer_ui.pending_edit.as_mut() {
                                            edit.caption_above = !edit.caption_above;
                                        } else {
                                            this.composer_ui.caption_above =
                                                !this.composer_ui.caption_above;
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
    pub(in crate::ui) fn text_limit_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.composer_ui.pending_attachments.is_empty() {
            return None;
        }
        let editing = self.composer_ui.pending_edit.as_ref();
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

    pub(in crate::ui) fn delete_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let confirm = self.message_ui.pending_delete.clone();
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
                                    if let Some(confirm) = this.message_ui.pending_delete.as_mut() {
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

    pub(in crate::ui) fn composer_reply_banner(
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

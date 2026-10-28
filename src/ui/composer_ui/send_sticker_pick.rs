//! Methods moved out of `composer_ui.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

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
        if self.route_story_reply_sticker(file_id, &emoji, (width, height), thumb, cx) {
            return;
        }
        if self.route_business_start_page_sticker(file_id, cx) {
            return;
        }
        if self.route_poll_option_sticker(file_id, &emoji, (width, height), cx) {
            return;
        }
        if self.premium_sticker_blocked(file_id, cx) {
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
        let reply = self.send_reply_for(chat_id, cx);
        let sent = if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_sticker(
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
            );
            let ok = result.is_ok();
            self.connection.set_note(if ok {
                StatusNote::silent(tr(keys::Q_CHAT_STICKER_SENT))
            } else {
                StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SEND_STICKER))
            });
            ok
        } else if self.demo_session.is_some() {
            self.apply_demo_sticker(chat_id, &emoji, file_id, reply);
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_STICKER_SENT)));
            true
        } else {
            false
        };
        if sent {
            self.consume_sent_reply(chat_id, cx);
            self.arm_send_fly(chat_id, file_id, cx);
        }
        cx.notify();
    }

    pub(in crate::ui) fn composer_edit_banner(
        &self,
        edit: &ComposerEdit,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // The line follows the live message: an edit elsewhere shows here.
        let preview = self
            .session()
            .and_then(|session| session.edit_target_text(edit))
            .unwrap_or_default();
        let kind = match edit.kind {
            quill::composer::ComposerEditKind::Text => tr(keys::Q_CHAT_EDITING_MESSAGE),
            quill::composer::ComposerEditKind::Caption => tr(keys::Q_CHAT_EDITING_CAPTION),
        };
        // An edited photo or video shows its small preview, as tdesktop's
        // field header does.
        let thumb = self.bar_thumbnail(edit.chat_id, edit.message_id, cx);
        // B5: tdesktop `EditCaptionBox` "Replace attachment".
        let replace_button = edit.allows_replace().then(|| {
            Button::new("composer-edit-replace")
                .icon(gpui_kit::assets::IconName::Paperclip)
                .label(tr(keys::Q_CHAT_REPLACE))
                .ghost()
                .small()
                .tooltip(tr(keys::Q_CHAT_REPLACE_ATTACHMENT))
                .accessibility_label(tr(keys::Q_CHAT_REPLACE_ATTACHMENT))
                .on_click(cx.listener(|this, _, _, cx| this.pick_edit_replacement(cx)))
                .into_any_element()
        });
        let replace_button = self.edit_media_trailing(edit, replace_button, cx);
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
                .tooltip(tr(keys::Q_CHAT_CANCEL_EDITING))
                .accessibility_label(tr(keys::Q_CHAT_CANCEL_EDITING))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.clear_edit(window, cx);
                })),
            Some(self.bar_clicks(
                quill::composer::BarKind::Edit,
                (!edit.scheduled).then_some(edit.message_id),
                cx,
            )),
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
        self.spawn_scoped(cx, async move |this, cx| {
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
                    this.connection.set_note(StatusNote::error(tr(
                        keys::Q_CHAT_COULDNT_LOAD_LINK_PREVIEW,
                    )));
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
            None => self.pending_send().preview,
        }
    }

    pub(in crate::ui) fn set_preview_choice(&mut self, choice: LinkPreviewChoice) {
        match self.composer_ui.pending_edit.as_mut() {
            Some(edit) => edit.link_preview = choice,
            None => {
                self.update_pending_send(|send| send.preview = choice);
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
                    this.connection.set_note(StatusNote::info(note));
                }
                cx.notify();
            });
        }
        menu = menu.label(tr(keys::Q_CHAT_LINK_PREVIEW_SETTINGS));
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
                PopupMenuItem::new(if above {
                    tr(keys::Q_CHAT_MOVE_DOWN)
                } else {
                    tr(keys::Q_CHAT_MOVE_UP)
                })
                .on_click(move |_, _, cx| {
                    apply(
                        &toggle,
                        cx,
                        Some(if above {
                            tr(keys::Q_CHAT_LINK_PREVIEW_WILL_APPEAR_BELOW)
                        } else {
                            tr(keys::Q_CHAT_LINK_PREVIEW_WILL_APPEAR_ABOVE)
                        }),
                        |c| c.above_text = !above,
                    );
                }),
            );
            if let Some((large, noun)) = size_toggle {
                let resize = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(format!(
                        "{} {noun}",
                        if large {
                            tr(keys::Q_CHAT_SHRINK)
                        } else {
                            tr(keys::Q_CHAT_ENLARGE)
                        }
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
                tr(keys::Q_CHAT_SHOW_PREVIEW)
            } else {
                tr(keys::Q_CHAT_DO_NOT_PREVIEW)
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
        if !self.pending_attachments().is_empty()
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
                Some(None) => Some(tr(keys::Q_CHAT_NO_PREVIEW_FOR_THIS_LINK).to_string()),
                // Request in flight.
                None => Some(tr(keys::Q_CHAT_GETTING_LINK_INFO).to_string()),
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
                    tr(keys::Q_CHAT_VIDEO)
                } else {
                    tr(keys::Q_CHAT_PHOTO)
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
                    .child(tr(keys::Q_CHAT_PREVIEW_OFF)),
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
                .label(tr(keys::Q_CHAT_LINK_OPTIONS))
                .ghost()
                .small()
                .tooltip(tr(keys::Q_CHAT_LINK_PREVIEW_SETTINGS))
                .accessibility_label(tr(keys::Q_CHAT_LINK_PREVIEW_SETTINGS))
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
        self.composer_ui
            .pending_edit
            .as_ref()?
            .media_edit
            .replacement
            .as_ref()?;
        self.edit_replacement_chip_body(cx)
    }

    /// `edit_replacement_chip` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn edit_replacement_chip_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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
                        .child(tr_with(
                            keys::Q_LEFTOVERS_REPLACING_ATTACHMENT_WITH,
                            &[("name", &replacement.file_name.to_string())],
                        )),
                )
                .when(as_file_toggle, |this| {
                    this.child(
                        Checkbox::new("composer-edit-replace-as-file")
                            .label(tr(keys::SEND_AS_DOCUMENTS_ONE))
                            .checked(as_file)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_edit_replace_as_file(cx);
                            })),
                    )
                })
                .when(spoiler_toggle, |this| {
                    this.child(
                        Checkbox::new("composer-edit-replace-spoiler")
                            .label(tr(keys::MENU_FORMATTING_SPOILER))
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
                        .tooltip(tr(keys::Q_CHAT_KEEP_THE_ORIGINAL_ATTACHMENT))
                        .accessibility_label(tr(keys::Q_CHAT_KEEP_THE_ORIGINAL_ATTACHMENT))
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
        if self.pending_attachments().is_empty() && !editing_caption {
            return None;
        }
        // Documents and music have no caption position; photos, videos and
        // GIFs do (`show_caption_above_media`), also after a replacement.
        let captionable = self
            .composer_ui
            .pending_edit
            .as_ref()
            .is_some_and(|edit| editing_caption && edit.caption_position_applies())
            || self.pending_attachments().iter().any(|att| {
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
            self.pending_send().caption_above
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
                                .child(tr(keys::Q_CHAT_ADD_A_CAPTION)),
                        )
                        .when(captionable && !self.open_chat_is_secret(), |this| {
                            this.child(
                                Checkbox::new("composer-caption-above")
                                    .label(tr(keys::Q_CHAT_CAPTION_ABOVE_MEDIA))
                                    .checked(above)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(edit) = this.composer_ui.pending_edit.as_mut() {
                                            edit.caption_above = !edit.caption_above;
                                        } else {
                                            this.update_pending_send(|send| {
                                                send.caption_above = !send.caption_above
                                            });
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
        if !self.pending_attachments().is_empty() {
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
            (
                tr_with(
                    keys::Q_CHAT_WILL_BE_SENT_AS_MESSAGES,
                    &[("parts", &parts.to_string())],
                ),
                text_muted(),
            )
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
        // A paid suggested post deleted too early loses its payment.
        let warning = self.paid_post_delete_warning();
        composer_context_bar(
            "delete-confirm",
            gpui_kit::assets::IconName::Trash,
            danger().into(),
            warning.map_or(tr(keys::Q_CHAT_DELETE_MESSAGE), |(title, _)| title),
            match warning {
                Some((_, text)) => text.to_string(),
                None if can_revoke && revoke => tr(keys::Q_CHAT_IT_WILL_BE_DELETED_FOR).to_string(),
                None => tr(keys::Q_CHAT_IT_WILL_BE_DELETED_FOR_2).to_string(),
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
                                .label(tr(keys::Q_CHAT_DELETE_FOR_EVERYONE))
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
                            .label(if warning.is_some() {
                                quill::suggested_posts::DELETE_ANYWAY
                            } else {
                                tr(keys::BOX_DELETE)
                            })
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
                .tooltip(tr(keys::CANCEL))
                .accessibility_label(tr(keys::Q_CHAT_CANCEL_DELETE))
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
            .and_then(|s| s.reply_target(reply.chat_id, reply.message_id))
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
        // A reply to one checklist task or poll option shows its text.
        let item_line = message.as_ref().and_then(|m| {
            quill::composer::reply_item::item_text(
                quill::telegram::envelope::effective_content(&m.content, m.ephemeral.as_ref()),
                &self
                    .session()
                    .map_or_else(|| reply.item.clone(), |s| reply.effective_item(s)),
            )
        });
        // Slice G1: show the quoted part when the reply carries one.
        let preview: AnyElement = match (&reply.quote, message.as_ref().map(|m| &m.content)) {
            _ if item_line.is_some() => div()
                .text_sm()
                .truncate()
                .child(super::bidi_line::one_line_plain(
                    super::search_ui::one_line_preview(&item_line.unwrap_or_default()),
                ))
                .into_any_element(),
            // The quote as it reads in the message, with its formatting and
            // no quote marks (tdesktop `FieldHeader::updateShownMessageText`).
            (Some(quote), _) => {
                let emoji = self.custom_emoji_images(
                    &quote.entities,
                    super::sticker_player::EmojiSide(14.),
                    cx,
                );
                div()
                    .text_sm()
                    .child(super::chatlist_style::chat_list_preview_line(
                        None,
                        &quote.text,
                        &quote.entities,
                        &emoji,
                        cx,
                    ))
                    .into_any_element()
            }
            (None, Some(quill::telegram::envelope::MessageContent::Text(text))) => {
                let emoji = self.custom_emoji_images(
                    &text.entities,
                    super::sticker_player::EmojiSide(14.),
                    cx,
                );
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
            _ => {
                // The message's own preview, or "Loading…" while it is not
                // in the history (tdesktop `Reply` state text).
                let line = message
                    .as_ref()
                    .map(quill::state::effective_preview)
                    .unwrap_or_else(|| tr(keys::PROFILE_LOADING).to_string());
                div()
                    .text_sm()
                    .truncate()
                    .child(super::bidi_line::one_line_plain(
                        super::search_ui::one_line_preview(&line),
                    ))
                    .into_any_element()
            }
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
                .tooltip(tr(keys::Q_CHAT_CANCEL_REPLY))
                .accessibility_label(tr(keys::Q_CHAT_CANCEL_REPLY))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.clear_reply(cx);
                })),
            Some(self.bar_clicks(
                self.reply_bar_kind(reply),
                (self.open_chat_id() == Some(reply.chat_id)).then_some(reply.message_id),
                cx,
            )),
            cx,
        )
    }
}

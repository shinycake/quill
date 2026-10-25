//! Methods moved out of `media_panel.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    pub(super) fn custom_emoji_cell(
        &self,
        id: u64,
        item: StickerItem,
        on_screen: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Every visible custom emoji animates (Telegram Desktop); the
        // small emoji playback cache keeps that affordable. Rows the list
        // built beyond the viewport (overdraw) hold still and do not ask
        // the frame clock for redraws.
        let file_id = item.file_id;
        // The reaction selector is this panel in reaction mode, with its own
        // switch (tdesktop `EmojiListWidget::powerSavingFlag`).
        let flag = if self.pickers.media_panel.reaction.is_some() {
            quill::power_saving::Flag::EmojiReactions
        } else {
            quill::power_saving::Flag::EmojiPanel
        };
        let animated = if quill::power_saving::on(flag) {
            // Battery and animations: the emoji panel shows stills.
            None
        } else if on_screen {
            self.custom_emoji_image(file_id, item.format, cx)
        } else {
            self.custom_emoji_image_parked(file_id, item.format, cx)
        };
        let still = self.panel_still(&item);
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        let fallback: SharedString = item.emoji.clone().into();
        let status_id = self.pickers.media_panel.status.and(item.custom_emoji_id);
        let cell = div()
            .id(("panel-custom-emoji", id))
            .size(px(EMOJI_CELL))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent))
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::SR_MESSAGE_CUSTOM_EMOJI,
                &[("emoji", &item.emoji.to_string())],
            ))
            .when(!premium, |this| this.opacity(0.55))
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                let next = hovering.then_some(file_id);
                if this.pickers.media_panel.hovered != next
                    && (*hovering || this.pickers.media_panel.hovered == Some(file_id))
                {
                    this.pickers.media_panel.hovered = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.insert_panel_custom_emoji(&item, window, cx)
            }))
            .child(
                match animated
                    .map(ImageSource::from)
                    .or_else(|| still.map(ImageSource::from))
                {
                    Some(source) => img(source)
                        .id(("panel-custom-emoji-img", id))
                        .size(px(CUSTOM_EMOJI_SIZE))
                        .aspect_square()
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => div().text_size(px(22.)).child(fallback).into_any_element(),
                },
            );
        match status_id {
            Some(status_id) => self.with_status_duration_menu(cell, status_id, cx),
            None => cell.into_any_element(),
        }
    }

    pub(super) fn panel_sticker_cell(
        &mut self,
        id: u64,
        item: StickerItem,
        recent_section: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hovered = self.pickers.media_panel.hovered == Some(item.file_id);
        // Only the hovered sticker animates; the rest stay still.
        let animated = hovered
            .then(|| self.sticker_image(item.file_id, item.format, cx))
            .flatten();
        let still = self.panel_still(&item);
        let source = animated
            .map(ImageSource::Render)
            .or_else(|| still.map(ImageSource::from));
        // What flies into the chat when this sticker is sent.
        let fly_image = source.clone();
        let file_id = item.file_id;
        // A Premium sticker carries the lock (or, with Premium, the star).
        let premium_mark = self
            .session()
            .filter(|s| item.requires_premium && s.premium_badges_shown())
            .map(|s| crate::ui::premium_mark::premium_mark(s.my_is_premium()));
        let favorite = self.session().is_some_and(|s| {
            s.stickers
                .stickers
                .favorites
                .iter()
                .any(|f| f.file_id == file_id)
        });
        let in_recent = recent_section
            && self.session().is_some_and(|s| {
                s.stickers
                    .stickers
                    .recent
                    .iter()
                    .any(|r| r.file_id == file_id)
            });
        let owner = cx.entity().downgrade();
        let (emoji, width, height) = (item.emoji.clone(), item.width, item.height);
        let thumb = item
            .thumb_file_id
            .filter(|id| id.0 != 0)
            .map(|id| (id, item.thumb_width, item.thumb_height));
        div()
            .id(("panel-sticker", id))
            .relative()
            .size(px(STICKER_CELL))
            .p_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent))
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::Q_MEDIA_SEND_STICKER,
                &[("emoji", &item.emoji.to_string())],
            ))
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                let next = hovering.then_some(file_id);
                if this.pickers.media_panel.hovered != next
                    && (*hovering || this.pickers.media_panel.hovered == Some(file_id))
                {
                    this.pickers.media_panel.hovered = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                crate::ui::send_fly::note_source(file_id, fly_image.clone(), (width, height));
                this.send_sticker_pick(file_id, emoji.clone(), width, height, thumb, cx);
                crate::ui::send_fly::note_source(file_id, None, (width, height));
                this.close_media_panel(cx);
            }))
            .context_menu(move |menu, _, _| {
                let favorite_owner = owner.clone();
                let recent_owner = owner.clone();
                let menu = menu.item(
                    PopupMenuItem::new(if favorite {
                        tr(keys::Q_MEDIA_REMOVE_FROM_FAVORITES)
                    } else {
                        tr(keys::Q_MEDIA_ADD_TO_FAVORITES)
                    })
                    .on_click(move |_, _, cx| {
                        let _ = favorite_owner.update(cx, |this, cx| {
                            let sent = this.live.as_mut().is_some_and(|live| {
                                live.driver.set_favorite_sticker(file_id, !favorite).is_ok()
                            });
                            if sent && !favorite {
                                this.note_faved_replaced(file_id, cx);
                            }
                            cx.notify();
                        });
                    }),
                );
                if in_recent {
                    menu.item(PopupMenuItem::new(tr(keys::EMOJI_REMOVE_RECENT)).on_click(
                        move |_, _, cx| {
                            let _ = recent_owner.update(cx, |this, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.remove_recent_sticker(file_id);
                                } else if let Some(session) = this.demo_session.as_mut() {
                                    session
                                        .stickers
                                        .stickers
                                        .recent
                                        .retain(|s| s.file_id != file_id);
                                }
                                cx.notify();
                            });
                        },
                    ))
                } else {
                    menu
                }
            })
            .child(match source {
                // GPUI advances an animated image's frames only for an
                // element with an id (its frame state lives there).
                Some(source) => img(source)
                    .id(("panel-sticker-img", id))
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div()
                    .size_full()
                    .rounded_md()
                    .bg(cx.theme().muted.opacity(0.5))
                    .into_any_element(),
            })
            .children(premium_mark)
            .child(crate::ui::send_fly::cell_probe(file_id))
            .into_any_element()
    }

    /// "Reset recent emoji": clear the local recently-used lists.
    pub(in crate::ui) fn reset_recent_emoji(&mut self, cx: &mut Context<Self>) {
        self.set_media_pref(
            |prefs| {
                prefs.recent_emoji.clear();
                prefs.recent_custom_emoji_ids.clear();
            },
            cx,
        );
        self.pickers.media_panel.key = None;
        cx.notify();
    }

    pub(super) fn insert_panel_emoji(
        &mut self,
        emoji: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.react_from_panel(quill::state::ReactionChoice::Emoji(emoji.to_string()), cx) {
            return;
        }
        if self.route_story_reply_emoji(emoji.as_ref(), window, cx) {
            return;
        }
        if self.route_poll_option_emoji(emoji.as_ref(), window, cx)
            || self.route_group_title_emoji(emoji.as_ref(), window, cx)
        {
            return;
        }
        self.composer.update(cx, |input, cx| {
            input.replace(emoji.as_ref(), window, cx);
            input.focus(window, cx);
        });
        let emoji = emoji.to_string();
        self.set_media_pref(
            move |prefs| {
                prefs.recent_emoji.retain(|e| e != &emoji);
                prefs.recent_emoji.insert(0, emoji);
                prefs
                    .recent_emoji
                    .truncate(quill::emoji_catalog::RECENT_LIMIT);
            },
            cx,
        );
    }

    pub(super) fn insert_panel_custom_emoji(
        &mut self,
        item: &StickerItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pickers.media_panel.status.is_some() {
            if let Some(id) = item.custom_emoji_id {
                self.choose_panel_status(Some(id), 0, cx);
            }
            return;
        }
        if let Some(id) = item.custom_emoji_id
            && self.react_from_panel(quill::state::ReactionChoice::CustomEmoji(id), cx)
        {
            return;
        }
        // A chat title takes standard emoji only (tdesktop `PeerTitle`).
        if self.group_title_emoji_active() {
            return;
        }
        if !self.session().is_some_and(|s| s.my_is_premium()) {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MEDIA_CUSTOM_EMOJI_NEED_TELEGRAM_PREMIUM,
            )));
            cx.notify();
            return;
        }
        let Some(id) = item.custom_emoji_id else {
            return;
        };
        // Drawn inline in the field, one caret stop (codex:composer-input).
        let fallback = item.emoji.clone();
        if self.route_story_reply_custom_emoji(&fallback, id, window, cx) {
            return;
        }
        if self.route_poll_option_custom_emoji(&fallback, id, window, cx) {
            return;
        }
        self.insert_composer_custom_emoji(&fallback, id, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.set_media_pref(
            move |prefs| {
                prefs.recent_custom_emoji_ids.retain(|e| *e != id);
                prefs.recent_custom_emoji_ids.insert(0, id);
                prefs.recent_custom_emoji_ids.truncate(128);
            },
            cx,
        );
    }

    pub(super) fn jump_to_panel_section(&mut self, section: usize, cx: &mut Context<Self>) {
        if let Some(first) = self
            .pickers
            .media_panel
            .sections
            .get(section)
            .map(|s| s.first_row)
        {
            self.pickers.media_panel.list.scroll_to(ListOffset {
                item_ix: first,
                offset_in_item: px(0.),
            });
            self.pickers.media_panel.active_section = section;
            cx.notify();
        }
    }

    /// The popover itself (absolutely positioned by the caller above the
    /// composer).
    pub(in crate::ui) fn media_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.sync_media_panel_rows(cx);
        let tab = self.pickers.media_panel.tab;
        // A live story comment takes emoji only (tdesktop's video stream
        // field has no sticker or GIF tabs).
        let emoji_only = (self.live_story_shown().is_some() && self.story_reply_panel_active())
            || self.group_title_emoji_active();
        let tabs = div().flex().items_center().gap_1().children(
            [
                (PanelTab::Emoji, tr(keys::SWITCH_EMOJI)),
                (PanelTab::Stickers, tr(keys::Q_MEDIA_STICKERS_2)),
                (PanelTab::Gifs, "GIFs"),
            ]
            .into_iter()
            .filter(|(value, _)| !emoji_only || *value == PanelTab::Emoji)
            .map(|(value, label)| {
                Button::new(SharedString::from(format!("panel-tab-{label}")))
                    .label(label)
                    .ghost()
                    .small()
                    .selected(tab == value)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_media_panel_tab(value, cx);
                    }))
            }),
        );
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .pt_2()
            .child(if self.pickers.media_panel.status.is_some() {
                div()
                    .px_1()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::Q_EMOJI_STATUS_SELECTOR))
                    .into_any_element()
            } else if self.pickers.media_panel.reaction.is_some() {
                div()
                    .px_1()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::Q_MEDIA_REACTIONS))
                    .into_any_element()
            } else {
                tabs.into_any_element()
            })
            .child(
                Button::new("media-panel-close")
                    .icon(gpui_kit::assets::IconName::X)
                    .ghost()
                    .small()
                    .tooltip(tr(keys::CLOSE))
                    .accessibility_label(tr(keys::CLOSE))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_media_panel(cx);
                    })),
            );
        let body: AnyElement = if tab == PanelTab::Gifs {
            self.gif_picker_panel(cx)
        } else {
            let weak = cx.weak_entity();
            let sections = self.pickers.media_panel.sections.clone();
            let active = self.pickers.media_panel.active_section;
            let search = div().px_2().pt_2().child(
                Textarea::new(if self.pickers.media_panel.status.is_some() {
                    &self.pickers.status_search_input
                } else if self.pickers.media_panel.reaction.is_some() {
                    &self.pickers.reaction_search_input
                } else if tab == PanelTab::Emoji {
                    &self.pickers.emoji_search_input
                } else {
                    &self.pickers.sticker_search_input
                })
                .aria_label(if self.pickers.media_panel.status.is_some() {
                    tr(keys::Q_EMOJI_STATUS_SEARCH)
                } else if tab == PanelTab::Emoji {
                    tr(keys::Q_MEDIA_SEARCH_EMOJI)
                } else {
                    tr(keys::Q_MEDIA_SEARCH_STICKERS)
                })
                .h(px(34.)),
            );
            // Footer pack icons are each set's first sticker: load the sets
            // and icon files they need, after the visible rows (two defers
            // put these behind the rows' requests; loads are capped).
            let mut icon_sets = Vec::new();
            let mut icon_files = Vec::new();
            for section in &sections {
                if let SectionIcon::Set(set_id) = section.icon {
                    match self.panel_item(StickerSource::Set(set_id), 0) {
                        None => icon_sets.push(set_id),
                        Some(item) => icon_files.extend(item.display_file_id()),
                    }
                }
            }
            if !icon_sets.is_empty() || !icon_files.is_empty() {
                let weak = cx.weak_entity();
                cx.defer(move |cx| {
                    cx.defer(move |cx| {
                        let _ = weak.update(cx, |this, _| {
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.ensure_library_sets(&icon_sets);
                                let _ = live.driver.ensure_media_files(&icon_files);
                            }
                        });
                    });
                });
            }
            let strip = (tab == PanelTab::Stickers
                && self.pickers.media_panel.reaction.is_none()
                && self.panel_query(cx).is_empty())
            .then(|| self.sticker_category_strip(cx))
            .flatten();
            let empty = self.pickers.media_panel.rows.is_empty();
            // Rows cannot ask the list where they are while it lays them
            // out, so take that from its last layout now.
            self.pickers.media_panel.on_screen = super::on_screen_rows(
                &self.pickers.media_panel.list,
                self.pickers.media_panel.rows.len(),
            );
            let list = list(
                self.pickers.media_panel.list.clone(),
                move |ix, _window, cx| {
                    weak.update(cx, |this, cx| this.render_panel_row(ix, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                },
            )
            .flex_1()
            .min_h_0();
            let footer = div()
                .id("media-panel-sections")
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().border)
                .overflow_x_scroll()
                .children(sections.into_iter().enumerate().map(|(index, section)| {
                    let icon: AnyElement = match section.icon {
                        SectionIcon::Emoji(glyph) => {
                            div().text_size(px(18.)).child(glyph).into_any_element()
                        }
                        SectionIcon::Glyph(icon) => {
                            Icon::new(icon).size(px(18.)).into_any_element()
                        }
                        SectionIcon::Set(set_id) => self
                            .panel_item(StickerSource::Set(set_id), 0)
                            .and_then(|item| self.panel_still(item))
                            .map(|path| {
                                img(path)
                                    .size(px(24.))
                                    .aspect_square()
                                    .object_fit(ObjectFit::Contain)
                                    .into_any_element()
                            })
                            .unwrap_or_else(|| {
                                div()
                                    .size(px(22.))
                                    .rounded_md()
                                    .bg(cx.theme().muted)
                                    .into_any_element()
                            }),
                    };
                    section_chip(
                        ("panel-section", index as u64),
                        icon,
                        section.label,
                        true,
                        index == active,
                        cx.listener(move |this, _, _, cx| {
                            this.jump_to_panel_section(index, cx);
                        }),
                        cx,
                    )
                }));
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(search)
                .children(strip)
                .child(if empty {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if tab == PanelTab::Stickers {
                            tr(keys::Q_MEDIA_NO_STICKERS_HERE_YET)
                        } else {
                            tr(keys::Q_MEDIA_NO_EMOJI_FOUND)
                        })
                        .into_any_element()
                } else {
                    list.into_any_element()
                })
                .child(footer)
                .into_any_element()
        };
        // The panel occludes what is under it, the chat's region tag too,
        // so it carries the tag itself: the UI driver treats it as part of
        // the chat it opens over (the surface check already allowed that
        // chat; sticker and GIF sends are refused while an agent drives).
        div()
            .id("media-panel")
            .occlude()
            .a11y_tag(quill::agent::ui_driver::redact::REGION_CHAT)
            .w(px(PANEL_WIDTH))
            .h(px(PANEL_HEIGHT))
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_lg()
            .overflow_hidden()
            .child(header)
            .child(body)
            .into_any_element()
    }
}

/// A button of a tab's footer strip (the emoji, sticker and GIF sections):
/// its icon, highlighted when `selected`, named `label` (shown as a tooltip
/// when `tooltip`; an emoji is its own name).
pub(in crate::ui) fn section_chip(
    id: impl Into<ElementId>,
    icon: AnyElement,
    label: SharedString,
    tooltip: bool,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |this| this.bg(cx.theme().accent))
        .hover(|style| style.bg(cx.theme().accent))
        .role(gpui_kit::Role::Button)
        .aria_label(label.clone())
        .aria_selected(selected)
        .tab_index(0)
        .when(tooltip, |this| {
            this.tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)
            })
        })
        .on_click(on_click)
        .child(icon)
}

//! GIF collection and search picker.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::pressable::PressableDiv;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::state::RequestPurpose;

impl QuillApp {
    fn search_gif_picker(&mut self, cx: &mut Context<Self>) {
        let query = self.pickers.gif_search_input.read(cx).value().to_string();
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.search_gifs(&query) {
                Ok(_) => tr(keys::Q_MISC_SEARCHING_GIFS).into(),
                Err(_) => tr(keys::Q_MISC_COULD_NOT_SEARCH_GIFS).into(),
            };
        }
        cx.notify();
    }
    fn show_saved_gifs(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.show_saved_gifs();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.gifs.search_mode = false;
        }
        cx.notify();
    }
    fn more_gif_search_results(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.more_gif_search_results() {
                Ok(_) => tr(keys::Q_MISC_LOADING_MORE_GIFS).into(),
                Err(_) => tr(keys::Q_MISC_COULD_NOT_LOAD_MORE_GIFS).into(),
            };
        }
        cx.notify();
    }
    fn save_gif_pick(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.set_gif_saved(file_id, true) {
                Ok(_) => tr(keys::Q_MISC_SAVING_GIF).into(),
                Err(_) => tr(keys::Q_MISC_COULD_NOT_SAVE_GIF).into(),
            };
        }
        cx.notify();
    }
    pub(super) fn gif_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self
            .session()
            .map(|session| session.stickers.gifs.clone())
            .unwrap_or_default();
        let files = self
            .session()
            .map(|session| session.media.files.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let (cx_muted, cx_muted_fg) = (cx.theme().muted, cx.theme().muted_foreground);
        let mut grid = div().id("gif-grid").flex().flex_wrap().gap_2();
        for (index, animation) in panel.visible_animations().iter().enumerate() {
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
                    .aspect_ratio(px(96.) / px(72.))
                    .rounded_md()
                    .object_fit(ObjectFit::Cover)
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_MISC_SEND_GIF,
                        &[("index_1", &(index + 1).to_string())],
                    ))
                    .tab_index(0)
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
                                .bg(cx_muted)
                                .text_color(cx_muted_fg)
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
                    .bg(cx.theme().muted)
                    .text_color(cx.theme().muted_foreground)
                    .text_xs()
                    .flex()
                    .items_center()
                    .justify_center()
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_MISC_SEND_GIF,
                        &[("index_1", &(index + 1).to_string())],
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_gif_pick(file_id, duration, width, height, cx);
                    }))
                    .child(label)
                    .into_any_element()
            };
            let saved = panel.animations.iter().any(|item| item.file_id == file_id);
            let pending = self.session().is_some_and(|s| {
                s.requests.has_purpose(RequestPurpose::AddSavedAnimation)
                    || s.requests.has_purpose(RequestPurpose::RemoveSavedAnimation)
            });
            // Save / unsave lives in the tile's context menu (like sticker
            // favorites) instead of a text button under every tile.
            let owner = cx.entity().downgrade();
            grid = grid.child(
                div()
                    .id(SharedString::from(format!(
                        "gif-cell-{index}-{}",
                        file_id.0
                    )))
                    .child(cell)
                    .context_menu(move |menu, _, _| {
                        let owner = owner.clone();
                        menu.item(
                            PopupMenuItem::new(if saved {
                                tr(keys::Q_MISC_REMOVE_FROM_SAVED_GIFS)
                            } else {
                                tr(keys::Q_MISC_SAVE_GIF)
                            })
                            .disabled(pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if saved {
                                        this.open_group_confirm(
                                            ChatId(0),
                                            GroupConfirmAction::RemoveSavedGif { file_id },
                                            cx,
                                        );
                                    } else {
                                        this.save_gif_pick(file_id, cx);
                                    }
                                });
                            }),
                        )
                    }),
            );
        }
        if panel.search_mode && !panel.search_next_offset.is_empty() {
            grid = grid.child(
                Button::new("gif-search-more")
                    .label(tr(keys::Q_MISC_MORE_GIFS))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_gif_search_results(cx))),
            );
        }
        let mut emojis = div().flex().flex_wrap().gap_1();
        for (index, emoji) in panel.provider_emojis.iter().enumerate() {
            let query = emoji.clone();
            emojis = emojis.child(
                Button::new(format!("gif-provider-emoji-{index}"))
                    .label(emoji.clone())
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pickers
                            .gif_search_input
                            .update(cx, |input, cx| input.set_value(query.clone(), window, cx));
                        this.search_gif_picker(cx);
                    })),
            );
        }
        let status = if panel.loading || panel.search_loading {
            Some(tr(keys::Q_MISC_LOADING_GIFS))
        } else if panel.search_failed && panel.search_mode && panel.search_bot_username.is_empty() {
            Some(tr(keys::Q_MISC_GIF_SEARCH_IS_NOT_AVAILABLE))
        } else if panel.failed || panel.search_mode && panel.search_failed {
            Some(tr(keys::Q_MISC_COULD_NOT_UPDATE_GIFS_TRY))
        } else if panel.visible_animations().is_empty() && panel.search_mode {
            Some(tr(keys::Q_MISC_NO_GIFS_FOUND))
        } else if panel.animations.is_empty() && !panel.search_mode {
            Some(tr(keys::Q_MISC_NO_SAVED_GIFS_YET_RIGHT))
        } else {
            None
        };
        let searching = panel.search_mode;
        div()
            .id("gif-picker")
            .when(!self.pickers.media_panel.open, |this| {
                this.max_h(px(420.)).overflow_y_scroll()
            })
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .when(!self.pickers.media_panel.open, |this| {
                this.border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().sidebar)
            })
            // Embedded in the media panel: its tab and close button lead.
            .when(!self.pickers.media_panel.open, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().font_semibold().child("GIFs"))
                        .child(
                            Button::new("close-gif-picker")
                                .icon(gpui_kit::assets::IconName::X)
                                .tooltip(tr(keys::CLOSE))
                                .accessibility_label(tr(keys::CLOSE))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_gif_panel(cx);
                                })),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Textarea::new(&self.pickers.gif_search_input)
                            .aria_label(tr(keys::GIFS_SEARCH))
                            .flex_1()
                            .min_w_0(),
                    )
                    .child(
                        Button::new("gif-search-submit")
                            .icon(gpui_kit::assets::IconName::Search)
                            .ghost()
                            .tooltip(tr(keys::GIFS_SEARCH))
                            .accessibility_label(tr(keys::GIFS_SEARCH))
                            .on_click(cx.listener(|this, _, _, cx| this.search_gif_picker(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("gif-saved-tab")
                            .label(tr(keys::Q_MISC_SAVED))
                            .ghost()
                            .small()
                            .selected(!searching)
                            .on_click(cx.listener(|this, _, _, cx| this.show_saved_gifs(cx))),
                    )
                    .child(
                        Button::new("gif-trending-tab")
                            .label(tr(keys::Q_MISC_TRENDING))
                            .ghost()
                            .small()
                            .selected(searching)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.pickers
                                    .gif_search_input
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                this.search_gif_picker(cx);
                            })),
                    ),
            )
            .when(!panel.search_provider.is_empty(), |body| {
                body.child(div().text_xs().child(tr_with(
                    keys::Q_MISC_GIF_SEARCH,
                    &[("search_provider", &panel.search_provider.to_string())],
                )))
            })
            .when_some(status, |body, status| {
                body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
            })
            .child(emojis)
            .child(grid)
    }
}

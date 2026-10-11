//! The inline bot results panel above the composer: the justified media
//! grid, list rows, the bot's button on top and scroll-to-load paging
//! (tdesktop `inline_bots/inline_results_inner.cpp`). The packing maths is
//! in `quill::inline_grid`; this file draws it and wires the clicks.

use super::QuillApp;
use super::inline_mode::{InlineView, inline_view};
use super::pressable::PressableDiv;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::inline_query_trigger;
use quill::inline_grid::{
    BUTTON_HEIGHT, ButtonAction, CellKind, MAX_PANEL_HEIGHT, MEDIA_HEIGHT, PANEL_PADDING,
    PANEL_WIDTH, Placed, SKIP, STICKER_SIZE, button_action, content_height, grid_top,
    layout_results, scroll_to_reveal, should_load_more,
};
use quill::local_path::sandboxed_display_path;
use quill::state::{DeepLinkAction, InlineBotResolve, InlineQueryFetch};
use quill::telegram::envelope::{InlineQueryResultSummary, InlineQueryResultsButton};
use std::path::PathBuf;

/// Row title: the result title, falling back to a capitalized kind label
/// (`sticker` results carry no title).
fn list_title(result: &InlineQueryResultSummary) -> String {
    if !result.title.is_empty() {
        return result.title.clone();
    }
    let mut kind = result.kind.replace('_', " ");
    if let Some(first) = kind.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    if kind.is_empty() {
        "Result".to_string()
    } else {
        kind
    }
}

/// First letter of a title, for a list row without a picture.
fn initial(title: &str) -> String {
    title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default()
}

impl QuillApp {
    /// Path of a downloaded result image the display roots allow.
    fn inline_image_path(&self, file_id: quill::ids::FileId, roots: &[PathBuf]) -> Option<PathBuf> {
        if file_id.0 == 0 {
            return None;
        }
        self.session()?
            .media
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    }

    /// The panel for the current inline trigger. Clicks act on mouse-down
    /// with `prevent_default`: the composer's blur closes the panel before
    /// a click could land.
    pub(super) fn inline_results_panel(
        &self,
        view: &InlineView,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut panel = div()
            .id("inline-results")
            .flex()
            .flex_col()
            .mx_4()
            .mb_2()
            .w(px(PANEL_WIDTH as f32))
            .max_w_full()
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            // Padding and gaps keep the composer focused.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                }),
            );
        let (results, button) = match view {
            InlineView::Status(text) => {
                return Some(
                    panel
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(text.clone()),
                        )
                        .into_any_element(),
                );
            }
            InlineView::Results { results, button } => (results, button),
        };
        let mosaic = layout_results(results);
        let has_button = button.is_some();
        let content = content_height(&mosaic, has_button);
        let viewport = content.min(MAX_PANEL_HEIGHT);
        let order = mosaic.order();
        let selected = (!order.is_empty()).then(|| {
            order[self
                .composer_ui
                .inline_results_selected
                .min(order.len() - 1)]
        });
        let roots = self.media_display_roots();
        let mut body = div()
            .flex()
            .flex_col()
            .pt(px(PANEL_PADDING as f32))
            .pb(px(PANEL_PADDING as f32));
        if let Some(button) = button {
            body = body.child(self.inline_button_row(button, cx));
        }
        for row in &mosaic.rows {
            let mut line = div()
                .flex()
                .items_start()
                .gap(px(SKIP as f32))
                .px(px(PANEL_PADDING as f32))
                .h(px(row.height as f32))
                .flex_none();
            if has_button && row.y == 0 {
                line = line.mt(px(SKIP as f32));
            }
            for cell in &row.cells {
                let result = &results[cell.item.result];
                let is_selected = selected == Some(cell.item.result);
                line = line.child(self.inline_cell(result, cell, is_selected, &roots, cx));
            }
            body = body.child(line);
        }
        panel = panel.child(
            div()
                .id("inline-results-scroll")
                .max_h(px(viewport as f32))
                .overflow_y_scroll()
                .track_scroll(&self.composer_ui.inline_scroll)
                .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                    this.load_more_inline_results_if_needed(cx);
                }))
                .child(body),
        );
        Some(panel.into_any_element())
    }

    /// The bot's button above the results (tdesktop `switchPmButton`).
    fn inline_button_row(
        &self,
        button: &InlineQueryResultsButton,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = button.clone();
        let label = button.text.clone();
        div()
            .px(px(PANEL_PADDING as f32))
            .child(
                div()
                    .id("inline-results-button")
                    .h(px(BUTTON_HEIGHT as f32))
                    .w_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(cx.theme().primary)
                    .text_color(cx.theme().primary_foreground)
                    .text_sm()
                    .font_semibold()
                    .truncate()
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .tab_index(0)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            this.press_inline_results_button(&button, cx);
                        }),
                    )
                    .child(label),
            )
            .into_any_element()
    }

    /// One result: a grid cell or a list row.
    fn inline_cell(
        &self,
        result: &InlineQueryResultSummary,
        cell: &Placed,
        selected: bool,
        roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let index = cell.item.result;
        let query_id = result.query_id;
        let result_id = result.id.clone();
        let label = list_title(result);
        let path = result
            .media
            .and_then(|media| self.inline_image_path(media.file_id, roots));
        let muted = cx.theme().muted;
        let muted_fg = cx.theme().muted_foreground;
        let picked = cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
            this.send_inline_pick(query_id, result_id.clone(), window, cx);
        });
        if cell.item.kind.is_grid() {
            let height = if cell.item.kind == CellKind::Sticker {
                STICKER_SIZE
            } else {
                MEDIA_HEIGHT
            };
            let fit = if cell.item.kind == CellKind::Sticker {
                ObjectFit::Contain
            } else {
                ObjectFit::Cover
            };
            let (w, h) = (px(cell.width as f32), px(height as f32));
            let content = match path {
                Some(path) => img(path)
                    .w(w)
                    .h(h)
                    .object_fit(fit)
                    .with_fallback(move || div().size_full().bg(muted).into_any_element())
                    .into_any_element(),
                None => div().size_full().bg(muted).into_any_element(),
            };
            return div()
                .id(("inline-cell", index as u64))
                .w(w)
                .h(h)
                .flex_none()
                .overflow_hidden()
                .rounded(px(4.))
                .role(Role::Button)
                .aria_label(label)
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .when(selected, |this| {
                    this.border_2().border_color(cx.theme().primary)
                })
                .on_mouse_down(MouseButton::Left, picked)
                .child(content)
                .into_any_element();
        }
        let side = px(cell.item.kind.list_thumb() as f32);
        let thumb = match path {
            Some(path) => img(path)
                .w(side)
                .h(side)
                .object_fit(ObjectFit::Cover)
                .with_fallback(move || div().size_full().bg(muted).into_any_element())
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(muted)
                .text_color(muted_fg)
                .child(initial(&label))
                .into_any_element(),
        };
        let mut text = div().flex().flex_col().flex_1().min_w_0().child(
            div()
                .text_sm()
                .font_semibold()
                .truncate()
                .child(label.clone()),
        );
        if !result.description.is_empty() {
            text = text.child(
                div()
                    .text_xs()
                    .text_color(muted_fg)
                    .line_clamp(2)
                    .child(result.description.clone()),
            );
        }
        div()
            .id(("inline-cell", index as u64))
            .w(px(cell.width as f32))
            .h(px(cell.item.height as f32))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(6.))
            .overflow_hidden()
            .rounded_md()
            .border_b_1()
            .border_color(cx.theme().border)
            .role(Role::Button)
            .aria_label(label)
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .when(selected, |this| this.bg(cx.theme().selection))
            .on_mouse_down(MouseButton::Left, picked)
            .child(
                div()
                    .w(side)
                    .h(side)
                    .flex_none()
                    .overflow_hidden()
                    .rounded(px(4.))
                    .child(thumb),
            )
            .child(text)
            .into_any_element()
    }

    /// tdesktop `Inner::switchPm`: a start button opens the bot's chat
    /// with `/start <parameter>` waiting in the composer (the same as a
    /// `t.me/bot?start=` link), a web app button opens the bot's mini app.
    pub(super) fn press_inline_results_button(
        &mut self,
        button: &InlineQueryResultsButton,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = button_action(button) else {
            return;
        };
        let Some((bot_id, username)) =
            self.session()
                .and_then(|session| match session.bots.inline_bot_resolve.as_ref() {
                    Some(InlineBotResolve::Resolved {
                        user_id, username, ..
                    }) => Some((*user_id, username.clone())),
                    _ => None,
                })
        else {
            return;
        };
        self.composer_ui.inline_results_open = false;
        self.composer_ui.inline_query_armed = None;
        match action {
            ButtonAction::StartBot { parameter } => {
                if let Some(live) = self.live.as_mut() {
                    let sent = live.driver.resolve_deep_link(DeepLinkAction::OpenUsername {
                        domain: username,
                        start_param: Some(parameter).filter(|p| !p.is_empty()),
                        post: None,
                        story_id: None,
                    });
                    if sent.is_err() {
                        self.set_status_note("Couldn't open the bot's chat.", cx);
                    }
                }
            }
            ButtonAction::WebApp { url } => self.open_inline_results_web_app(bot_id, &url, cx),
        }
        cx.notify();
    }

    /// Where the open panel's grid starts and what it scrolls, for the
    /// paging and keyboard-reveal maths. `None` unless results are shown.
    fn inline_panel_geometry(
        &self,
        cx: &Context<Self>,
    ) -> Option<(quill::inline_grid::Mosaic, bool)> {
        match inline_view(self, cx)? {
            InlineView::Results { results, button } => {
                Some((layout_results(&results), button.is_some()))
            }
            InlineView::Status(_) => None,
        }
    }

    /// Keep the highlighted result in the panel's viewport.
    pub(super) fn scroll_inline_selection_into_view(&mut self, cx: &Context<Self>) {
        let Some((mosaic, has_button)) = self.inline_panel_geometry(cx) else {
            return;
        };
        let order = mosaic.order();
        if order.is_empty() {
            return;
        }
        let target = order[self
            .composer_ui
            .inline_results_selected
            .min(order.len() - 1)];
        let Some(row) = mosaic
            .rows
            .iter()
            .find(|row| row.cells.iter().any(|cell| cell.item.result == target))
        else {
            return;
        };
        let viewport = content_height(&mosaic, has_button).min(MAX_PANEL_HEIGHT);
        let handle = &self.composer_ui.inline_scroll;
        let current = -f32::from(handle.offset().y) as i32;
        let top = grid_top(has_button) + row.y;
        // Let the first row show its padding (and the button above it).
        let top = if row.y == 0 { 0 } else { top };
        let next = scroll_to_reveal(current, viewport, top, row.height);
        if next != current {
            handle.set_offset(gpui_kit::gpui::point(px(0.), px(-(next as f32))));
        }
    }

    /// tdesktop pages `next_offset` once the scroll position gets within
    /// one viewport of the end (`Widget::onScroll`), so a short first page
    /// keeps loading until the panel fills.
    pub(super) fn load_more_inline_results_if_needed(&mut self, cx: &mut Context<Self>) {
        if !self.composer_ui.inline_results_open {
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let Some((_, query)) = inline_query_trigger(&text) else {
            return;
        };
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let Some(slot) = session
            .bots
            .inline_query
            .as_ref()
            .filter(|slot| slot.chat_id == chat_id && slot.query == query)
        else {
            return;
        };
        let InlineQueryFetch::Loaded {
            results,
            button,
            next_offset,
            ..
        } = &slot.fetch
        else {
            return;
        };
        let (bot_user_id, offset) = (slot.bot_user_id, next_offset.clone());
        let loading = session
            .requests
            .has_inline_query_in_flight(chat_id, bot_user_id);
        let mosaic = layout_results(results);
        let has_button = button
            .as_ref()
            .is_some_and(|button| button_action(button).is_some());
        let content = content_height(&mosaic, has_button);
        let viewport = content.min(MAX_PANEL_HEIGHT);
        let scroll_top = -f32::from(self.composer_ui.inline_scroll.offset().y);
        if !should_load_more(
            scroll_top,
            viewport as f32,
            content as f32,
            !offset.is_empty(),
            loading,
        ) {
            return;
        }
        let query = query.to_string();
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .inline_query(bot_user_id, chat_id, &query, &offset);
        }
    }
}

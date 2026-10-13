//! The expanded effect list (tdesktop `Selector::expand` in
//! `EmojiListMode::MessageEffects`): a search field, every reaction effect
//! in rows of 32 px cells, then "Effects from stickers". Premium effects
//! carry a lock for an account without Premium; sticker effects show their
//! emoji in the corner for one with it. A search keeps the effects whose
//! emoji matched; nothing left reads "No effects found."

use super::super::app::QuillApp;
use super::strip::Motion;
use super::{SendMenu, tracker};
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::{TestSupportExt, *};
use quill::message_effects::MessageEffect;
use quill::message_effects_picker::{
    SEARCH_ROW, SECTION_HEADER, STRIP_IMAGE, STRIP_SIZE, STRIP_SKIP, emoji_columns, emoji_key,
    locked, panel_min_height, panel_rect, picker_lists, sticker_grid,
};
use quill::power_saving::Flag;
use std::collections::HashSet;

/// tdesktop `emojiPremiumLock` over a locked emoji effect: small, in the
/// cell's corner.
const LOCK: f32 = 13.;

/// A Premium lock on a cell's bottom-right corner (the cell is relative).
fn lock(size: f32) -> AnyElement {
    div()
        .absolute()
        .right_0()
        .bottom_0()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(linear_gradient(
            135.,
            linear_color_stop(rgb(0x6b93ff), 0.),
            linear_color_stop(rgb(0xc66cd8), 1.),
        ))
        .child(
            Icon::new(gpui_kit::assets::IconName::Lock)
                .with_size(px(size * 0.6))
                .text_color(rgb(0xffffff)),
        )
        .into_any_element()
}

impl QuillApp {
    /// The emoji a search for `query` matched: the catalog's names, and
    /// the keyword answer for other languages.
    fn effect_search_emoji(&self, query: &str) -> HashSet<String> {
        let mut found: HashSet<String> = quill::emoji_catalog::search(None, query)
            .map(|entry| emoji_key(entry.emoji))
            .collect();
        if let Some(session) = self.session() {
            found.extend(
                session
                    .stickers
                    .emoji
                    .keyword_emojis
                    .iter()
                    .map(|emoji| emoji_key(emoji)),
            );
        }
        found
    }

    fn effect_list_cell(
        &self,
        open: &SendMenu,
        effect: &MessageEffect,
        side: f32,
        sticker: bool,
        premium: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = effect.id;
        let hover_bg = cx.theme().accent;
        let (picture_side, flag) = if sticker {
            (side - 8., Flag::StickersPanel)
        } else {
            (STRIP_IMAGE, Flag::EmojiPanel)
        };
        let corner = if !locked(effect, premium) {
            // A sticker effect names its emoji in the corner once it can be
            // used (`_cornerEmoji` with `_paintAsPremium`).
            (sticker && premium).then(|| {
                div()
                    .absolute()
                    .right(px(4.))
                    .bottom(px(4.))
                    .text_size(px(14.))
                    .line_height(px(16.))
                    .child(super::super::reactions::emoji_presentation(&effect.emoji))
                    .into_any_element()
            })
        } else if sticker {
            Some(super::super::premium_mark::premium_mark(false))
        } else {
            Some(lock(LOCK))
        };
        let cells = open.areas.clone();
        div()
            .id(("send-effect-all", id as u64))
            .test_support()
            .relative()
            .size(px(side))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .role(Role::Button)
            .aria_label(format!("Effect {}", effect.emoji))
            .child(self.effect_cell_picture(effect, picture_side, Motion::Playing(Some(flag)), cx))
            .children(corner)
            .child(tracker(move |bounds| {
                cells.cells.borrow_mut().insert(id, bounds);
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.choose_send_effect(id, cx)))
            .into_any_element()
    }

    /// The list over the strip and the menu.
    pub(super) fn effects_panel(
        &self,
        open: &SendMenu,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let Some(column) = open.areas.column.get() else {
            // Placed over the menu once it has been painted.
            self.request_animation_tick(60, cx);
            return None;
        };
        let search = open.search.clone()?;
        let premium = session.my_is_premium();
        let possible = session.premium_possible();
        let all = picker_lists(&session.stickers.effects, possible, None);
        let width = f32::from(column.size.width);
        let min_height = panel_min_height(width, all.reactions.len(), all.stickers.len());
        let (x, y, w, h) = panel_rect(
            (
                f32::from(column.origin.x),
                f32::from(column.origin.y),
                width,
                f32::from(column.size.height),
            ),
            min_height,
            f32::from(open.viewport.height),
        );
        let found = (!open.query.is_empty()).then(|| self.effect_search_emoji(&open.query));
        let lists = picker_lists(&session.stickers.effects, possible, found.as_ref());
        let inner = w - 2. * STRIP_SKIP;
        let muted = cx.theme().muted_foreground;

        let mut list = div()
            .id("send-effects-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(STRIP_SKIP))
            .pb(px(STRIP_SKIP))
            .flex()
            .flex_col();
        let columns = emoji_columns(w);
        for chunk in lists.reactions.chunks(columns) {
            let mut row = div().flex().h(px(STRIP_SIZE));
            for effect in chunk {
                row =
                    row.child(self.effect_list_cell(open, effect, STRIP_SIZE, false, premium, cx));
            }
            list = list.child(row);
        }
        if !lists.stickers.is_empty() {
            let (sticker_columns, cell) = sticker_grid(w);
            list = list.child(
                div()
                    .h(px(SECTION_HEADER))
                    .flex()
                    .items_center()
                    .text_xs()
                    .font_semibold()
                    .text_color(muted)
                    .child("Effects from stickers"),
            );
            for chunk in lists.stickers.chunks(sticker_columns) {
                let mut row = div().flex().w(px(inner));
                for effect in chunk {
                    row = row.child(self.effect_list_cell(open, effect, cell, true, premium, cx));
                }
                list = list.child(row);
            }
        }
        if lists.reactions.is_empty() && lists.stickers.is_empty() {
            list = list.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(muted)
                    .child("No effects found."),
            );
        }
        let areas = open.areas.clone();
        Some(
            div()
                .id("send-effects-panel")
                .test_support()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .occlude()
                .flex()
                .flex_col()
                .popover_style(cx)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.close_send_menu(cx);
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .h(px(SEARCH_ROW))
                        .flex()
                        .items_center()
                        .px(px(STRIP_SKIP))
                        .child(
                            Input::new(&search)
                                .small()
                                .prefix(
                                    Icon::new(gpui_kit::assets::IconName::Search)
                                        .size(px(14.))
                                        .text_color(muted),
                                )
                                .aria_label("Search effects"),
                        ),
                )
                .child(list)
                .child(tracker(move |bounds| areas.panel.set(Some(bounds))))
                .into_any_element(),
        )
    }
}

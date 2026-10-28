//! The strip above the send menu: "Add an animated effect" and the
//! account's reaction effects, the last cell expanding the list (tdesktop
//! `Selector` with its `Strip`, `about` and the expand button).

use super::super::app::QuillApp;
use super::{SendMenu, tracker};
use gpui_kit::component::*;
use gpui_kit::{TestSupportExt, *};
use quill::lang::{keys, tr, tr_with};
use quill::message_effects::MessageEffect;
use quill::message_effects_picker::{
    STRIP_HEIGHT, STRIP_IMAGE, STRIP_SIZE, STRIP_SKIP, cell_sticker, picker_lists, strip_layout,
};
use quill::power_saving::{self, Flag};

/// The menu is this wide until it has been painted once.
const MENU_WIDTH_GUESS: f32 = 200.;

/// How a cell's picture moves.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Motion {
    /// Its first frame.
    Still,
    /// Playing, unless `flag` is on in power saving.
    Playing(Option<Flag>),
}

impl QuillApp {
    /// The picture of an effect in a cell `side` wide: its select
    /// animation (a sticker effect's sticker), the emoji until it decodes.
    pub(super) fn effect_cell_picture(
        &self,
        effect: &MessageEffect,
        side: f32,
        motion: Motion,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let frame = cell_sticker(effect).and_then(|sticker| {
            let play = match motion {
                Motion::Still => false,
                Motion::Playing(flag) => flag.is_none_or(|flag| !power_saving::on(flag)),
            };
            if play {
                self.custom_emoji_image(sticker.file_id, sticker.format, side, cx)
            } else {
                self.custom_emoji_image_parked(sticker.file_id, sticker.format, side, cx)
            }
        });
        match frame {
            Some(frame) => img(frame)
                .size(px(side))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .size(px(side))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(side * 0.8))
                .line_height(px(side))
                .child(super::super::reactions::emoji_presentation(&effect.emoji))
                .into_any_element(),
        }
    }

    /// The strip and the width the menu under it takes (tdesktop
    /// `setForceWidth`), or `None` when no reaction effect is known yet.
    pub(super) fn effects_strip(
        &self,
        open: &SendMenu,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, f32)> {
        let session = self.session()?;
        let lists = picker_lists(&session.stickers.effects, session.premium_possible(), None);
        if lists.reactions.is_empty() {
            return None;
        }
        let menu_width = open
            .areas
            .menu
            .get()
            .map_or(MENU_WIDTH_GUESS, |bounds| f32::from(bounds.size.width));
        let room = f32::from(open.viewport.width) - 16.;
        let layout = strip_layout(lists.reactions.len(), menu_width, room);
        let paused = open.preview.is_some();
        let theme = cx.theme();
        let (hover_bg, muted) = (theme.accent, theme.muted_foreground);
        let mut row = div()
            .flex()
            .items_center()
            .h(px(STRIP_HEIGHT))
            .px(px(STRIP_SKIP));
        for effect in lists.reactions.iter().take(layout.shown) {
            let id = effect.id;
            let motion = if open.hovered == Some(id) && !paused {
                Motion::Playing(None)
            } else {
                Motion::Still
            };
            let cells = open.areas.clone();
            row = row.child(
                div()
                    .id(("send-effect", id as u64))
                    .test_support()
                    .relative()
                    .size(px(STRIP_SIZE))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover_bg))
                    .role(Role::Button)
                    .aria_label(tr_with(
                        keys::Q_MISC_EFFECT,
                        &[("emoji", &effect.emoji.to_string())],
                    ))
                    .child(self.effect_cell_picture(effect, STRIP_IMAGE, motion, cx))
                    .child(tracker(move |bounds| {
                        cells.cells.borrow_mut().insert(id, bounds);
                    }))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if let Some(open) = this.composer_ui.send_menu.open.as_mut() {
                            if *hovered {
                                open.hovered = Some(id);
                            } else if open.hovered == Some(id) {
                                open.hovered = None;
                            }
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_send_effect(id, cx);
                    })),
            );
        }
        if layout.expand {
            row = row.child(
                div()
                    .id("send-effects-expand")
                    .test_support()
                    .size(px(STRIP_SIZE))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .text_color(muted)
                    .hover(move |style| style.bg(hover_bg))
                    .role(Role::Button)
                    .aria_label(tr(keys::Q_MISC_SHOW_ALL_EFFECTS))
                    .child(Icon::new(gpui_kit::assets::IconName::ChevronDown).size(px(16.)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.expand_send_effects(window, cx);
                    })),
            );
        }
        let areas = open.areas.clone();
        Some((
            div()
                .id("send-effects-strip")
                .test_support()
                .relative()
                .occlude()
                .w_full()
                .flex()
                .flex_col()
                .popover_style(cx)
                .child(
                    div()
                        .pt_1p5()
                        .px_2()
                        .text_xs()
                        .text_center()
                        .text_color(muted)
                        .child(tr(keys::EFFECT_ADD_TITLE)),
                )
                .child(row)
                .child(tracker(move |bounds| areas.strip.set(Some(bounds))))
                .into_any_element(),
            layout.width,
        ))
    }
}

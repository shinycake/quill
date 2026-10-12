//! The sticker tab's category strip and Premium stickers (tdesktop
//! `Ui::SearchWithGroups` with `Data::EmojiStatuses::stickerGroupsValue`,
//! `chat_helpers/stickers_list_widget.cpp`).
//!
//! While the search field is empty, a row of category icons sits under it.
//! Choosing one lists its stickers instead of the sets: the stickers of the
//! category's emoji, or, for the Premium category, the Premium stickers.
//! The Premium stickers carry a lock (a star with Premium) wherever they
//! are listed (`premium_mark`), and are left out altogether where Premium
//! cannot be bought (`PrepareStickers(skipPremium)`).

use super::*;
use quill::telegram::EmojiCategorySource;

/// Side of a category chip.
const CHIP: f32 = 30.;
/// Side of a category icon.
const ICON: f32 = 22.;

impl QuillApp {
    /// The chosen category's name for the results header.
    pub(super) fn chosen_category_name(&self) -> Option<SharedString> {
        let index = self.pickers.media_panel.category?;
        let category = self
            .session()?
            .stickers
            .emoji
            .sticker_categories
            .get(index)?;
        Some(match &category.source {
            EmojiCategorySource::Premium if category.name.is_empty() => "Premium".into(),
            _ => category.name.clone().into(),
        })
    }

    /// Whether the sticker at `ix` of `source` is listed: Premium stickers
    /// are hidden where Premium is out of reach for the account.
    pub(super) fn sticker_listed(&self, source: StickerSource, ix: usize) -> bool {
        let Some(session) = self.session() else {
            return true;
        };
        session.premium_badges_shown()
            || !self
                .panel_item(source, ix)
                .is_some_and(|item| item.requires_premium)
    }

    /// Ask TDLib for the stickers of the chosen category.
    pub(super) fn search_chosen_sticker_category(&mut self) {
        let Some(index) = self.pickers.media_panel.category else {
            return;
        };
        let Some(source) = self
            .session()
            .and_then(|s| s.stickers.emoji.sticker_categories.get(index))
            .map(|category| category.source.clone())
        else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.search_sticker_category(&source);
        }
    }

    /// Choose a category, or leave it (`None`).
    pub(super) fn choose_sticker_category(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.pickers.media_panel.category = index;
        self.pickers.media_panel.key = None;
        self.pickers.media_panel.active_section = 0;
        cx.notify();
    }

    /// The strip of category icons under the sticker search field; `None`
    /// until the categories are known. Asks for them (once) and for their
    /// icons.
    pub(super) fn sticker_category_strip(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let asked = self
            .session()
            .is_some_and(|s| s.stickers.emoji.sticker_categories_asked);
        if !asked && self.live.is_some() {
            let weak = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, _| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.fetch_sticker_categories();
                    }
                });
            });
        }
        let session = self.session()?;
        let categories = &session.stickers.emoji.sticker_categories;
        if categories.is_empty() {
            return None;
        }
        let chosen = self.pickers.media_panel.category;
        let mut icon_files = Vec::new();
        let chips: Vec<AnyElement> = categories
            .iter()
            .enumerate()
            .map(|(index, category)| {
                let still = category.icon.as_ref().and_then(|icon| {
                    icon_files.extend(icon.display_file_id());
                    self.panel_still(icon)
                });
                let glyph: AnyElement = match still {
                    Some(path) => img(path)
                        .size(px(ICON))
                        .aspect_square()
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None if category.source == EmojiCategorySource::Premium => {
                        Icon::new(gpui_kit::assets::IconName::StarFill)
                            .size(px(16.))
                            .into_any_element()
                    }
                    None => div()
                        .text_size(px(14.))
                        .child(
                            category
                                .name
                                .chars()
                                .next()
                                .map(String::from)
                                .unwrap_or_default(),
                        )
                        .into_any_element(),
                };
                let name: SharedString = category.name.clone().into();
                div()
                    .id(("sticker-category", index as u64))
                    .size(px(CHIP))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .when(chosen == Some(index), |this| this.bg(cx.theme().accent))
                    .hover(|style| style.bg(cx.theme().accent))
                    .role(gpui_kit::Role::Button)
                    .aria_label(name.clone())
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_sticker_category(Some(index), cx);
                    }))
                    .child(glyph)
                    .into_any_element()
            })
            .collect();
        if !icon_files.is_empty() {
            let weak = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, _| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.ensure_media_files(&icon_files);
                    }
                });
            });
        }
        // A chosen category replaces the search icon with a way back
        // (tdesktop's `_back` button).
        let back = chosen.is_some().then(|| {
            Button::new("sticker-category-back")
                .icon(gpui_kit::assets::IconName::ChevronLeft)
                .ghost()
                .small()
                .tooltip("Back")
                .accessibility_label("Back to all stickers")
                .on_click(cx.listener(|this, _, _, cx| this.choose_sticker_category(None, cx)))
        });
        Some(
            div()
                .id("sticker-categories")
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .pt_1()
                .overflow_x_scroll()
                .children(back)
                .children(chips)
                .into_any_element(),
        )
    }
}

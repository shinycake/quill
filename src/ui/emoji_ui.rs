//! Plain emoji picker: offline catalog, native cursor edits and account recents.
use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::emoji_catalog::{CATEGORIES, catalog, search};

impl QuillApp {
    fn insert_picker_emoji(
        &mut self,
        emoji: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            input.replace(emoji, window, cx);
            input.focus(window, cx);
        });
        self.set_media_pref(|prefs| prefs.remember_emoji(emoji), cx);
    }

    pub(super) fn emoji_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.emoji_search_input.read(cx).value().to_string();
        let recent = self
            .session()
            .map(|s| s.media_prefs.recent_emoji.as_slice())
            .unwrap_or_default();
        let entries: Vec<_> = if !query.trim().is_empty() {
            search(None, &query).collect()
        } else if self.emoji_category == 0 {
            recent
                .iter()
                .take(quill::emoji_catalog::RECENT_LIMIT)
                .filter_map(|emoji| catalog().find(|entry| entry.emoji == emoji))
                .collect()
        } else {
            search(CATEGORIES.get(self.emoji_category - 1).copied(), "").collect()
        };
        let mut tabs = div().flex().flex_wrap().gap_1();
        for (index, title) in std::iter::once("Recent")
            .chain(CATEGORIES.iter().copied())
            .enumerate()
        {
            tabs = tabs.child(
                Button::new(format!("emoji-category-{index}"))
                    .label(title)
                    .selected(query.trim().is_empty() && self.emoji_category == index)
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.emoji_category = index;
                        this.emoji_visible_count = 120;
                        this.emoji_search_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        cx.notify();
                    })),
            );
        }
        let mut grid = div().id("emoji-grid").flex().flex_wrap().gap_1();
        for entry in entries
            .iter()
            .skip(self.emoji_visible_count.saturating_sub(120))
            .take(120)
        {
            let emoji = entry.emoji;
            grid = grid.child(
                Button::new(format!("emoji-pick-{emoji}"))
                    .label(emoji)
                    .accessibility_label(entry.name)
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.insert_picker_emoji(emoji, window, cx)
                    })),
            );
        }
        if self.emoji_visible_count > 120 {
            grid = grid.child(
                Button::new("emoji-previous")
                    .label("Previous emoji")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.emoji_visible_count =
                            this.emoji_visible_count.saturating_sub(120).max(120);
                        cx.notify();
                    })),
            );
        }
        if entries.len() > self.emoji_visible_count {
            grid = grid.child(
                Button::new("emoji-more")
                    .label("Next emoji")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.emoji_visible_count += 120;
                        cx.notify();
                    })),
            );
        }
        let title = if query.trim().is_empty() {
            if self.emoji_category == 0 {
                "Recent"
            } else {
                CATEGORIES[self.emoji_category - 1]
            }
        } else {
            "Search results"
        };
        let note = if entries.is_empty() {
            "No emoji found".to_string()
        } else {
            format!("{} emoji", entries.len())
        };
        div()
            .id("emoji-picker")
            .max_h(px(360.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child(title))
                    .child(
                        Button::new("emoji-clear-recent")
                            .label("Clear recent")
                            .ghost()
                            .disabled(recent.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_media_pref(|p| p.recent_emoji.clear(), cx)
                            })),
                    )
                    .child(Button::new("emoji-close").label("Close").ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.emoji_picker_open = false;
                            cx.notify();
                        }),
                    )),
            )
            .child(Textarea::new(&self.emoji_search_input))
            .child(tabs)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(note),
            )
            .child(grid)
    }
}

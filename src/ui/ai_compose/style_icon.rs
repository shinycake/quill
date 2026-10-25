//! The "Style Icon" view (tdesktop create_ai_tone_box.cpp
//! `ChooseToneIconBox`): the default topic icons first (its custom recent
//! list), then the installed custom emoji sets, and Cancel.

use crate::ui::ai_compose_state::{AiComposeView, EditorWarning};
use crate::ui::app::QuillApp;
use crate::ui::pressable::PressableDiv;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::FileId;
use quill::lang::{keys, tr};
use quill::telegram::envelope::StickerItem;
use std::path::PathBuf;

const DOWNLOAD_CELLS: usize = 240;

const CELL: f32 = 40.;
const GLYPH: f32 = 32.;

impl QuillApp {
    pub(super) fn open_ai_style_icons(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_forum_topic_icons();
            let _ = live.driver.open_media_panel();
        }
        self.set_ai_compose_view(AiComposeView::StyleIcon, cx);
    }

    pub(super) fn ai_emoji_still(&self, id: i64) -> Option<PathBuf> {
        if id <= 0 {
            return None;
        }
        if let Some(path) = self.custom_emoji_still(id) {
            return Some(path);
        }
        let session = self.session()?;
        let item = session
            .threads
            .forum_topic_icons
            .iter()
            .chain(session.media.media_library.set_stickers.values().flatten())
            .find(|item| item.custom_emoji_id == Some(id))?;
        self.panel_still(item)
    }

    fn ai_style_icon_sections(&self) -> Vec<(Option<String>, Vec<StickerItem>)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let icons: Vec<StickerItem> = session
            .threads
            .forum_topic_icons
            .iter()
            .filter(|item| item.custom_emoji_id.is_some())
            .cloned()
            .collect();
        let sets = session
            .stickers
            .emoji
            .installed_sets
            .iter()
            .filter_map(|set| {
                let items = session.media.media_library.set_stickers.get(&set.id)?;
                Some((Some(set.title.clone()), items.clone()))
            });
        std::iter::once((None, icons)).chain(sets).collect()
    }

    pub(super) fn fetch_ai_style_icons(&mut self) {
        let showing = self
            .ai_compose
            .open
            .as_ref()
            .is_some_and(|open| open.view == AiComposeView::StyleIcon);
        if !showing {
            return;
        }
        let set_ids: Vec<i64> = self
            .session()
            .map(|s| {
                s.stickers
                    .emoji
                    .installed_sets
                    .iter()
                    .map(|set| set.id)
                    .collect()
            })
            .unwrap_or_default();
        let files: Vec<FileId> = self
            .ai_style_icon_sections()
            .iter()
            .flat_map(|(_, items)| items.iter())
            .take(DOWNLOAD_CELLS)
            .filter(|item| self.panel_still(item).is_none())
            .filter_map(|item| item.display_file_id())
            .collect();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let _ = live.driver.ensure_library_sets(&set_ids);
        let _ = live.driver.ensure_media_files(&files);
    }

    pub(super) fn ai_style_icons_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let sections = self.ai_style_icon_sections();
        let mut column = v_flex()
            .id("ai-style-icons")
            .gap_2()
            .role(Role::List)
            .aria_label(tr(keys::AI_COMPOSE_TONE_ICON_TITLE));
        for (title, items) in sections {
            if items.is_empty() {
                continue;
            }
            if let Some(title) = title {
                column = column.child(
                    div()
                        .pt_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(title),
                );
            }
            let cells = items.into_iter().filter_map(|item| {
                let id = item.custom_emoji_id?;
                let glyph = match self.panel_still(&item) {
                    Some(path) => img(path)
                        .size(px(GLYPH))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => div()
                        .text_size(px(GLYPH * 0.7))
                        .child(item.emoji.clone())
                        .into_any_element(),
                };
                Some(
                    div()
                        .id(SharedString::from(format!(
                            "ai-style-icon-{}-{id}",
                            item.set_id
                        )))
                        .size(px(CELL))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .pressable(cx.theme())
                        .role(Role::Button)
                        .aria_label(item.emoji.clone())
                        .tab_index(0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pick_ai_style_icon(id, cx);
                        }))
                        .child(glyph),
                )
            });
            column = column.child(h_flex().flex_wrap().gap_1().children(cells));
        }
        column.into_any_element()
    }

    pub(super) fn ai_style_icons_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .justify_end()
            .child(
                Button::new("ai-style-icons-cancel")
                    .ghost()
                    .label(tr(keys::CANCEL))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_ai_compose_view(AiComposeView::StyleEditor, cx);
                    })),
            )
            .into_any_element()
    }

    fn pick_ai_style_icon(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(editor) = self.ai_style_editor_mut() {
            editor.emoji_id = id;
            if editor.warning == Some(EditorWarning::Icon) {
                editor.warning = None;
            }
        }
        self.set_ai_compose_view(AiComposeView::StyleEditor, cx);
    }
}

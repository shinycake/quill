//! Settings > Chats > Stickers and emoji: which stickers are suggested for
//! an emoji (tdesktop `suggestStickersByEmoji`) and the entry to the
//! manager box.
use crate::ui::app::QuillApp;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};
use quill::sticker_suggest::StickerSuggestMode;

const MODES: [(StickerSuggestMode, Key); 3] = [
    (
        StickerSuggestMode::InstalledAndRecommended,
        keys::Q_MEDIA_INSTALLED_AND_RECOMMENDED,
    ),
    (
        StickerSuggestMode::InstalledOnly,
        keys::Q_MEDIA_INSTALLED_ONLY,
    ),
    (StickerSuggestMode::None, keys::SETTINGS_COLOR_EMOJI_OFF),
];

impl QuillApp {
    pub(in crate::ui) fn sticker_suggest_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self
            .session()
            .map(|session| session.settings.media_prefs.sticker_suggest_mode)
            .unwrap_or_default();
        v_flex()
            .id("sticker-suggest-section")
            .gap_1()
            .p_3()
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .child(tr(keys::Q_MEDIA_SUGGEST_STICKERS_BY_EMOJI)),
            )
            .child(
                RadioGroup::vertical("sticker-suggest-modes")
                    .selected_index(MODES.iter().position(|(value, _)| *value == mode))
                    .children(MODES.iter().map(|(value, label)| {
                        Radio::new(format!("sticker-suggest-{value:?}")).label(tr(*label))
                    }))
                    .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                        if let Some((value, _)) = MODES.get(ix) {
                            this.set_sticker_suggest_mode(*value, cx);
                        }
                    })),
            )
            .into_any_element()
    }
}

//! The description block of a group or channel info panel. tdesktop shows
//! it as linkified text (`info_profile_actions.cpp`, the about label) whose
//! context menu offers Copy Text and Translate; the text arrives from TDLib
//! as a plain string, so links come from `quill::text::autolink`.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::message_text::rich_text_line;
use gpui_kit::TestSupportExt;
use quill::text::autolink::autolink_entities;
use std::collections::HashMap;

impl QuillApp {
    /// A labelled, linkified description for `chat_id`'s info panel.
    pub(in crate::ui) fn about_text_block(
        &self,
        chat_id: ChatId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entities = autolink_entities(text);
        let body = rich_text_line(
            text,
            &entities,
            (chat_id.0, 0),
            false,
            &self.message_ui.spoiler_revealed,
            gpui_kit::px(14.0),
            &HashMap::new(),
            cx,
        );
        let owner = cx.entity().downgrade();
        let copy = text.to_string();
        let translate = self.translate_menu_offered(chat_id, text);
        div()
            .id("info-about")
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::INFO_ABOUT_LABEL)),
            )
            .child(div().text_sm().child(body))
            .test_support()
            .context_menu(move |menu, _, _| {
                about_menu(menu, &owner, chat_id, copy.clone(), translate)
            })
            .into_any_element()
    }
}

/// Copy Text, then Translate unless the text is already in a language the
/// user reads (tdesktop `SkipTranslate`).
fn about_menu(
    mut menu: PopupMenu,
    owner: &gpui_kit::WeakEntity<QuillApp>,
    chat_id: ChatId,
    text: String,
    translate: bool,
) -> PopupMenu {
    let copy_owner = owner.clone();
    let copy_text = text.clone();
    menu = menu.item(
        PopupMenuItem::new(tr(keys::CONTEXT_COPY_TEXT)).on_click(move |_, _, cx| {
            let _ = copy_owner.update(cx, |this, cx| {
                this.copy_profile_text(&copy_text, tr(keys::Q_PEOPLE_ABOUT_COPIED), cx);
            });
        }),
    );
    if translate {
        let owner = owner.clone();
        menu = menu.item(PopupMenuItem::new(tr(keys::CONTEXT_TRANSLATE)).on_click(
            move |_, window, cx| {
                let text = text.clone();
                let _ = owner.update(cx, |this, cx| {
                    this.open_translate_selection(chat_id, text, window, cx);
                });
            },
        ));
    }
    menu
}

//! The description block of a group or channel info panel. tdesktop shows
//! it as linkified text (`info_profile_actions.cpp`, the about label) whose
//! context menu offers Copy Text and Translate. TDLib parsed the links when
//! the full info arrived (`quill::text::about`); this only draws them.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::message_text::rich_text_line;
use gpui_kit::TestSupportExt;
use quill::text::TextEntity;
use std::collections::HashMap;

impl QuillApp {
    /// A labelled, linkified description for `chat_id`'s info panel.
    pub(in crate::ui) fn about_text_block(
        &self,
        chat_id: ChatId,
        text: &str,
        entities: &[TextEntity],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = rich_text_line(
            text,
            entities,
            (chat_id.0, 0),
            false,
            &self.message_ui.spoiler_revealed,
            gpui_kit::px(14.0),
            &HashMap::new(),
            cx,
        );
        let owner = cx.entity().downgrade();
        let copy = text.to_string();
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
            .context_menu(move |menu, _, cx| about_menu(menu, &owner, chat_id, copy.clone(), cx))
            .into_any_element()
    }
}

/// The right-click menu: the copy item of the link under the pointer, Copy
/// Text, then Translate unless the text is already in a language the user
/// reads (tdesktop `SkipTranslate`). Built when the menu opens.
fn about_menu(
    mut menu: PopupMenu,
    owner: &gpui_kit::WeakEntity<QuillApp>,
    chat_id: ChatId,
    text: String,
    cx: &mut App,
) -> PopupMenu {
    // The press that opened this menu noted the link under it; take it so
    // it cannot leak into a later menu.
    let (link, translate) = owner
        .update(cx, |this, _| {
            let link = this.message_ui.right_clicked_link.take().map(|(_, l)| l);
            (link, this.translate_menu_offered(chat_id, &text))
        })
        .unwrap_or((None, false));
    if let Some(link) = link
        && let (Some(label), Some(copied)) = (link.copy_label(), link.copy_text())
    {
        let copy_owner = owner.clone();
        let copied = copied.to_string();
        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
            let _ = copy_owner.update(cx, |this, cx| {
                this.copy_profile_text(&copied, tr(keys::STAR_REF_LINK_COPIED_TITLE), cx);
            });
        }));
    }
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

//! The description block of a group or channel info panel. tdesktop shows
//! it as linkified text (`info_profile_actions.cpp`, the about label) whose
//! context menu offers Copy Text and Translate. TDLib parsed the links when
//! the full info arrived (`quill::text::about`); this only draws them. The
//! menu is shared with a user's bio row.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::message_text::rich_text_line;
use gpui_kit::TestSupportExt;
use quill::text::TextEntity;
use quill::translate::about_menu::{AboutCopy, AboutMenu, about_menu};
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
            .context_menu(move |menu, window, cx| {
                about_text_menu(
                    menu,
                    &owner,
                    chat_id,
                    &copy,
                    tr(keys::Q_PEOPLE_ABOUT_COPIED),
                    window,
                    cx,
                )
            })
            .into_any_element()
    }

    /// The items an about text's menu offers right now.
    pub(super) fn about_menu_plan(
        &self,
        chat_id: ChatId,
        text: &str,
        selection: Option<&str>,
    ) -> AboutMenu {
        about_menu(text, selection, |candidate| {
            !self.translate_menu_offered(chat_id, candidate)
        })
    }
}

/// The right-click menu of an about text (`setupAboutContextMenu`): the
/// copy item of the link under the pointer, Copy Selected Text over a
/// selection or Copy Text, then Translate (Selected Text) unless
/// `SkipTranslate` holds. Built when the menu opens.
pub(super) fn about_text_menu(
    mut menu: PopupMenu,
    owner: &gpui_kit::WeakEntity<QuillApp>,
    chat_id: ChatId,
    text: &str,
    copied_toast: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> PopupMenu {
    let selection = crate::ui::selectable_text::selected_message_text(window, cx)
        .filter(|(key, _)| *key == (chat_id.0, 0))
        .map(|(_, selected)| selected);
    // The press that opened this menu noted the link under it; take it so
    // it cannot leak into a later menu.
    let (link, plan) = owner
        .update(cx, |this, _| {
            let link = this.message_ui.right_clicked_link.take().map(|(_, l)| l);
            (
                link,
                this.about_menu_plan(chat_id, text, selection.as_deref()),
            )
        })
        .unwrap_or_else(|_| (None, about_menu(text, selection.as_deref(), |_| true)));
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
    if let Some(copy) = plan.copy {
        let (label, copied) = match copy {
            AboutCopy::Selected(text) => (tr(keys::Q_CHAT_COPY_SELECTED_TEXT), text),
            AboutCopy::Text(text) => (tr(keys::CONTEXT_COPY_TEXT), text),
        };
        let copy_owner = owner.clone();
        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
            let _ = copy_owner.update(cx, |this, cx| {
                this.copy_profile_text(&copied, copied_toast, cx);
            });
        }));
    }
    if let Some(translate) = plan.translate {
        let owner = owner.clone();
        let label = if translate.selected {
            tr(keys::Q_CHAT_TRANSLATE_SELECTED_TEXT)
        } else {
            tr(keys::CONTEXT_TRANSLATE)
        };
        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
            let text = translate.text.clone();
            let _ = owner.update(cx, |this, cx| {
                this.open_translate_selection(chat_id, text, window, cx);
            });
        }));
    }
    menu
}

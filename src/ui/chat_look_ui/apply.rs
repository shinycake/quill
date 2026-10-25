//! The chat look dialog's footer. A new wallpaper for a private chat is
//! applied "for me" or "for me and {user}"; the second needs Premium and
//! opens its preview otherwise (tdesktop `BackgroundPreviewBox::applyForPeer`).
#![deny(clippy::indexing_slicing)]

use super::ChatLookDialog;
use crate::ui::app::QuillApp;
use crate::ui::shell::DialogKind;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::PremiumFeature;

/// What the footer's state needs from the dialog builder.
pub(super) struct FooterState {
    pub(super) chat_id: i64,
    pub(super) changed: bool,
    pub(super) applying: bool,
}

impl QuillApp {
    /// The person a new wallpaper can also be set for: a private chat's
    /// first name, when Premium is sold and the wallpaper is not a boosted
    /// chat's (which is for everyone).
    fn wallpaper_for_both(&self, chat_id: i64, state: &ChatLookDialog) -> Option<String> {
        let session = self.session()?;
        let new_wallpaper = state.background.is_some() || state.file.is_some();
        if !new_wallpaper || self.is_boosted_look(chat_id) || !session.premium_possible() {
            return None;
        }
        let user_id = session.private_chat_user_id(ChatId(chat_id))?;
        match session.user(user_id) {
            Some(user) if !user.first_name.is_empty() => Some(user.first_name.clone()),
            _ => session.chats.get(&chat_id).map(|chat| chat.title.clone()),
        }
    }

    fn apply_chat_look_for(&mut self, both: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialogs.chat_look_dialog.as_mut() {
            dialog.both = both;
        }
        self.apply_chat_look(cx);
        self.close_kit_dialog_if_done(DialogKind::ChatLook, window, cx);
    }

    pub(super) fn chat_look_footer(
        &self,
        footer: FooterState,
        state: &ChatLookDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = !footer.changed || footer.applying;
        let apply_label = |key| {
            if footer.applying {
                tr(keys::Q_MISC_APPLYING)
            } else {
                tr(key)
            }
        };
        let cancel = Button::new("chat-look-cancel")
            .label(tr(keys::CANCEL))
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_chat_look_dialog(cx);
                this.close_kit_dialog_if_done(DialogKind::ChatLook, window, cx);
            }));
        let row = div().flex().flex_wrap().justify_end().gap_2();
        let Some(name) = self.wallpaper_for_both(footer.chat_id, state) else {
            return row
                .child(cancel)
                .child(
                    Button::new("chat-look-apply")
                        .label(apply_label(keys::Q_MISC_APPLY))
                        .primary()
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_chat_look_for(false, window, cx);
                        })),
                )
                .into_any_element();
        };
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        row.child(cancel)
            .child(
                Button::new("chat-look-apply-me")
                    .label(apply_label(keys::BACKGROUND_APPLY_ME))
                    .outline()
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_chat_look_for(false, window, cx);
                    })),
            )
            .child(
                Button::new("chat-look-apply-both")
                    .label(tr_with(keys::BACKGROUND_APPLY_BOTH, &[("user", &name)]))
                    .primary()
                    .when(!premium, |button| button.icon(IconName::Lock))
                    .disabled(premium && busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if premium {
                            this.apply_chat_look_for(true, window, cx);
                        } else {
                            // The chat look outranks the Premium dialog, so
                            // it closes for the preview.
                            this.close_chat_look_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::ChatLook, window, cx);
                            this.open_premium_at(PremiumFeature::BackgroundForBoth, cx);
                        }
                    })),
            )
            .into_any_element()
    }
}

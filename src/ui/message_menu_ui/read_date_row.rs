//! A private chat's read time row. When your own privacy hides it, a
//! "show when" button opens the read time box (tdesktop `WhenAction` with
//! `WhoReadState::MyHidden` and `ShowOrPremium::ReadTime`).
#![deny(clippy::indexing_slicing)]

use super::{MenuRow, info_row};
use crate::ui::app::QuillApp;
use crate::ui::profile_cover::ShowOrPremium;
use crate::ui::text_muted;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_time::CivilTime;
use quill::message_menu::read_status_label;
use quill::telegram::envelope::MessageReadDate;

impl QuillApp {
    pub(super) fn read_date_row(
        &self,
        order: u8,
        user_id: Option<i64>,
        read: MessageReadDate,
        now: &CivilTime,
        cx: &mut Context<Self>,
    ) -> MenuRow {
        let label = read_status_label(read, now);
        let Some(user_id) = user_id.filter(|_| read == MessageReadDate::MyPrivacyRestricted) else {
            return info_row(order, "menu-read-date", Some(IconName::Eye), label);
        };
        let open = cx.listener(move |this, _, _, cx| {
            this.close_message_menu();
            this.open_show_or_premium(ShowOrPremium::ReadTime, user_id, cx);
        });
        (
            order,
            div()
                .id("menu-read-date")
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_1p5()
                .text_xs()
                .text_color(text_muted())
                .child(Icon::new(IconName::Eye).size(px(16.)))
                .child(label)
                .child(
                    div()
                        .ml_auto()
                        .debug_selector(|| "menu-read-show".to_string())
                        .child(
                            Button::new("menu-read-show")
                                .label(tr(keys::CONTEXT_READ_SHOW))
                                .xsmall()
                                .rounded(px(9.))
                                .on_click(open),
                        ),
                )
                .into_any_element(),
        )
    }
}

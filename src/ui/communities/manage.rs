//! The community section of a group, channel or bot manage screen
//! (tdesktop `Controller::fillCommunitySection`): the linked community
//! and "Remove … from Community", or "Add … to a Community" with its
//! explanation.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::pressable::action_row;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use quill::state::InfoPanelTarget;

impl QuillApp {
    /// The section, or `None` where tdesktop shows none (not the owner, a
    /// basic group, a channel's direct messages).
    pub(in crate::ui) fn community_manage_section(
        &self,
        chat_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.chat_offers_community_section(chat_id) {
            return None;
        }
        let kind = self.community_chat_kind(chat_id);
        let muted = cx.theme().muted_foreground;
        let section = div().flex().flex_col().w_full().gap_1();
        let linked = session
            .chat_community_id(chat_id)
            .filter(|id| session.groups.communities.contains_key(id));
        Some(
            match linked {
                Some(community_id) => {
                    let name = self.community_name(community_id);
                    section
                        .child(
                            div()
                                .id("community-manage-linked")
                                .flex()
                                .items_center()
                                .gap_3()
                                .px_2()
                                .py_1p5()
                                .rounded_md()
                                .cursor_pointer()
                                .role(Role::Button)
                                .aria_label(name.clone())
                                .hover(|row| row.bg(cx.theme().list_hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_info_panel_target(
                                        InfoPanelTarget::Community(community_id),
                                        window,
                                        cx,
                                    );
                                }))
                                .child(super::row::community_avatar(&name, 40., cx))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .min_w_0()
                                        .child(div().font_semibold().truncate().child(name))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(tr(keys::COMMUNITY_TITLE)),
                                        ),
                                ),
                        )
                        .child(
                            action_row(
                                "community-manage-remove",
                                Some(IconName::CircleMinus),
                                by_kind(
                                    kind,
                                    keys::COMMUNITY_REMOVE_BUTTON,
                                    keys::COMMUNITY_REMOVE_BUTTON_CHANNEL,
                                    keys::COMMUNITY_REMOVE_BUTTON_BOT,
                                ),
                                true,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.ask_remove_from_community(community_id, chat_id, cx);
                                },
                            )),
                        )
                }
                None => section
                    .child(
                        action_row(
                            "community-manage-add",
                            Some(IconName::Users),
                            by_kind(
                                kind,
                                keys::COMMUNITY_ADD_BUTTON,
                                keys::COMMUNITY_ADD_BUTTON_CHANNEL,
                                keys::COMMUNITY_ADD_BUTTON_BOT,
                            ),
                            false,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_add_to_community(chat_id, cx);
                        })),
                    )
                    .child(
                        div()
                            .px_2()
                            .pb_2()
                            .text_xs()
                            .text_color(muted)
                            .child(by_kind(
                                kind,
                                keys::COMMUNITY_ADD_ABOUT,
                                keys::COMMUNITY_ADD_ABOUT_CHANNEL,
                                keys::COMMUNITY_ADD_ABOUT_BOT,
                            )),
                    ),
            }
            .into_any_element(),
        )
    }
}

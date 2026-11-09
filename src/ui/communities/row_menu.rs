//! The collapsed community row's right-click menu, in tdesktop's order
//! for a community entry (`window_peer_menu.cpp`): View community info,
//! Ungroup, the Mute submenu (`PeerMenuAddMuteSubmenuAction`,
//! `FillMuteMenu`) and Mark as read for every chat the row stands for.
//! Mute applies with `setCommunityNotificationSettings`, which the
//! community's chats follow while they keep their defaults.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::context_menu_keys::{MenuKeyTarget, context_menu_item, submenu_key};
use crate::ui::dismiss_layer::DismissLayer;
use crate::ui::menu_states::ChatMenuSubmenu;
use crate::ui::menu_surface::MenuSurface;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use quill::telegram::envelope::{
    ChatNotificationSettings, MUTE_FOR_1_HOUR, MUTE_FOR_2_DAYS, MUTE_FOR_8_HOURS, MUTE_FOREVER,
};

/// Width of the menu; the submenu opens beside it.
const MENU_WIDTH: f32 = 240.;
const PANEL_PADDING: f32 = 4.;
/// The Mute row's place among the menu's keyboard items (after View
/// community info and Ungroup), which its submenu rows sort under.
const MUTE_ROW_ORDER: u8 = 2;

/// The open row menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RowMenu {
    pub(crate) community_id: i64,
    pub(crate) position: Point<Pixels>,
    /// The Mute submenu, open by hover or by key.
    pub(crate) submenu: Option<ChatMenuSubmenu>,
}

impl RowMenu {
    pub(crate) fn at(community_id: i64, position: Point<Pixels>) -> Self {
        Self {
            community_id,
            position,
            submenu: None,
        }
    }
}

type MenuAction = Box<dyn Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)>;

impl QuillApp {
    /// Change the community's notification settings: the shown value
    /// follows at once (`community_notification_settings` reads the send).
    pub(super) fn set_community_notifications(
        &mut self,
        community_id: i64,
        change: impl FnOnce(&mut ChatNotificationSettings),
        cx: &mut Context<Self>,
    ) {
        let mut settings = self
            .session()
            .and_then(|s| s.community_notification_settings(community_id))
            .unwrap_or_default();
        change(&mut settings);
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .set_community_notification_settings(community_id, settings)
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    pub(super) fn set_community_mute_for(
        &mut self,
        community_id: i64,
        mute_for: i32,
        cx: &mut Context<Self>,
    ) {
        self.set_community_notifications(
            community_id,
            |settings| *settings = settings.clone().with_mute_for(mute_for),
            cx,
        );
    }

    /// "Mark as read" for every chat the row stands for.
    fn mark_community_read(&mut self, community_id: i64, cx: &mut Context<Self>) {
        let Some(summary) = self.community_row_summary(community_id) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let failed = summary
                .member_chat_ids
                .iter()
                .filter_map(|chat_id| {
                    live.driver
                        .mark_chat_as_read(quill::ids::ChatId(*chat_id))
                        .err()
                })
                .last();
            if let Some(err) = failed {
                self.connection
                    .set_note(StatusNote::error(user_error(&err)));
            }
        }
        cx.notify();
    }

    pub(in crate::ui) fn open_community_submenu(
        &mut self,
        which: ChatMenuSubmenu,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.admin.community.flow.row_menu.as_mut() else {
            return false;
        };
        if menu.submenu != Some(which) {
            menu.submenu = Some(which);
            cx.notify();
        }
        true
    }

    pub(in crate::ui) fn close_community_submenu(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.admin.community.flow.row_menu.as_mut() else {
            return false;
        };
        if menu.submenu.take().is_some() {
            cx.notify();
        }
        true
    }

    fn close_community_menu(&mut self, cx: &mut Context<Self>) {
        self.admin.community.flow.row_menu = None;
        cx.notify();
    }

    /// The Mute row: a direct Unmute while muted, else "Mute
    /// notifications" with the duration submenu, as the chat rows have.
    fn community_mute_row(&self, menu: RowMenu, cx: &mut Context<Self>) -> AnyElement {
        let community_id = menu.community_id;
        let settings = self
            .session()
            .and_then(|s| s.community_notification_settings(community_id))
            .unwrap_or_default();
        let items = &self.frame.menu.items;
        if settings.is_muted() {
            return context_menu_item(
                "community-menu-mute",
                IconName::Bell,
                tr(keys::MUTE_MENU_UNMUTE),
                text_menu().into(),
                true,
                items,
                cx,
            )
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if *hovered {
                    this.close_community_submenu(cx);
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.close_community_menu(cx);
                this.set_community_mute_for(community_id, 0, cx);
            }))
            .into_any_element();
        }
        items.mark_submenu_parent(MUTE_ROW_ORDER, ChatMenuSubmenu::Mute);
        let parent = context_menu_item(
            "community-menu-mute",
            IconName::BellOff,
            tr(keys::Q_CHAT_MUTE_NOTIFICATIONS),
            text_menu().into(),
            true,
            items,
            cx,
        )
        .relative()
        .child(div().flex_1())
        .child(Icon::new(IconName::ChevronRight).size(px(14.)))
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            if *hovered {
                this.open_community_submenu(ChatMenuSubmenu::Mute, cx);
            }
        }))
        .on_click(cx.listener(|this, _, _, cx| {
            this.open_community_submenu(ChatMenuSubmenu::Mute, cx);
        }));
        if menu.submenu != Some(ChatMenuSubmenu::Mute) {
            return parent.into_any_element();
        }
        let sound_off = !settings.use_default_sound && settings.sound_id == 0;
        let hover = cx.theme().accent;
        let sub_item = |index: usize, id: &'static str, icon: IconName, label: String| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .text_color(text_menu())
                .hover(|style| style.bg(hover))
                .menu_key_target(submenu_key(MUTE_ROW_ORDER, index), items, cx)
                .role(Role::MenuItem)
                .aria_label(label.clone())
                .child(Icon::new(icon).size(px(16.)))
                .child(label)
        };
        let mut rows: Vec<AnyElement> = vec![
            sub_item(
                0,
                "community-mute-sound",
                if sound_off {
                    IconName::Volume2
                } else {
                    IconName::VolumeX
                },
                if sound_off {
                    tr(keys::MUTE_MENU_SOUND_ON)
                } else {
                    tr(keys::MUTE_MENU_SOUND_OFF)
                }
                .to_string(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.close_community_menu(cx);
                let (use_default, sound_id) = quill::mute_menu::sound_toggle_target(sound_off);
                this.set_community_notifications(
                    community_id,
                    |settings| {
                        settings.use_default_sound = use_default;
                        settings.sound_id = sound_id;
                    },
                    cx,
                );
            }))
            .into_any_element(),
        ];
        for (index, (id, seconds)) in [
            ("community-mute-1h", MUTE_FOR_1_HOUR),
            ("community-mute-8h", MUTE_FOR_8_HOURS),
            ("community-mute-2d", MUTE_FOR_2_DAYS),
        ]
        .into_iter()
        .enumerate()
        {
            rows.push(
                sub_item(
                    1 + index,
                    id,
                    IconName::BellOff,
                    tr_with(
                        keys::MUTE_MENU_DURATION_ANY,
                        &[(
                            "duration",
                            quill::mute_menu::mute_period_text(seconds).as_str(),
                        )],
                    ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.close_community_menu(cx);
                    this.set_community_mute_for(community_id, seconds, cx);
                }))
                .into_any_element(),
            );
        }
        rows.push(
            sub_item(
                4,
                "community-mute-custom",
                IconName::Timer,
                tr(keys::MUTE_MENU_DURATION).to_string(),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_community_menu(cx);
                this.open_community_mute_for(community_id, window, cx);
            }))
            .into_any_element(),
        );
        rows.push(
            sub_item(
                5,
                "community-mute-forever",
                IconName::BellOff,
                tr(keys::MUTE_MENU_DURATION_FOREVER).to_string(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.close_community_menu(cx);
                this.set_community_mute_for(community_id, MUTE_FOREVER, cx);
            }))
            .into_any_element(),
        );
        // Beside the row, its top rows level with it; drawn after the
        // menu so it stays on top.
        let submenu = div()
            .absolute()
            .top(px(-PANEL_PADDING))
            .left_full()
            .child(deferred(
                div()
                    .id("community-mute-submenu")
                    .occlude()
                    .flex()
                    .flex_col()
                    .w(px(MENU_WIDTH))
                    .p_1()
                    .menu_surface(cx)
                    .children(rows),
            ));
        parent.child(submenu).into_any_element()
    }

    /// The row's right-click menu.
    pub(in crate::ui) fn community_row_menu_overlay(
        &self,
        menu: RowMenu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let community_id = menu.community_id;
        let any_unread = self
            .community_row_summary(community_id)
            .is_some_and(|summary| summary.unread_chats > 0);
        let item = |id: &'static str,
                    icon: IconName,
                    label: &'static str,
                    on_click: MenuAction,
                    cx: &mut Context<QuillApp>| {
            context_menu_item(
                id,
                icon,
                label,
                text_menu().into(),
                true,
                &self.frame.menu.items,
                cx,
            )
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if *hovered {
                    this.close_community_submenu(cx);
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.admin.community.flow.row_menu = None;
                on_click(this, window, cx);
                cx.notify();
            }))
            .into_any_element()
        };
        let mut rows: Vec<AnyElement> = vec![
            item(
                "community-menu-info",
                IconName::Info,
                tr(keys::CONTEXT_VIEW_COMMUNITY),
                Box::new(move |this, window, cx| {
                    this.open_info_panel_target(
                        quill::state::InfoPanelTarget::Community(community_id),
                        window,
                        cx,
                    );
                }),
                cx,
            ),
            item(
                "community-menu-ungroup",
                IconName::ChevronsUpDown,
                tr(keys::COMMUNITY_UNGROUP),
                Box::new(move |this, _, cx| {
                    this.show_community_box(CommunityBox::Ungroup { community_id }, cx);
                }),
                cx,
            ),
            self.community_mute_row(menu, cx),
        ];
        if any_unread {
            rows.push(item(
                "community-menu-read",
                IconName::CircleCheck,
                tr(keys::CONTEXT_MARK_READ),
                Box::new(move |this, _, cx| this.mark_community_read(community_id, cx)),
                cx,
            ));
        }
        let panel = div()
            .id("community-menu-panel")
            .role(Role::Group)
            .occlude()
            .flex()
            .flex_col()
            .w(px(MENU_WIDTH))
            .px_1()
            .py_1()
            .menu_surface(cx)
            .children(rows);
        DismissLayer::new(
            "community-menu-overlay",
            cx.listener(|this, _, _, cx| {
                this.admin.community.flow.row_menu = None;
                cx.notify();
            }),
        )
        .layer(|layer| layer.track_focus(&self.frame.menu.focus))
        .anchored(
            menu.position,
            div()
                .id("community-menu-region")
                .role(Role::Group)
                .a11y_tag(quill::agent::ui_driver::redact::REGION_MENU)
                .child(panel),
        )
        .focus_trap("community-menu-focus", &self.frame.menu.focus)
    }
}

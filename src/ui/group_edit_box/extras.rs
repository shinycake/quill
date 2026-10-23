//! The Edit box rows Quill has beyond tdesktop's list (hidden members,
//! anti-spam, topic management, the supergroup and broadcast upgrades,
//! ownership transfer), then the group's sticker and emoji packs and
//! Delete, which close tdesktop's list.
#![deny(clippy::indexing_slicing)]

use super::manage::{management_row, management_toggle_row};
use crate::ui::GroupConfirmAction;
use crate::ui::app::QuillApp;
use crate::ui::dialogs::{GroupSettingsAction, GroupSettingsView};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName as I;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::RequestPurpose;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    pub(super) fn quill_manage_rows(
        &self,
        chat_id: ChatId,
        is_channel: bool,
        is_basic_group: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let controls = session
            .map(|s| s.group_admin_controls(chat_id))
            .unwrap_or_default();
        let is_owner = session.is_some_and(|s| s.chat_is_owner(chat_id));
        let supergroup = !is_channel && !is_basic_group;
        let mut section = div().flex().flex_col().w_full().gap_1();
        if controls.hide_members {
            let on = session
                .and_then(|s| {
                    s.chat_supergroup(chat_id)
                        .and_then(|id| s.supergroup_full_info(id))
                        .map(|info| info.admin.has_hidden_members)
                })
                .unwrap_or(false);
            let busy = session.is_some_and(|s| s.group_admin_busy(chat_id));
            section = section.child(self.settings_switch(
                chat_id,
                "b7-hide-members",
                I::EyeOff,
                tr(keys::Q_PEOPLE_HIDE_MEMBERS),
                on,
                !busy,
                tr(keys::PROFILE_HIDE_PARTICIPANTS_ABOUT).into(),
                GroupSettingsAction::HiddenMembers(!on),
                cx,
            ));
        }
        if supergroup && session.is_some_and(|s| s.chat_can_toggle_anti_spam(chat_id)) {
            let enabled = self.anti_spam_enabled(chat_id);
            section = section.child(management_toggle_row(
                "g2-toggle-anti-spam",
                I::ShieldCheck,
                tr(keys::Q_PEOPLE_AGGRESSIVE_ANTI_SPAM),
                enabled,
                false,
                cx.listener(move |this, _, _, cx| {
                    let enabled = this.anti_spam_enabled(chat_id);
                    this.set_anti_spam(chat_id, !enabled, cx);
                }),
                cx,
            ));
        }
        if supergroup
            && session.is_some_and(|s| {
                s.chats
                    .get(&chat_id.0)
                    .is_some_and(|chat| chat.is_forum_chat())
                    && s.chat_can_manage_topics(chat_id)
            })
        {
            section = section.child(management_row(
                "g2-open-forum-manage",
                I::MessagesSquare,
                tr(keys::RIGHTS_GROUP_TOPICS),
                false,
                cx.listener(move |this, _, window, cx| {
                    this.open_forum_manage_dialog(chat_id, window, cx);
                }),
                cx,
            ));
        }
        if controls.upgrade {
            section = section.child(management_row(
                "b7-upgrade",
                I::ArrowUp,
                tr(keys::Q_PEOPLE_UPGRADE_TO_SUPERGROUP),
                false,
                cx.listener(move |this, _, _, cx| {
                    this.open_group_settings_view(chat_id, GroupSettingsView::ConfirmUpgrade, cx);
                }),
                cx,
            ));
        }
        if supergroup && is_owner && !self.chat_is_broadcast(chat_id) {
            section = section
                .child(management_row(
                    "g1-broadcast-upgrade",
                    I::Megaphone,
                    tr(keys::RIGHTS_GIGAGROUP_CONVERT),
                    false,
                    cx.listener(move |this, _, _, cx| {
                        this.open_group_confirm(chat_id, GroupConfirmAction::BroadcastIntro, cx);
                    }),
                    cx,
                ))
                .child(super::manage::divider_text(
                    quill::admin_extras::broadcast_about(),
                    cx,
                ));
        }
        if self.group_flavor(chat_id).is_some() && is_owner {
            section = section.child(management_row(
                "g1-transfer-ownership",
                I::Crown,
                if is_channel {
                    tr(keys::Q_PEOPLE_TRANSFER_CHANNEL_OWNERSHIP)
                } else {
                    tr(keys::RIGHTS_TRANSFER_GROUP)
                },
                false,
                cx.listener(move |this, _, window, cx| {
                    this.open_transfer_ownership(chat_id, window, cx);
                }),
                cx,
            ));
        }
        if supergroup && session.is_some_and(|s| s.chat_can_set_sticker_set(chat_id)) {
            section = section.child(self.group_sticker_choices(chat_id, cx));
        }
        let can_delete = session
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.can_be_deleted_for_all_users);
        if can_delete {
            section = section.child(management_row(
                "g1-delete-chat",
                I::Trash,
                if is_channel {
                    tr(keys::PROFILE_DELETE_CHANNEL)
                } else {
                    tr(keys::PROFILE_DELETE_GROUP)
                },
                true,
                cx.listener(move |this, _, _, cx| {
                    this.open_group_confirm(chat_id, GroupConfirmAction::DeleteChat, cx);
                }),
                cx,
            ));
        }
        section.into_any_element()
    }

    fn anti_spam_enabled(&self, chat_id: ChatId) -> bool {
        self.session()
            .and_then(|session| {
                let supergroup = session.chat_supergroup(chat_id)?;
                session
                    .groups
                    .supergroup_anti_spam_enabled
                    .get(&supergroup)
                    .copied()
            })
            .unwrap_or(false)
    }

    fn group_sticker_choices(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let info = session.chats.get(&chat_id.0).and_then(|c| match c.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                session.supergroup_full_info(supergroup_id)
            }
            _ => None,
        });
        let busy = [
            RequestPurpose::SetSupergroupStickerSet,
            RequestPurpose::SetSupergroupCustomEmojiStickerSet,
        ]
        .iter()
        .any(|p| session.requests.has_purpose_for_chat(*p, chat_id));
        let loading = [
            RequestPurpose::GetInstalledStickerSets,
            RequestPurpose::GetInstalledEmojiSets,
        ]
        .iter()
        .any(|p| session.requests.has_purpose(*p));
        let mut section = div().flex().flex_col().gap_1().child(
            Button::new("group-load-sticker-packs")
                .label(if loading {
                    tr(keys::Q_PEOPLE_LOADING_PACKS)
                } else {
                    tr(keys::Q_PEOPLE_LOAD_REFRESH_GROUP_PACKS)
                })
                .ghost()
                .disabled(loading)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut()
                        && live.driver.load_group_sticker_choices(chat_id).is_err()
                    {
                        this.connection.set_note(StatusNote::error(tr(
                            keys::Q_PEOPLE_COULD_NOT_LOAD_GROUP_PACKS,
                        )));
                    }
                    cx.notify();
                })),
        );
        if session.stickers.stickers.failed || session.stickers.emoji.failed {
            section = section.child(
                div()
                    .text_sm()
                    .child(tr(keys::Q_PEOPLE_COULD_NOT_LOAD_PACKS_RETRY)),
            );
        }
        for (custom, title, current, sets) in [
            (
                false,
                tr(keys::GROUP_STICKERS),
                info.map_or(0, |i| i.sticker_set_id),
                &session.stickers.stickers.sets,
            ),
            (
                true,
                tr(keys::EMOJI_GROUP_BADGE),
                info.map_or(0, |i| i.custom_emoji_sticker_set_id),
                &session.stickers.emoji.installed_sets,
            ),
        ] {
            let current_label = if current == 0 {
                tr(keys::Q_PEOPLE_NONE).to_string()
            } else {
                sets.iter()
                    .find(|s| s.id == current)
                    .map(|s| s.title.clone())
                    .unwrap_or_else(|| {
                        tr_with(keys::Q_PEOPLE_PACK, &[("current", &current.to_string())])
                    })
            };
            let mut choices = div()
                .id(if custom {
                    "group-emoji-choices"
                } else {
                    "group-sticker-choices"
                })
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(160.))
                .overflow_y_scroll()
                .child(div().text_sm().child(format!("{title}: {current_label}")));
            for (id, label) in std::iter::once((0, tr(keys::Q_PEOPLE_REMOVE_PACK).to_string()))
                .chain(sets.iter().map(|s| (s.id, s.title.clone())))
            {
                choices = choices.child(
                    Button::new(format!("group-pack-{custom}-{id}"))
                        .label(label)
                        .ghost()
                        .selected(id == current)
                        .disabled(busy || id == current)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(live) = this.live.as_mut() {
                                let result = if custom {
                                    live.driver
                                        .set_supergroup_custom_emoji_sticker_set(chat_id, id)
                                } else {
                                    live.driver.set_supergroup_sticker_set(chat_id, id)
                                };
                                this.connection.set_note(match result {
                                    Ok(Some(_)) => StatusNote::silent(tr(
                                        keys::Q_PEOPLE_GROUP_PACK_CHANGE_REQUESTED,
                                    )),
                                    Ok(None) => StatusNote::info(tr(
                                        keys::Q_PEOPLE_GROUP_PACK_CHANGE_IS_UNAVAILABLE,
                                    )),
                                    Err(_) => StatusNote::error(tr(
                                        keys::Q_PEOPLE_COULD_NOT_CHANGE_GROUP_PACK,
                                    )),
                                });
                            }
                            cx.notify();
                        })),
                );
            }
            section = section.child(choices);
        }
        section.into_any_element()
    }
}

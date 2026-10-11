//! The row of labeled icon tiles under a user profile's name (tdesktop's
//! `info_profile_top_bar.cpp` / `info_profile_actions.cpp` button row).
//! Which tiles appear is pure logic in [`user_profile_actions`]; the
//! rendering below wires each tile to Quill's existing flow.

use crate::ui::app::QuillApp;
use crate::ui::group_panels::info_tile;
use crate::ui::navigation::NavigationAction;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::telegram::envelope::MUTE_FOREVER;

/// One tile of the profile action row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum ProfileAction {
    Message,
    Mute,
    Call,
    Video,
    Gift,
    SecretChat,
    AddContact,
    EditProfile,
    More,
}

/// What decides the tiles of a user profile.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::ui) struct UserActionFacts {
    pub is_self: bool,
    pub is_bot: bool,
    pub is_contact: bool,
    /// The open chat is the private chat with this user (the header
    /// panel), so Mute, Gift and More act on it.
    pub in_own_chat: bool,
    /// Calls and secret chats are allowed (non-bot, not yourself).
    pub can_reach: bool,
}

/// The tiles for a user profile, in tdesktop's order (Message, Mute,
/// Call, Video, Gift, ... More). Gift and More need the open private
/// chat because Quill's gift picker and chat menu act on the open chat.
pub(in crate::ui) fn user_profile_actions(f: UserActionFacts) -> Vec<ProfileAction> {
    use ProfileAction::*;
    let mut out = Vec::new();
    if f.is_self {
        out.push(EditProfile);
        return out;
    }
    if !f.in_own_chat {
        out.push(Message);
    }
    if f.in_own_chat {
        out.push(Mute);
    }
    if f.can_reach {
        out.extend([Call, Video]);
    }
    if f.in_own_chat && !f.is_bot {
        out.push(Gift);
    }
    if f.can_reach {
        out.push(SecretChat);
    }
    if !f.is_contact && !f.is_bot {
        out.push(AddContact);
    }
    if f.in_own_chat {
        out.push(More);
    }
    out
}

impl QuillApp {
    /// The rendered action row of a user profile, `None` when empty.
    pub(in crate::ui) fn user_profile_action_row(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session();
        let user = session.and_then(|s| s.user(user_id));
        let open_chat = session.and_then(|s| s.open_chat);
        let facts = UserActionFacts {
            is_self: session.and_then(|s| s.my_user_id) == Some(user_id),
            is_bot: user.is_some_and(|u| u.is_bot),
            is_contact: user.is_none_or(|u| u.is_contact),
            in_own_chat: session
                .zip(open_chat)
                .is_some_and(|(s, chat)| s.private_chat_user_id(chat) == Some(user_id)),
            can_reach: session.is_some_and(|s| Self::can_start_secret_chat_with(s, user_id)),
        };
        let actions = user_profile_actions(facts);
        if actions.is_empty() {
            return None;
        }
        let extras = session
            .and_then(|s| s.user_full_info(user_id))
            .map(|i| &i.extras);
        let call_off = extras.and_then(|e| {
            if e.calls_private {
                Some("This user's privacy settings don't allow calls.")
            } else if e.calls_blocked {
                Some("This user can't be called.")
            } else {
                None
            }
        });
        let video_off = call_off.or_else(|| {
            extras
                .is_some_and(|e| e.video_calls_unsupported)
                .then_some("This user doesn't support video calls.")
        });
        let muted = session
            .zip(open_chat)
            .and_then(|(s, chat)| s.chats.get(&chat.0))
            .is_some_and(|c| c.is_muted());
        let mut row = div().flex().flex_wrap().justify_center().gap_2().w_full();
        for action in actions {
            row = row.child(match action {
                ProfileAction::Message => info_tile(
                    "info-panel-message",
                    IconName::MessageSquare,
                    "Message",
                    cx.listener(move |this, _, window, cx| {
                        this.dismiss_profile_modal();
                        this.open_user_chat(user_id, window, cx);
                    }),
                    cx,
                )
                .into_any_element(),
                ProfileAction::Mute => info_tile(
                    "info-panel-mute",
                    if muted {
                        IconName::Bell
                    } else {
                        IconName::BellOff
                    },
                    if muted { "Unmute" } else { "Mute" },
                    cx.listener(move |this, _, _, cx| {
                        if let Some(chat) = this.session().and_then(|s| s.open_chat) {
                            this.apply_chat_mute(chat, if muted { 0 } else { MUTE_FOREVER }, cx);
                        }
                    }),
                    cx,
                )
                .into_any_element(),
                ProfileAction::Call if call_off.is_some() => {
                    disabled_tile("info-panel-call", IconName::Phone, "Call", call_off, cx)
                }
                ProfileAction::Video if video_off.is_some() => disabled_tile(
                    "info-panel-video-call",
                    IconName::Video,
                    "Video",
                    video_off,
                    cx,
                ),
                ProfileAction::Call => info_tile(
                    "info-panel-call",
                    IconName::Phone,
                    "Call",
                    cx.listener(move |this, _, _, cx| this.start_call_for_user(user_id, false, cx)),
                    cx,
                )
                .into_any_element(),
                ProfileAction::Video => info_tile(
                    "info-panel-video-call",
                    IconName::Video,
                    "Video",
                    cx.listener(move |this, _, _, cx| this.start_call_for_user(user_id, true, cx)),
                    cx,
                )
                .into_any_element(),
                ProfileAction::Gift => info_tile(
                    "info-panel-gift",
                    IconName::Gift,
                    "Gift",
                    cx.listener(|this, _, window, cx| {
                        this.navigate(NavigationAction::Gift, window, cx)
                    }),
                    cx,
                )
                .into_any_element(),
                ProfileAction::SecretChat => info_tile(
                    "info-panel-start-secret",
                    IconName::Lock,
                    "Secret chat",
                    cx.listener(move |this, _, _, cx| this.start_secret_chat_for_user(user_id, cx)),
                    cx,
                )
                .into_any_element(),
                ProfileAction::AddContact => info_tile(
                    "info-panel-add-contact",
                    IconName::UserPlus,
                    "Add contact",
                    cx.listener(move |this, _, window, cx| {
                        this.open_add_contact_dialog(user_id, window, cx)
                    }),
                    cx,
                )
                .into_any_element(),
                ProfileAction::EditProfile => info_tile(
                    "info-panel-edit-profile",
                    IconName::Pencil,
                    "Edit profile",
                    cx.listener(|this, _, window, cx| this.open_edit_profile_dialog(window, cx)),
                    cx,
                )
                .into_any_element(),
                // The chat header's menu button, under a label like the
                // other tiles.
                ProfileAction::More => div()
                    .id("info-panel-more")
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_0p5()
                    .w(px(76.))
                    .py_1()
                    .rounded_lg()
                    .bg(cx.theme().secondary)
                    .child(self.chat_navigation_menu(cx))
                    .child(div().text_xs().child("More"))
                    .into_any_element(),
            });
        }
        Some(row.into_any_element())
    }
}

/// A tile that cannot be used, with the reason as its tooltip.
fn disabled_tile(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    reason: Option<&'static str>,
    cx: &App,
) -> AnyElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .w(px(76.))
        .py_2()
        .rounded_lg()
        .bg(cx.theme().secondary)
        .opacity(0.5)
        .role(gpui_kit::Role::Button)
        .aria_label(label)
        .when_some(reason, |this, text| {
            this.tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(text).build(window, cx)
            })
        })
        .child(
            Icon::new(icon)
                .size(px(18.))
                .text_color(cx.theme().muted_foreground),
        )
        .child(div().text_xs().child(label))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::ProfileAction::*;
    use super::{UserActionFacts, user_profile_actions};

    fn facts() -> UserActionFacts {
        UserActionFacts {
            is_contact: true,
            can_reach: true,
            ..Default::default()
        }
    }

    #[test]
    fn contact_in_own_chat_has_the_full_row() {
        let f = UserActionFacts {
            in_own_chat: true,
            ..facts()
        };
        assert_eq!(
            user_profile_actions(f),
            [Mute, Call, Video, Gift, SecretChat, More]
        );
    }

    #[test]
    fn member_profile_opens_the_chat_instead_of_muting() {
        assert_eq!(
            user_profile_actions(facts()),
            [Message, Call, Video, SecretChat]
        );
    }

    #[test]
    fn bot_has_no_call_gift_or_secret_chat() {
        let f = UserActionFacts {
            is_bot: true,
            can_reach: false,
            in_own_chat: true,
            ..facts()
        };
        assert_eq!(user_profile_actions(f), [Mute, More]);
    }

    #[test]
    fn stranger_can_be_added() {
        let f = UserActionFacts {
            is_contact: false,
            ..facts()
        };
        assert!(user_profile_actions(f).contains(&AddContact));
    }

    #[test]
    fn own_profile_only_edits() {
        let f = UserActionFacts {
            is_self: true,
            can_reach: false,
            in_own_chat: true,
            ..facts()
        };
        assert_eq!(user_profile_actions(f), [EditProfile]);
    }
}

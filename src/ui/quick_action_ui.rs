//! Settings radio group and double-click dispatch for the message quick
//! action (tdesktop `chatQuickAction`, `ListWidget::mouseDoubleClickEvent`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::quick_action::{QuickAction, QuickEffect};

impl QuillApp {
    /// Settings: "Double-click a message to" Reply / React.
    pub(super) fn quick_action_radios(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.session().map_or_else(QuickAction::default, |s| {
            s.settings.media_prefs.quick_action
        });
        div()
            .id("media-pref-quick-action")
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .child(tr(keys::Q_LEFTOVERS_DOUBLE_CLICK_A_MESSAGE_TO)),
            )
            .child(
                RadioGroup::vertical("media-pref-quick-action-group")
                    .selected_index(Some(current.index()))
                    .children(QuickAction::ALL.iter().map(|(_, label)| {
                        Radio::new(format!("quick-action-{label}")).label(*label)
                    }))
                    .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                        if let Some(action) = QuickAction::ALL
                            .get(ix.min(QuickAction::ALL.len() - 1))
                            .map(|entry| entry.0)
                        {
                            this.set_media_pref(|prefs| prefs.quick_action = action, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    /// A double-click landed on a message: run the chosen quick action.
    pub(super) fn double_click_action(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let sent = session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.messages.get(&message_id.0))
            .is_some_and(|m| m.can_react());
        match session.settings.media_prefs.quick_action.effect(sent) {
            QuickEffect::Reply => self.begin_reply_from_message(chat_id, message_id, window, cx),
            QuickEffect::React => self.quick_react(chat_id, message_id, cx),
            QuickEffect::Nothing => {}
        }
    }
}

//! The call windows under the passcode lock: tdesktop's per-layer rule.
//! The call panel stays up and answerable (`LockRole::Unlocked` in
//! `passcode::lock_layer`), but `Application::lockByPasscode` hides its
//! layers (`Calls::Instance::hidePanelLayers`), and Add People and the share
//! link actions ask for the passcode first (`Panel::addPeople`,
//! `GroupPanel::addMembers`, `shareConferenceLink`). So at the lock every
//! box over a call window closes, the stream key is hidden again, and while
//! locked the openers of those boxes refuse and the stream URL and key are
//! not drawn. Mute, video, screen share, hang up and answer keep working.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::*;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Called from `close_for_lock`.
    pub(in crate::ui) fn close_call_layers_for_lock(&mut self, cx: &mut Context<Self>) {
        if self.call_boxes.add_people.take().is_some()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.search_add_people("");
        }
        self.group_call.stream.overlay = None;
        self.group_call.stream.key_shown = false;
        self.group_call_screen.title_dialog = None;
        self.group_call_screen.settings_open = false;
        self.screen_share.privacy_for = None;
        cx.notify();
    }
}

/// A call card on the main window's lock screen: a ringing call that the
/// locked window's kit box cannot show (the box is a layer of the locked
/// window, so it is not drawn). `locked_root` stacks the cards.
pub(in crate::ui) fn lock_call_card(
    id: &'static str,
    title: String,
    body: String,
    decline: Button,
    answer: Button,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let theme = cx.theme();
    div()
        .id(id)
        .w(px(360.))
        .p_3()
        .gap_2()
        .flex()
        .flex_col()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(div().text_sm().child(body))
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(decline)
                .child(answer),
        )
        .into_any_element()
}

impl QuillApp {
    /// A second call ringing in during a call, on the lock screen. Its
    /// prompt is a kit box of the main window, not drawn while locked, and
    /// the call windows stay answerable under the lock, so this one must
    /// be too: Decline, or End & Answer.
    pub(in crate::ui) fn swap_call_lock_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let title = self.swap_call_title()?;
        let decline = Button::new("lock-swap-call-decline")
            .label(tr(keys::CALL_DECLINE))
            .danger()
            .on_click(cx.listener(|this, _, _, cx| this.decline_swap_call(cx)));
        let answer = Button::new("lock-swap-call-answer")
            .label(tr(keys::Q_OPS_END_ANSWER))
            .primary()
            .on_click(cx.listener(|this, _, _, cx| this.answer_swap_call(cx)));
        Some(lock_call_card(
            "lock-swap-call",
            title,
            tr(keys::Q_OPS_YOURE_ALREADY_IN_A_CALL).to_string(),
            decline,
            answer,
            cx,
        ))
    }
}

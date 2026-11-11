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

/// The element ids of one second-call card: the lock screen's and the
/// call window's are in different windows, and tests name each.
struct SwapCardIds {
    card: &'static str,
    decline: &'static str,
    answer: &'static str,
}

impl QuillApp {
    /// A second call ringing in during a call, on the lock screen. Its
    /// prompt is a kit box of the main window, not drawn while locked, and
    /// the call windows stay answerable under the lock, so this one must
    /// be too: Decline, or End & Answer.
    pub(in crate::ui) fn swap_call_lock_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.swap_call_card(
            SwapCardIds {
                card: "lock-swap-call",
                decline: "lock-swap-call-decline",
                answer: "lock-swap-call-answer",
            },
            cx,
        )
    }

    /// The same card over the call window, locked or not. The kit box
    /// (`CallSwap`) is session-wide and shows in the window of the last
    /// press, which can be a main window hidden to the tray or behind the
    /// call: the call window, where the user is during a call, answers
    /// the second call too.
    pub(in crate::ui) fn swap_call_window_card(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.swap_call_card(
            SwapCardIds {
                card: "call-window-swap-call",
                decline: "call-window-swap-call-decline",
                answer: "call-window-swap-call-answer",
            },
            cx,
        )
    }

    fn swap_call_card(&self, ids: SwapCardIds, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (title, caller) = self.swap_call_text()?;
        let decline = Button::new(ids.decline)
            .label(tr(keys::CALL_DECLINE))
            .danger()
            .on_click(cx.listener(|this, _, _, cx| this.decline_swap_call(cx)));
        let answer = Button::new(ids.answer)
            .label(tr(keys::Q_OPS_END_ANSWER))
            .primary()
            .on_click(cx.listener(|this, _, _, cx| this.answer_swap_call(cx)));
        Some(lock_call_card(ids.card, title, caller, decline, answer, cx))
    }
}

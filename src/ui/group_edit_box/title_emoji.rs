//! The name field's emoji toggle (tdesktop `EditPeerInfoBox` with a
//! `TabbedPanel` in `Mode::PeerTitle`): the shared emoji panel drops down
//! under the toggle, standard emoji only, and a picked emoji goes in at the
//! cursor.

use crate::ui::app::QuillApp;
use crate::ui::media_panel::PanelTab;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::{Selectable, Sizable};
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The emoji panel is open for the Edit box's name field.
    pub(in crate::ui) fn group_title_emoji_active(&self) -> bool {
        self.group_edit_box().is_some_and(|edit| edit.title_emoji)
            && self.pickers.media_panel.open
            && self.pickers.media_panel.reaction.is_none()
            && self.pickers.media_panel.status.is_none()
    }

    pub(super) fn toggle_group_title_emoji(&mut self, cx: &mut Context<Self>) {
        if self.group_title_emoji_active() {
            self.close_group_title_emoji(cx);
            return;
        }
        let Some(edit) = self.group_edit_box_mut() else {
            return;
        };
        edit.title_emoji = true;
        self.open_media_panel(PanelTab::Emoji, cx);
    }

    pub(super) fn close_group_title_emoji(&mut self, cx: &mut Context<Self>) {
        let active = self.group_title_emoji_active();
        if let Some(edit) = self.group_edit_box_mut() {
            edit.title_emoji = false;
        }
        if active {
            self.close_media_panel(cx);
        }
    }

    /// An emoji picked in the panel. `true` when it went to the name.
    pub(in crate::ui) fn route_group_title_emoji(
        &mut self,
        emoji: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.group_title_emoji_active() {
            return false;
        }
        let Some(field) = self.group_edit_box().map(|edit| edit.title_input.clone()) else {
            return false;
        };
        field.update(cx, |input, cx| {
            input.replace(emoji.to_string(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
        true
    }

    /// The toggle drawn inside the name field, with the open panel
    /// hanging from its bottom-right corner.
    pub(super) fn group_title_emoji_toggle(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let toggle = Button::new("group-edit-title-emoji")
            .icon(IconName::FaceSlightlySmiling)
            .ghost()
            .xsmall()
            .selected(self.group_title_emoji_active())
            .tooltip(tr(keys::SWITCH_EMOJI))
            .accessibility_label(tr(keys::SWITCH_EMOJI))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_group_title_emoji(cx)));
        div()
            .relative()
            .child(toggle)
            .children(self.group_title_emoji_layer(window, cx))
            .into_any_element()
    }

    /// The open panel, over the box, and a cover that closes it on a click
    /// anywhere else.
    fn group_title_emoji_layer(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.group_title_emoji_active() {
            return None;
        }
        let panel = self.media_panel(cx);
        let cover = deferred(
            anchored().position(point(px(0.), px(0.))).child(
                div()
                    .id("group-edit-title-emoji-cover")
                    .w(window.viewport_size().width)
                    .h(window.viewport_size().height)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.close_group_title_emoji(cx)),
                    ),
            ),
        )
        .with_priority(gpui_base::POPUP_PRIORITY - 1);
        let popup = deferred(
            anchored()
                .anchor(Anchor::TopRight)
                .offset(point(px(0.), px(8.)))
                .snap_to_window_with_margin(px(8.))
                .child(
                    div()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(panel),
                ),
        )
        .with_priority(gpui_base::POPUP_PRIORITY);
        // A point at the toggle's bottom-right corner to hang the panel from.
        Some(
            div()
                .absolute()
                .top_full()
                .right_0()
                .size_0()
                .child(cover)
                .child(popup)
                .into_any_element(),
        )
    }
}

//! The `/` command menu popup above the composer.

use super::*;
use quill::composer::commands_with_username;

impl QuillApp {
    /// Phase 3.3: the `/` command menu popup above the composer.
    /// Bot-specific commands first, then the global (`getCommands`)
    /// section when both exist. A bot's rows carry its userpic, and in a
    /// group the command reads `/command@bot` (tdesktop `FieldAutocomplete`).
    /// Tap sends; Up/Down/Enter via the key interceptor; Esc / blur /
    /// outside tap dismisses.
    pub(in crate::ui) fn command_menu_dropdown(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (prefix, items) = self.command_menu_state(cx)?;
        let selected = self
            .composer_ui
            .command_menu_selected
            .min(items.len().saturating_sub(1));
        let has_specific = items.iter().any(|item| !item.global);
        let has_global = items.iter().any(|item| item.global);
        let show_headers = has_specific && has_global;
        let with_username = commands_with_username(&items, &prefix);
        let mut list = div()
            .id("command-menu")
            .flex()
            .flex_col()
            // A compact popup that scrolls (Telegram Desktop), not a
            // list covering the whole chat.
            .max_h(px(320.))
            .overflow_y_scroll()
            .mx_4()
            .mb_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);
        let mut global_header_shown = false;
        for (index, item) in items.iter().enumerate() {
            if show_headers && item.global && !global_header_shown {
                global_header_shown = true;
                list = list.child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_CHAT_GLOBAL)),
                );
            }
            if item.quick_reply == Some(quill::quick_replies::EDIT_ROW) {
                list = list.child(
                    div()
                        .id(("command-menu-item", index as u64))
                        .w_full()
                        .flex()
                        .justify_center()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .role(gpui_kit::Role::Button)
                        .aria_label(item.description.clone())
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .when(index == selected, |this| this.bg(cx.theme().selection))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();
                                this.send_command_menu_index(index, window, cx);
                            }),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().primary)
                                .child(item.description.clone()),
                        ),
                );
                continue;
            }
            let command = item.command_text(with_username);
            let label = if item.description.is_empty() {
                command.clone()
            } else {
                format!("{command} — {}", item.description)
            };
            let highlighted = index == selected;
            let avatar = item.bot.as_ref().map(|bot| {
                let photo = self
                    .session()
                    .and_then(|s| s.user_photo_path(bot.user_id))
                    .and_then(|path| {
                        quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
                    });
                crate::ui::message_text::kit_avatar_element(&bot.name, photo.as_deref(), px(28.))
            });
            let text = if avatar.is_some() {
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_baseline()
                    .gap_2()
                    .text_sm()
                    .child(div().font_semibold().flex_none().child(command))
                    .when(!item.description.is_empty(), |this| {
                        this.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(cx.theme().muted_foreground)
                                .child(item.description.clone()),
                        )
                    })
            } else {
                div().text_sm().flex_1().min_w_0().child(label.clone())
            };
            // Ephemeral-command icon (Telegram blog "Ephemeral Bot
            // Messages": ephemeral commands are marked with a special
            // icon in the bot menu) — eye-off, muted, trailing: the
            // command's result is only visible to the sender.
            let mut row = div()
                .id(("command-menu-item", index as u64))
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_2()
                .rounded_md()
                .role(gpui_kit::Role::Button)
                .aria_label(label)
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .when(highlighted, |this| this.bg(cx.theme().selection))
                // On press, not click: the press moves focus off the
                // composer, whose blur closes this menu before a click
                // could land. Telegram Desktop sends a clicked command.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        window.prevent_default();
                        this.send_command_menu_index(index, window, cx);
                    }),
                )
                .children(avatar)
                .child(text);
            if item.is_ephemeral {
                row = row.child(
                    Icon::new(IconName::EyeOff)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                );
            }
            list = list.child(row);
        }
        Some(list.into_any_element())
    }
}

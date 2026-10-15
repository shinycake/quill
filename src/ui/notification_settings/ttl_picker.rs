//! The auto-delete timer picker.

use super::*;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header. The picker is secret-chat-only (the header
    /// button is gated), so only the secret presets are reachable today;
    /// the non-secret day-multiple branch below is defensive, kept for
    /// the planned regular-chat picker follow-up
    /// (`setChatMessageAutoDeleteTime`, schema 1.8.67 line 13454).
    pub(in crate::ui) fn ttl_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open_chat = session.as_ref().and_then(|s| s.open_chat);
        let open_chat_summary: Option<&ChatSummary> =
            open_chat.and_then(|id| session.as_ref()?.chats.get(&id.0));
        let is_secret =
            open_chat_summary.is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let current = open_chat_summary
            .and_then(|chat| chat.ttl_status_line())
            .unwrap_or_else(|| tr(keys::SETTINGS_TTL_AFTER_OFF).to_string());
        let title = if is_secret {
            tr(keys::SETTINGS_TTL_AFTER_SUBTITLE)
        } else {
            tr(keys::Q_PREFS_AUTO_DELETE_TIMER)
        };
        // Schema value rule (1.8.67 line 13454): secret chats accept
        // arbitrary seconds; other chats need day multiples.
        let presets: Vec<(&str, i32)> = if is_secret {
            vec![
                (tr(keys::SETTINGS_TTL_AFTER_OFF), 0),
                ("5s", 5),
                ("30s", 30),
                ("1m", 60),
                ("1h", 3600),
                ("1d", 86400),
                ("1w", 604800),
            ]
        } else {
            // tdesktop `lng_manage_messages_ttl_after1..3`; a longer or
            // odd period goes through the Custom stepper below.
            vec![
                (tr(keys::SETTINGS_TTL_AFTER_OFF), 0),
                (tr(keys::MANAGE_MESSAGES_TTL_AFTER1), 86_400),
                (tr(keys::MANAGE_MESSAGES_TTL_AFTER2), 604_800),
                (tr(keys::MANAGE_MESSAGES_TTL_AFTER3), 2_678_400),
            ]
        };
        // Phase 6: kit RadioGroup (was: buttons with a ● prefix on the
        // active preset). Controlled: the chosen index writes the value.
        let active_ix = presets.iter().position(|(_, secs)| {
            open_chat_summary.is_some_and(|chat| chat.message_auto_delete_time == *secs)
        });
        let preset_row = RadioGroup::horizontal("ttl-presets")
            .selected_index(active_ix)
            .children(
                presets
                    .iter()
                    .map(|(label, secs)| Radio::new(format!("ttl-set-{secs}")).label(*label)),
            )
            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                let Some(secs) = presets.get(ix).map(|preset| preset.1) else {
                    return;
                };
                if let Some(chat_id) = open_chat {
                    this.apply_chat_ttl(chat_id, secs, cx);
                }
            }));
        let custom_row = (!is_secret).then(|| {
            let secs = self.notify.ttl_custom_secs;
            let mut row = div()
                .id("ttl-custom")
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .child(
                    Button::new("ttl-custom-toggle")
                        .small()
                        .label(tr(keys::Q_PREFS_CUSTOM))
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notify.ttl_custom_open = !this.notify.ttl_custom_open;
                            cx.notify();
                        })),
                );
            if self.notify.ttl_custom_open {
                row = row
                    .child(
                        Button::new("ttl-custom-minus")
                            .small()
                            .ghost()
                            .label("\u{2212}")
                            .accessibility_label(tr(keys::Q_PREFS_SHORTER))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notify.ttl_custom_secs =
                                    quill::auto_delete::step(this.notify.ttl_custom_secs, -1);
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().child(quill::auto_delete::format_ttl(secs)))
                    .child(
                        Button::new("ttl-custom-plus")
                            .small()
                            .ghost()
                            .label("+")
                            .accessibility_label(tr(keys::Q_PREFS_LONGER))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notify.ttl_custom_secs =
                                    quill::auto_delete::step(this.notify.ttl_custom_secs, 1);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("ttl-custom-apply")
                            .small()
                            .label(tr(keys::Q_PREFS_ENABLE_AUTO_DELETE))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(chat_id) = open_chat {
                                    let secs = this.notify.ttl_custom_secs;
                                    this.apply_chat_ttl(chat_id, secs, cx);
                                }
                            })),
                    );
            }
            row
        });
        let about = if is_secret {
            tr(keys::Q_PREFS_MESSAGES_AUTO_DELETE_AFTER_THE).to_string()
        } else {
            open_chat_summary
                .map(|chat| quill::auto_delete::about_line(&chat.kind).to_string())
                .unwrap_or_default()
        };
        div()
            .id("ttl-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child(title))
                    .child(
                        Button::new("close-ttl-picker")
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip(tr(keys::CLOSE))
                            .accessibility_label(tr(keys::CLOSE))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notify.ttl_picker_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr_with(
                        keys::Q_PREFS_CURRENT,
                        &[
                            ("current", &current.to_string()),
                            ("about", &about.to_string()),
                        ],
                    )),
            )
            .child(preset_row)
            .children(custom_row)
    }
}

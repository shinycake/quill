//! Methods moved out of `privacy.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Slice S3: one Everybody / My contacts / Nobody radio row.
    pub(in crate::ui) fn privacy_radio_row(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        who: PrivacyWho,
        current: Option<PrivacyWho>,
    ) -> AnyElement {
        let selected = current == Some(who);
        let locked = self.privacy_choice_locked(target, who);
        div()
            .id(format!(
                "privacy-choice-{}-{}",
                target.label().replace(' ', "-"),
                who.label().replace(' ', "-")
            ))
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{} · {}", target.label(), who.label()))
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(if selected {
                cx.theme().accent.opacity(0.15)
            } else {
                cx.theme().background
            })
            .child(div().text_xs().child(if selected { "●" } else { "○" }))
            .child(div().text_sm().child(who.label()))
            .when(locked, |row| {
                row.child(
                    Icon::new(IconName::Lock)
                        .size(px(12.))
                        .text_color(cx.theme().muted_foreground),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.set_privacy_target_who(target, who, window, cx);
            }))
            .into_any_element()
    }

    /// Narrowing who can send voice messages needs Premium (tdesktop
    /// `VoicesPrivacyController::premiumClickedCallback`).
    fn privacy_choice_locked(&self, target: PrivacyEditorTarget, who: PrivacyWho) -> bool {
        matches!(target, PrivacyEditorTarget::Rule(key) if key.restriction_needs_premium())
            && who != PrivacyWho::Everybody
            && !self.privacy_premium()
    }

    /// Slice S3: change one rule's base choice, keeping the current
    /// exception rules (TGX `toggleGlobal`). A locked choice shows the
    /// Premium toast instead.
    pub(in crate::ui) fn set_privacy_target_who(
        &mut self,
        target: PrivacyEditorTarget,
        who: PrivacyWho,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.privacy_choice_locked(target, who) {
            self.voice_privacy_premium_toast(window, cx);
            return;
        }
        let PrivacyEditorTarget::Rule(key) = target else {
            return;
        };
        self.mutate_privacy_detail(key, |detail| detail.who = Some(who), cx);
    }

    /// Slice S3: one always/never exception row with its user count.
    pub(super) fn privacy_exception_row(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        count: usize,
    ) -> AnyElement {
        div()
            .id(format!("privacy-exceptions-{kind:?}"))
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::Q_PREFS_EXCEPTIONS_2,
                &[("label", (target.label())), ("label_2", (kind.label()))],
            ))
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .rounded_md()
            .child(div().text_sm().child(kind.label()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if count == 0 {
                        tr(keys::SETTINGS_NO_BLOCKED_USERS).to_string()
                    } else {
                        format!("{count}")
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.privacy.exceptions = Some((target, kind));
                this.privacy.exception_picker_open = false;
                cx.notify();
            }))
            .into_any_element()
    }

    /// Slice S3: "Hide read time" toggle (TGX
    /// `SettingsPrivacyKeyController` extra toggle under Last Seen).
    /// Checked = read dates hidden (`!show_read_date`).
    pub(super) fn read_date_toggle_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let hidden = !self
            .session()
            .and_then(|s| s.settings.read_date_show)
            .unwrap_or(true);
        let loading = self.session().is_some_and(|s| s.settings.read_date_loading);
        div()
            .id("privacy-hide-read-time")
            .role(gpui_kit::Role::Button)
            .aria_label(tr(keys::EDIT_LASTSEEN_HIDE_READ_TIME))
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .child(div().text_xs().child(if hidden { "☑" } else { "☐" }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::EDIT_LASTSEEN_HIDE_READ_TIME)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loading {
                                tr(keys::Q_PREFS_UPDATING)
                            } else {
                                tr(keys::Q_PREFS_DONT_SHARE_WHEN_YOUVE_READ)
                            }),
                    ),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_read_date(cx);
            }))
            .into_any_element()
    }

    /// Slice S3: flip `show_read_date`. Live: optimistic `set`; demo:
    /// flip the fixture.
    pub(super) fn toggle_read_date(&mut self, cx: &mut Context<Self>) {
        let next = !self
            .session()
            .and_then(|s| s.settings.read_date_show)
            .unwrap_or(true);
        if let Some(live) = self.live.as_mut() {
            let result = { live.driver.set_read_date_privacy(next) };
            if let Err(err) = result {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_READ_DATE,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.read_date_show = Some(next);
        }
        cx.notify();
    }

    /// Slice S3 + B13: the always/never exception list (tdesktop
    /// `PrivacyExceptionsBoxController`): the Premium users / Mini Apps
    /// rows where tdesktop offers them, then users and groups with Remove
    /// and Add pickers.
    pub(crate) fn privacy_exceptions_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.privacy.exceptions?;
        self.privacy_exceptions_overlay_body(cx)
    }

    /// `privacy_exceptions_overlay` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn privacy_exceptions_overlay_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (target, kind) = self.privacy.exceptions?;
        let session = self.session()?;
        let PrivacyEditorTarget::Rule(key) = target else {
            return None;
        };
        let detail = session
            .settings
            .privacy
            .get(&key)
            .and_then(|st| match st {
                PrivacyKeyState::Ready(d) => Some(d.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let (ids, chat_ids): (&[i64], &[i64]) = match kind {
            PrivacyExceptionKind::Always => (&detail.always, &detail.always_chats),
            PrivacyExceptionKind::Never => (&detail.never, &detail.never_chats),
        };
        let mut body = div().flex().flex_col().gap_1();
        if key.allows_premium_exception(kind == PrivacyExceptionKind::Always) {
            body = body.child(self.exception_flag_row(
                cx,
                target,
                kind,
                ExceptionFlag::Premium,
                tr(keys::EDIT_PRIVACY_PREMIUM),
                tr(keys::EDIT_PRIVACY_PREMIUM_STATUS),
                detail.allow_premium,
            ));
        }
        if key.allows_bots_exception() {
            let on = match kind {
                PrivacyExceptionKind::Always => detail.allow_bots,
                PrivacyExceptionKind::Never => detail.never_bots,
            };
            body = body.child(self.exception_flag_row(
                cx,
                target,
                kind,
                ExceptionFlag::Bots,
                tr(keys::EDIT_PRIVACY_MINIAPPS),
                tr(keys::EDIT_PRIVACY_MINIAPPS_STATUS),
                on,
            ));
        }
        if ids.is_empty() && chat_ids.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_NO_USERS_OR_GROUPS_ADDED)),
            );
        }
        for user_id in ids {
            let user_id = *user_id;
            let name = session
                .users
                .get(&user_id)
                .map(|u| u.display_name())
                .unwrap_or_else(|| {
                    tr_with(keys::Q_PREFS_USER, &[("user_id", &user_id.to_string())])
                });
            body = body.child(
                div()
                    .id(format!("privacy-exception-user-{user_id}"))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .child(div().text_sm().child(name))
                    .child(
                        Button::new(format!("privacy-exception-remove-{user_id}"))
                            .small()
                            .label(tr(keys::BOX_REMOVE))
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_exception(target, kind, user_id, false, cx);
                            })),
                    ),
            );
        }
        for chat_id in chat_ids {
            let chat_id = *chat_id;
            let title = session
                .chats
                .get(&chat_id)
                .map(|c| c.title.clone())
                .unwrap_or_else(|| {
                    tr_with(keys::Q_PREFS_GROUP, &[("chat_id", &chat_id.to_string())])
                });
            body = body.child(
                div()
                    .id(format!("privacy-exception-chat-{chat_id}"))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .child(div().text_sm().child(title))
                    .child(
                        Button::new(format!("privacy-exception-remove-chat-{chat_id}"))
                            .small()
                            .label(tr(keys::BOX_REMOVE))
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_exception_chat(target, kind, chat_id, false, cx);
                            })),
                    ),
            );
        }
        body = body.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("privacy-exception-add")
                        .small()
                        .label(tr(keys::Q_PREFS_ADD_USER))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.privacy.exception_picker_open = true;
                            this.privacy.extra.exception_picker_groups = false;
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.fetch_contacts();
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("privacy-exception-add-group")
                        .small()
                        .label(tr(keys::Q_PREFS_ADD_GROUP))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.privacy.exception_picker_open = true;
                            this.privacy.extra.exception_picker_groups = true;
                            cx.notify();
                        })),
                ),
        );
        if self.privacy.exception_picker_open {
            body = if self.privacy.extra.exception_picker_groups {
                body.child(self.exception_group_picker(cx, target, kind, &detail))
            } else {
                body.child(self.exception_picker(cx, target, kind, &detail))
            };
        }
        let title = if key == PrivacySettingKey::AllowUnpaidMessages {
            key.label()
        } else {
            kind.label()
        };
        Some(self.privacy_shell(cx, "exceptions", title, body.into_any_element()))
    }

    /// One Premium users / Mini Apps switch row of an exception list.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn exception_flag_row(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        flag: ExceptionFlag,
        label: &'static str,
        about: &'static str,
        on: bool,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(about),
                    ),
            )
            .child(
                Switch::new(format!("privacy-exception-flag-{flag:?}-{kind:?}"))
                    .checked(on)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                        this.edit_exception_flag(target, kind, flag, on, cx);
                    })),
            )
            .into_any_element()
    }

    /// Slice S3: add-exception contact picker — contacts not already in
    /// either exception list.
    pub(super) fn exception_picker(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        detail: &PrivacyRuleDetail,
    ) -> AnyElement {
        let session = self.session();
        let rows: Vec<ContactRow> = session.map(|s| s.contact_rows()).unwrap_or_default();
        let rows: Vec<&ContactRow> = rows
            .iter()
            .filter(|r| !detail.always.contains(&r.user_id) && !detail.never.contains(&r.user_id))
            .collect();
        let mut body = div().flex().flex_col().gap_1().mt_1();
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .px_1()
                .child(tr(keys::Q_PREFS_CHOOSE_A_CONTACT)),
        );
        if rows.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_NO_MORE_CONTACTS)),
            );
        }
        for row in rows {
            let user_id = row.user_id;
            body = body.child(
                div()
                    .id(format!("privacy-picker-user-{user_id}"))
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_PREFS_ADD_TO_PRIVACY_EXCEPTIONS_2,
                        &[("name", &row.name.to_string())],
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().child(row.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_exception(target, kind, user_id, true, cx);
                    })),
            );
        }
        body.into_any_element()
    }

    /// B13: add-exception group picker — the user's groups and
    /// supergroups (tdesktop's exception lists take chats whose members
    /// the rule then covers), not already in either list.
    pub(super) fn exception_group_picker(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        detail: &PrivacyRuleDetail,
    ) -> AnyElement {
        let groups: Vec<(i64, String)> = self
            .session()
            .map(|s| {
                s.ordered_chats()
                    .into_iter()
                    .filter(|chat| {
                        matches!(
                            chat.kind,
                            quill::telegram::envelope::ChatKind::BasicGroup { .. }
                                | quill::telegram::envelope::ChatKind::Supergroup {
                                    is_channel: false,
                                    ..
                                }
                        )
                    })
                    .filter(|chat| {
                        !detail.always_chats.contains(&chat.id.0)
                            && !detail.never_chats.contains(&chat.id.0)
                    })
                    .map(|chat| (chat.id.0, chat.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut body = div().flex().flex_col().gap_1().mt_1();
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .px_1()
                .child(tr(keys::Q_PREFS_CHOOSE_A_GROUP)),
        );
        if groups.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_NO_MORE_GROUPS)),
            );
        }
        for (chat_id, title) in groups {
            body = body.child(
                div()
                    .id(format!("privacy-picker-chat-{chat_id}"))
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_PREFS_ADD_TO_PRIVACY_EXCEPTIONS,
                        &[("title", &title.to_string())],
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().child(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_exception_chat(target, kind, chat_id, true, cx);
                    })),
            );
        }
        body.into_any_element()
    }

    /// Apply `edit` to the key's rule detail and save the result.
    /// Not Ready (Loading/Failed): ignored — an empty detail would
    /// recompose into a base-less rule list (first-match => deny-all).
    pub(super) fn mutate_privacy_detail(
        &mut self,
        key: PrivacySettingKey,
        edit: impl FnOnce(&mut PrivacyRuleDetail),
        cx: &mut Context<Self>,
    ) {
        let current = self.session().and_then(|s| {
            s.settings.privacy.get(&key).and_then(|st| match st {
                PrivacyKeyState::Ready(d) => Some(d.clone()),
                _ => None,
            })
        });
        let Some(mut detail) = current else {
            return;
        };
        edit(&mut detail);
        self.save_privacy_rule(key, detail, cx);
    }

    /// Slice S3: add or remove one user from an always/never exception
    /// list. Adding to one list removes the user from the other (a user
    /// can't meaningfully be in both under first-match evaluation).
    pub(super) fn edit_exception(
        &mut self,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        user_id: i64,
        add: bool,
        cx: &mut Context<Self>,
    ) {
        let PrivacyEditorTarget::Rule(key) = target else {
            return;
        };
        self.mutate_privacy_detail(
            key,
            |detail| {
                let (mine, other) = match kind {
                    PrivacyExceptionKind::Always => (&mut detail.always, &mut detail.never),
                    PrivacyExceptionKind::Never => (&mut detail.never, &mut detail.always),
                };
                if add {
                    other.retain(|id| *id != user_id);
                    if !mine.contains(&user_id) {
                        mine.push(user_id);
                    }
                } else {
                    mine.retain(|id| *id != user_id);
                }
            },
            cx,
        );
        if add {
            self.privacy.exception_picker_open = false;
        }
        cx.notify();
    }

    /// B13: add or remove one group (`...ChatMembers` rules) of an
    /// always/never list; the other list drops it, like users.
    pub(in crate::ui) fn edit_exception_chat(
        &mut self,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        chat_id: i64,
        add: bool,
        cx: &mut Context<Self>,
    ) {
        let PrivacyEditorTarget::Rule(key) = target else {
            return;
        };
        self.mutate_privacy_detail(
            key,
            |detail| {
                let (mine, other) = match kind {
                    PrivacyExceptionKind::Always => {
                        (&mut detail.always_chats, &mut detail.never_chats)
                    }
                    PrivacyExceptionKind::Never => {
                        (&mut detail.never_chats, &mut detail.always_chats)
                    }
                };
                if add {
                    other.retain(|id| *id != chat_id);
                    if !mine.contains(&chat_id) {
                        mine.push(chat_id);
                    }
                } else {
                    mine.retain(|id| *id != chat_id);
                }
            },
            cx,
        );
        if add {
            self.privacy.exception_picker_open = false;
        }
        cx.notify();
    }

    /// B13: flip the Premium users / Mini Apps row of an exception list.
    /// Choosing a row in one list clears it in the other (tdesktop
    /// `EditPrivacyBox::editExceptions`).
    pub(in crate::ui) fn edit_exception_flag(
        &mut self,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        flag: ExceptionFlag,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        let PrivacyEditorTarget::Rule(key) = target else {
            return;
        };
        self.mutate_privacy_detail(
            key,
            |detail| match (flag, kind) {
                (ExceptionFlag::Premium, _) => detail.allow_premium = on,
                (ExceptionFlag::Bots, PrivacyExceptionKind::Always) => {
                    detail.allow_bots = on;
                    if on {
                        detail.never_bots = false;
                    }
                }
                (ExceptionFlag::Bots, PrivacyExceptionKind::Never) => {
                    detail.never_bots = on;
                    if on {
                        detail.allow_bots = false;
                    }
                }
            },
            cx,
        );
    }
}

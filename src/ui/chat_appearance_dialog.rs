//! "Appearance" box of a channel or supergroup (tdesktop
//! `boxes/peers/edit_peer_color_box.cpp`, opened from the chat's edit box):
//! name color and reply icon (channels), profile color and profile icon,
//! and the emoji status. Parts the chat has too few boosts for show their
//! level ("Level N required") and Apply asks for boosts instead of sending,
//! like tdesktop's `CheckBoostLevel`. Rules live in `quill::chat_appearance`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::danger;
use super::dialogs::GroupSettingsView;
use super::pressable::{PressableDiv, action_row};
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chat_appearance::{
    AppearanceRules, ApplyOutcome, ChatAppearanceDraft, ChatAppearanceKind, ChatAppearanceTab,
    FeatureLevels, name_color_level, profile_color_level,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::peer_color::{name_swatches, profile_swatches};
use quill::state::RequestPurpose;
use quill::telegram::envelope::ChatKind;
use std::cell::RefCell;
use std::rc::Rc;

/// The open box: the chat, the tab and the draft the swatches edit.
pub(crate) struct ChatAppearanceDialog {
    pub(super) chat_id: ChatId,
    pub(super) tab: ChatAppearanceTab,
    pub(super) draft: ChatAppearanceDraft,
    /// Apply sent at least one request.
    pub(super) sent: bool,
    /// The last Apply was refused for too few boosts: `(required, have)`.
    pub(super) needs_boosts: Option<(i32, i32)>,
}

impl QuillApp {
    /// The kind of chat, when it has an appearance at all.
    fn chat_appearance_kind(&self, chat_id: ChatId) -> Option<ChatAppearanceKind> {
        match self.session()?.chats.get(&chat_id.0)?.kind {
            ChatKind::Supergroup {
                is_channel: true, ..
            } => Some(ChatAppearanceKind::Channel),
            ChatKind::Supergroup { .. } => Some(ChatAppearanceKind::Supergroup),
            _ => None,
        }
    }

    /// The chat's current look from the cached `chat` object and updates.
    fn chat_appearance_current(&self, chat_id: ChatId) -> Option<ChatAppearanceDraft> {
        let session = self.session()?;
        let accent = session
            .chats_state
            .chat_accents
            .get(&chat_id.0)
            .copied()
            .unwrap_or_default();
        let look = session
            .chats_state
            .chat_looks
            .get(&chat_id.0)
            .copied()
            .unwrap_or_default();
        Some(ChatAppearanceDraft {
            name_color_id: accent.accent_color_id,
            name_emoji_id: accent.background_custom_emoji_id,
            profile_color_id: look.profile_accent_color_id,
            profile_emoji_id: look.profile_background_custom_emoji_id,
            status_emoji_id: look.emoji_status_id,
            emoji_pack_id: session
                .chat_supergroup(chat_id)
                .and_then(|id| session.supergroup_full_info(id))
                .map_or(0, |info| info.custom_emoji_sticker_set_id),
        })
    }

    /// The chat's boost level, once `getChatBoostStatus` answered.
    fn chat_appearance_level(&self, chat_id: ChatId) -> Option<i32> {
        self.session()?
            .groups
            .chat_boost_status
            .get(&chat_id.0)
            .map(|(level, _)| *level)
    }

    fn chat_appearance_features(&self, chat_id: ChatId, kind: ChatAppearanceKind) -> FeatureLevels {
        FeatureLevels::from_features(
            self.session()
                .and_then(|s| s.groups.chat_boost_features.get(&chat_id.0)),
            kind,
        )
    }

    /// "Appearance" row of the chat's settings (tdesktop `Appearance`
    /// button in the edit box, shown to those who can edit the chat's info).
    pub(super) fn chat_appearance_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.chat_appearance_kind(chat_id)?;
        if !self.session()?.chat_can_change_info(chat_id) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        Some(
            action_row(
                "gl-appearance",
                Some(IconName::Palette),
                tr(keys::Q_MISC_APPEARANCE),
                false,
                cx,
            )
            .child(div().flex_1())
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .text_color(muted),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_chat_appearance_dialog(chat_id, cx);
            }))
            .into_any_element(),
        )
    }

    pub(super) fn open_chat_appearance_dialog(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let (Some(kind), Some(draft)) = (
            self.chat_appearance_kind(chat_id),
            self.chat_appearance_current(chat_id),
        ) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_chat_appearance(chat_id);
        }
        self.dialogs.chat_appearance_dialog = Some(ChatAppearanceDialog {
            chat_id,
            tab: match kind {
                ChatAppearanceKind::Channel => ChatAppearanceTab::Name,
                ChatAppearanceKind::Supergroup => ChatAppearanceTab::Profile,
            },
            draft,
            sent: false,
            needs_boosts: None,
        });
        cx.notify();
    }

    pub(super) fn close_chat_appearance_dialog(&mut self, cx: &mut Context<Self>) {
        if self.dialogs.chat_appearance_dialog.take().is_some()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.close_chat_appearance();
        }
        cx.notify();
    }

    pub(super) fn edit_chat_appearance(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut ChatAppearanceDraft, ChatAppearanceTab),
    ) {
        if let Some(dialog) = self.dialogs.chat_appearance_dialog.as_mut() {
            let tab = dialog.tab;
            edit(&mut dialog.draft, tab);
            dialog.sent = false;
            dialog.needs_boosts = None;
        }
        cx.notify();
    }

    /// Apply: boost level first, then one request per changed part.
    fn apply_chat_appearance(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, draft)) = self
            .dialogs
            .chat_appearance_dialog
            .as_ref()
            .map(|d| (d.chat_id, d.draft))
        else {
            return;
        };
        let (Some(kind), Some(current)) = (
            self.chat_appearance_kind(chat_id),
            self.chat_appearance_current(chat_id),
        ) else {
            return;
        };
        let level = self.chat_appearance_level(chat_id);
        let outcome = {
            let Some(session) = self.session() else {
                return;
            };
            let rules = AppearanceRules {
                kind,
                features: self.chat_appearance_features(chat_id, kind),
                name_palette: &session.chats_state.name_accent_colors,
                profile_palette: &session.chats_state.profile_accent_colors,
            };
            draft.apply_outcome(&current, level, &rules)
        };
        match outcome {
            ApplyOutcome::Unchanged => self.close_chat_appearance_dialog(cx),
            ApplyOutcome::LevelUnknown => {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.fetch_chat_boost_status(chat_id);
                }
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MISC_CHECKING_THE_BOOST_LEVEL_PRESS,
                )));
            }
            ApplyOutcome::NeedsBoosts { required, have } => {
                if let Some(dialog) = self.dialogs.chat_appearance_dialog.as_mut() {
                    dialog.needs_boosts = Some((required, have));
                }
            }
            ApplyOutcome::Send(plan) => {
                let Some(live) = self.live.as_mut() else {
                    self.connection.set_note(StatusNote::info(tr(
                        keys::Q_MISC_DEMO_MODE_APPEARANCE_NEEDS_A,
                    )));
                    cx.notify();
                    return;
                };
                let result = live.driver.apply_chat_appearance(chat_id, plan);
                self.connection.set_note(if result.is_err() {
                    StatusNote::error(tr(keys::Q_MISC_COULD_NOT_CHANGE_THE_APPEARANCE))
                } else {
                    StatusNote::silent(tr(keys::Q_MISC_APPEARANCE_UPDATE_REQUESTED))
                });
                if let Some(dialog) = self.dialogs.chat_appearance_dialog.as_mut() {
                    dialog.sent = result.is_ok();
                    dialog.needs_boosts = None;
                }
            }
        }
        cx.notify();
    }

    /// "Get boosts": leave the box and show the chat's Boosts page.
    fn chat_appearance_open_boosts(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self
            .dialogs
            .chat_appearance_dialog
            .as_ref()
            .map(|d| d.chat_id)
        else {
            return;
        };
        self.close_chat_appearance_dialog(cx);
        self.open_group_settings_view(chat_id, GroupSettingsView::Boosts, cx);
    }

    pub(super) fn build_chat_appearance_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ChatAppearance, |this, _, cx| {
                this.close_chat_appearance_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_MISC_APPEARANCE)));
            if let Some(live) = this.live.as_mut() {
                live.driver.download_background_emoji_icons();
                live.driver.download_chat_status_choices();
            }
            let Some(body) = this.chat_appearance_body(cx) else {
                return dialog.on_close(on_close);
            };
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("chat-appearance-close")
                        .label(tr(keys::CLOSE))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_chat_appearance_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::ChatAppearance, window, cx);
                        })),
                )
                .child(
                    Button::new("chat-appearance-apply")
                        .label(tr(keys::Q_MISC_APPLY))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_chat_appearance(cx);
                            this.close_kit_dialog_if_done(DialogKind::ChatAppearance, window, cx);
                        })),
                );
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// The thumbnail of a custom emoji known to the emoji panel, or its
    /// emoji character while the file is not local.
    fn chat_appearance_emoji_visual(&self, custom_emoji_id: i64, size: f32) -> AnyElement {
        let session = self.session();
        let roots = self.media_display_roots();
        let sticker = session.and_then(|s| {
            s.stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .chain(s.users_state.background_emoji_icons.iter())
                .find(|sticker| sticker.custom_emoji_id == Some(custom_emoji_id))
        });
        let Some(sticker) = sticker else {
            return div().into_any_element();
        };
        let path = sticker
            .display_file_id()
            .and_then(|id| session.and_then(|s| s.media.files.get(&id.0)))
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, &roots));
        match path {
            Some(path) => img(path)
                .w(px(size))
                .h(px(size))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .text_size(px(size * 0.7))
                .child(sticker.emoji.clone())
                .into_any_element(),
        }
    }

    /// A tile grid of "Off" plus `ids`, the chosen one marked.
    fn chat_appearance_emoji_grid(
        &self,
        grid_id: &'static str,
        ids: &[i64],
        selected: i64,
        on_pick: impl Fn(&mut QuillApp, i64, &mut Context<QuillApp>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tile = |on: bool| {
            let base = div()
                .size(px(40.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md();
            if on {
                base.bg(cx.theme().primary.opacity(0.25))
            } else {
                base
            }
        };
        let off = on_pick.clone();
        let mut grid = div()
            .id(grid_id)
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(160.))
            .overflow_y_scroll()
            .role(Role::List)
            .aria_label(tr(keys::Q_MISC_CHOICES))
            .child(
                tile(selected == 0)
                    .id((grid_id, 0usize))
                    .pressable(cx.theme())
                    .role(Role::Button)
                    .aria_label(tr(keys::Q_MISC_NONE))
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| off(this, 0, cx)))
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::SETTINGS_CHAT_AUTOMATION_OFF)),
                    ),
            );
        let mut seen = std::collections::HashSet::new();
        for (ix, id) in ids.iter().copied().enumerate() {
            if id <= 0 || !seen.insert(id) {
                continue;
            }
            let pick = on_pick.clone();
            grid = grid.child(
                tile(selected == id)
                    .id((grid_id, ix + 1))
                    .pressable(cx.theme())
                    .role(Role::Button)
                    .aria_label(tr(keys::Q_MISC_CHOICE))
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| pick(this, id, cx)))
                    .child(self.chat_appearance_emoji_visual(id, 28.)),
            );
        }
        grid.into_any_element()
    }

    /// "Level N required" when the chat is below `required`, nothing once
    /// it is there. With the level unknown the minimum shows plainly.
    pub(super) fn chat_appearance_lock_note(
        required: i32,
        level: Option<i32>,
        muted: Hsla,
    ) -> Option<Div> {
        (required > 0 && level.is_none_or(|have| have < required)).then(|| {
            div().text_xs().text_color(muted).child(tr_with(
                keys::Q_MISC_LEVEL_REQUIRED,
                &[("required", &required.to_string())],
            ))
        })
    }

    fn chat_appearance_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialogs.chat_appearance_dialog.as_ref()?;
        let (chat_id, tab, draft, sent, needs_boosts) = (
            dialog.chat_id,
            dialog.tab,
            dialog.draft,
            dialog.sent,
            dialog.needs_boosts,
        );
        let kind = self.chat_appearance_kind(chat_id)?;
        let current = self.chat_appearance_current(chat_id)?;
        let session = self.session()?;
        let title = session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        let level = self.chat_appearance_level(chat_id);
        let features = self.chat_appearance_features(chat_id, kind);
        let busy = [
            RequestPurpose::SetChatAccentColor,
            RequestPurpose::SetChatProfileAccentColor,
            RequestPurpose::SetChatEmojiStatus,
        ]
        .iter()
        .any(|purpose| session.requests.has_purpose_for_chat(*purpose, chat_id));
        let error = session.chats_state.appearance_error.clone();
        let dark = super::chat_theme::is_dark_palette();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let group_word = match kind {
            ChatAppearanceKind::Channel => "channel",
            ChatAppearanceKind::Supergroup => "group",
        };

        let name_palette = &session.chats_state.name_accent_colors;
        let profile_palette = &session.chats_state.profile_accent_colors;
        let name_rows = name_swatches(&session.chats_state.available_name_accent_ids, dark);
        let profile_rows = profile_swatches(
            &session.chats_state.available_accent_color_ids,
            profile_palette,
        );
        let name_color = super::chat_theme::peer_name_color(draft.name_color_id);
        let profile_rgb = profile_rows
            .iter()
            .find(|s| s.id == draft.profile_color_id)
            .map(|s| s.rgb);

        let preview = match tab {
            ChatAppearanceTab::Profile => {
                let band: Hsla = profile_rgb.map_or(theme.muted, rgb_to_hsla);
                let on_band = if profile_rgb.is_some() {
                    gpui_kit::white()
                } else {
                    theme.foreground
                };
                let mut pattern = div().flex().gap_2().opacity(0.45);
                if draft.profile_emoji_id != 0 {
                    for _ in 0..5 {
                        pattern = pattern
                            .child(self.chat_appearance_emoji_visual(draft.profile_emoji_id, 22.));
                    }
                }
                let mut name_line = div().flex().items_center().gap_1().text_color(on_band);
                name_line = name_line.child(div().font_semibold().child(title.clone()));
                if draft.status_emoji_id != 0 {
                    name_line = name_line
                        .child(self.chat_appearance_emoji_visual(draft.status_emoji_id, 18.));
                }
                div()
                    .id("chat-appearance-preview-profile")
                    .role(Role::Group)
                    .aria_label(tr(keys::Q_MISC_PROFILE_PREVIEW))
                    .w_full()
                    .h(px(112.))
                    .rounded_lg()
                    .bg(band)
                    .p_3()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(pattern)
                    .child(name_line)
                    .into_any_element()
            }
            ChatAppearanceTab::Name => {
                let mut quote = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .rounded_md()
                    .overflow_hidden()
                    .bg(name_color.opacity(0.14))
                    .child(
                        div()
                            .flex()
                            .child(div().w(px(3.)).h_full().flex_none().bg(name_color))
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(name_color)
                                            .child(title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .child(tr(keys::Q_MISC_REPLYING_TO_THIS_POST)),
                                    ),
                            ),
                    );
                if draft.name_emoji_id != 0 {
                    quote = quote.child(
                        div()
                            .px_2()
                            .opacity(0.6)
                            .child(self.chat_appearance_emoji_visual(draft.name_emoji_id, 24.)),
                    );
                }
                div()
                    .id("chat-appearance-preview-name")
                    .role(Role::Group)
                    .aria_label(tr(keys::Q_MISC_REPLY_PREVIEW))
                    .w_full()
                    .rounded_lg()
                    .bg(theme.muted)
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(name_color)
                            .child(title.clone()),
                    )
                    .child(quote)
                    .into_any_element()
            }
        };

        let tab_button = |id: &'static str, label: &'static str, which: ChatAppearanceTab| {
            Button::new(id)
                .label(label)
                .ghost()
                .selected(tab == which)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(dialog) = this.dialogs.chat_appearance_dialog.as_mut() {
                        dialog.tab = which;
                    }
                    cx.notify();
                }))
        };
        let mut tabs = div().flex().gap_2().child(tab_button(
            "chat-appearance-tab-profile",
            tr(keys::Q_MISC_PROFILE),
            ChatAppearanceTab::Profile,
        ));
        if kind == ChatAppearanceKind::Channel {
            tabs = tabs.child(tab_button(
                "chat-appearance-tab-name",
                tr(keys::Q_MISC_NAME),
                ChatAppearanceTab::Name,
            ));
        }

        let mut swatches = div().flex().flex_wrap().gap_2();
        match tab {
            ChatAppearanceTab::Name => {
                for swatch in &name_rows {
                    let id = swatch.id;
                    let required = name_color_level(id, current.name_color_id, name_palette);
                    let label = if required > 0 && level.is_none_or(|have| have < required) {
                        tr_with(keys::Q_MISC_LEVEL, &[("required", &required.to_string())])
                    } else {
                        String::new()
                    };
                    swatches = swatches.child(self.appearance_swatch(
                        format!("chat-appearance-name-{id}"),
                        swatch.rgb,
                        &label,
                        draft.name_color_id == id,
                        cx,
                        move |this, cx| this.edit_chat_appearance(cx, |d, _| d.name_color_id = id),
                    ));
                }
            }
            ChatAppearanceTab::Profile => {
                for swatch in &profile_rows {
                    let id = swatch.id;
                    let required =
                        profile_color_level(id, current.profile_color_id, kind, profile_palette);
                    let label = if required > 0 && level.is_none_or(|have| have < required) {
                        tr_with(keys::Q_MISC_LEVEL, &[("required", &required.to_string())])
                    } else {
                        String::new()
                    };
                    swatches = swatches.child(self.appearance_swatch(
                        format!("chat-appearance-profile-{id}"),
                        swatch.rgb,
                        &label,
                        draft.profile_color_id == id,
                        cx,
                        move |this, cx| {
                            this.edit_chat_appearance(cx, |d, _| d.profile_color_id = id)
                        },
                    ));
                }
            }
        }

        let (about, icon_title, icon_about, icon_required, selected_icon) = match tab {
            ChatAppearanceTab::Name => (
                tr_with(
                    keys::Q_MISC_THIS_COLOR_IS_USED_FOR,
                    &[("group_word", group_word)],
                ),
                tr(keys::SETTINGS_COLOR_EMOJI),
                tr_with(
                    keys::Q_MISC_MAKE_REPLIES_TO_THIS_STAND,
                    &[("group_word", group_word)],
                ),
                features.reply_icon,
                draft.name_emoji_id,
            ),
            ChatAppearanceTab::Profile => (
                tr_with(
                    keys::Q_MISC_THIS_COLOR_IS_USED_FOR_2,
                    &[("group_word", group_word)],
                ),
                tr(keys::SETTINGS_COLOR_PROFILE_EMOJI),
                tr(keys::Q_MISC_CHOOSE_AN_ICON_TO_REPEAT).to_string(),
                features.profile_icon,
                draft.profile_emoji_id,
            ),
        };
        let icon_ids: Vec<i64> = session
            .users_state
            .background_emoji_icons
            .iter()
            .filter_map(|sticker| sticker.custom_emoji_id)
            .collect();
        let icons_empty = icon_ids.is_empty();
        let icon_grid = self.chat_appearance_emoji_grid(
            "chat-appearance-icons",
            &icon_ids,
            selected_icon,
            |this, id, cx| {
                this.edit_chat_appearance(cx, |d, tab| match tab {
                    ChatAppearanceTab::Name => d.name_emoji_id = id,
                    ChatAppearanceTab::Profile => d.profile_emoji_id = id,
                });
            },
            cx,
        );

        let mut panel = div()
            .id("chat-appearance-panel")
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(match level {
                Some(level) => tr_with(
                    keys::Q_MISC_THIS_IS_AT_BOOST_LEVEL,
                    &[("group_word", group_word), ("level", &level.to_string())],
                ),
                None => tr_with(
                    keys::Q_MISC_CHECKING_THIS_S_BOOST_LEVEL,
                    &[("group_word", group_word)],
                ),
            }))
            .child(tabs)
            .child(preview)
            .child(swatches);
        if tab == ChatAppearanceTab::Profile && draft.profile_color_id != -1 {
            panel = panel.child(
                div().flex().child(
                    Button::new("chat-appearance-profile-reset")
                        .label(tr(keys::Q_MISC_RESET_PROFILE_COLOR))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_chat_appearance(cx, |d, _| {
                                d.profile_color_id = -1;
                                d.profile_emoji_id = 0;
                            });
                        })),
                ),
            );
        }
        panel = panel
            .child(div().text_xs().text_color(muted).child(about))
            .child(div().font_semibold().child(icon_title))
            .children(Self::chat_appearance_lock_note(icon_required, level, muted))
            .child(icon_grid)
            .when(icons_empty, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MISC_LOADING_ICONS)),
                )
            })
            .child(div().text_xs().text_color(muted).child(icon_about));

        // Emoji status of the chat (both tabs).
        let status_ids = session.stickers.emoji.chat_default_status_ids.clone();
        let status_empty = status_ids.is_empty();
        let status_grid = self.chat_appearance_emoji_grid(
            "chat-appearance-status",
            &status_ids,
            draft.status_emoji_id,
            |this, id, cx| this.edit_chat_appearance(cx, |d, _| d.status_emoji_id = id),
            cx,
        );
        panel = panel
            .child(div().font_semibold().child(match kind {
                ChatAppearanceKind::Channel => tr(keys::Q_MISC_CHANNEL_EMOJI_STATUS),
                ChatAppearanceKind::Supergroup => tr(keys::Q_MISC_GROUP_EMOJI_STATUS),
            }))
            .children(Self::chat_appearance_lock_note(
                features.status,
                level,
                muted,
            ))
            .child(status_grid)
            .when(status_empty, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MISC_LOADING_STATUSES)),
                )
            })
            .child(div().text_xs().text_color(muted).child(tr_with(
                keys::Q_MISC_THE_EMOJI_STATUS_IS_SHOWN,
                &[("group_word", group_word)],
            )));

        if kind == ChatAppearanceKind::Supergroup {
            panel = panel.child(self.chat_appearance_pack_section(
                features,
                level,
                draft.emoji_pack_id,
                cx,
            ));
        }
        panel = panel.child(self.chat_appearance_wallpaper_section(chat_id, kind, level, cx));

        if let Some((required, have)) = needs_boosts {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_sm().text_color(danger()).child(tr_with(
                        keys::Q_LEFTOVERS_LEVEL_REQUIRED,
                        &[
                            ("required", &required.to_string()),
                            ("group_word", group_word),
                            ("have", &have.to_string()),
                        ],
                    )))
                    .child(
                        div().flex().child(
                            Button::new("chat-appearance-boosts")
                                .label(tr(keys::Q_MISC_GET_BOOSTS))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.chat_appearance_open_boosts(cx);
                                })),
                        ),
                    ),
            );
        }
        if busy {
            panel = panel.child(div().text_sm().child(tr(keys::Q_MISC_APPLYING)));
        } else if let Some(error) = error {
            panel = panel.child(div().text_sm().text_color(danger()).child(error));
        } else if sent {
            panel = panel.child(div().text_sm().child(tr(keys::Q_MISC_APPEARANCE_UPDATED)));
        }
        Some(panel.into_any_element())
    }
}

fn rgb_to_hsla(color: u32) -> Hsla {
    rgb(color).into()
}

crate::ui::shell::register_dialogs! {
    /// Name color, profile color and emoji status of a channel or group.
    ChatAppearance => DialogSpec::new(
        4890,
        |app| app.dialogs.chat_appearance_dialog.is_some(),
        QuillApp::build_chat_appearance_dialog,
    ),
}

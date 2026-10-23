//! "Your color" box (tdesktop `boxes/peers/edit_peer_color_box.cpp`): the
//! account's name color, reply icon, profile color and profile icon, with a
//! live preview. Applying needs Telegram Premium; without it the button
//! opens Premium and sends nothing, like tdesktop's `ShowPremiumPreview`.
//! The Name tab also lists gifts whose colors can be worn
//! (`getReceivedGifts` with `exclude_without_colors`), applied with
//! `setUpgradedGiftColors`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::peer_color::{
    ApplyOutcome, PeerColorDraft, PeerColorTab, name_swatches, profile_swatches,
};
use quill::state::RequestPurpose;
use std::cell::RefCell;
use std::rc::Rc;

/// The open box: the tab and the draft the swatches edit.
pub(crate) struct PeerColorDialog {
    pub(super) tab: PeerColorTab,
    pub(super) draft: PeerColorDraft,
    /// Apply sent at least one request; the box then reports the result.
    pub(super) sent: bool,
}

impl QuillApp {
    /// The account's current colors from its user record.
    fn my_peer_colors(&self) -> Option<PeerColorDraft> {
        let session = self.session()?;
        let user = session.user(session.my_user_id?)?;
        Some(PeerColorDraft {
            name_color_id: user.accent_color_id,
            name_emoji_id: user.background_custom_emoji_id,
            gift_colors_id: user.upgraded_gift_colors_id,
            profile_color_id: user.profile_accent_color_id,
            profile_emoji_id: user.profile_background_custom_emoji_id,
        })
    }

    /// Appearance settings entry (tdesktop `settings_chat.cpp` "Your color").
    pub(in crate::ui) fn appearance_peer_color_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_section(
            cx,
            tr(keys::Q_MISC_YOUR_COLOR),
            tr(keys::Q_MISC_NAME_COLOR_PROFILE_COLOR_AND),
            Button::new("appearance-your-color")
                .label(tr(keys::Q_MISC_YOUR_COLOR_2))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_peer_color_dialog(PeerColorTab::Name, cx);
                }))
                .into_any_element(),
        )
    }

    pub(super) fn open_peer_color_dialog(&mut self, tab: PeerColorTab, cx: &mut Context<Self>) {
        let Some(draft) = self.my_peer_colors() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_MISC_COLORS_NEED_A_SIGNED_IN)));
            cx.notify();
            return;
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.users_state.profile_edit_error = None;
            let _ = live.driver.load_background_emoji_icons();
            let _ = live.driver.load_wearable_gifts();
        }
        self.dialogs.peer_color_dialog = Some(PeerColorDialog {
            tab,
            draft,
            sent: false,
        });
        cx.notify();
    }

    pub(super) fn close_peer_color_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.peer_color_dialog = None;
        cx.notify();
    }

    fn edit_peer_color(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut PeerColorDraft, PeerColorTab),
    ) {
        if let Some(dialog) = self.dialogs.peer_color_dialog.as_mut() {
            let tab = dialog.tab;
            edit(&mut dialog.draft, tab);
            dialog.sent = false;
        }
        cx.notify();
    }

    /// Apply: Premium gate first, then one request per changed section.
    fn apply_peer_color(&mut self, cx: &mut Context<Self>) {
        let (Some(dialog), Some(current)) = (
            self.dialogs.peer_color_dialog.as_ref(),
            self.my_peer_colors(),
        ) else {
            return;
        };
        let draft = dialog.draft;
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        match draft.apply_outcome(&current, premium) {
            ApplyOutcome::NeedsPremium => {
                self.open_premium(cx);
            }
            ApplyOutcome::Unchanged => self.close_peer_color_dialog(cx),
            ApplyOutcome::Send { name, profile } => {
                let Some(live) = self.live.as_mut() else {
                    self.connection
                        .set_note(StatusNote::info(tr(keys::Q_MISC_DEMO_MODE_COLORS_NEED_A)));
                    cx.notify();
                    return;
                };
                live.driver.session.users_state.profile_edit_error = None;
                let mut failed = false;
                if name {
                    failed |= if draft.gift_colors_id != 0 {
                        live.driver.set_upgraded_gift_colors(draft.gift_colors_id)
                    } else {
                        live.driver
                            .set_accent_color(draft.name_color_id, draft.name_emoji_id)
                    }
                    .is_err();
                }
                if profile {
                    failed |= live
                        .driver
                        .set_profile_colors(draft.profile_color_id, draft.profile_emoji_id)
                        .is_err();
                }
                self.connection.set_note(if failed {
                    StatusNote::error(tr(keys::Q_MISC_COULD_NOT_CHANGE_THE_COLORS))
                } else {
                    StatusNote::silent(tr(keys::Q_MISC_STYLE_UPDATE_REQUESTED))
                });
                if let Some(dialog) = self.dialogs.peer_color_dialog.as_mut() {
                    dialog.sent = !failed;
                }
                cx.notify();
            }
        }
    }

    pub(super) fn build_peer_color_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PeerColor, |this, _, cx| {
                this.close_peer_color_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::SETTINGS_COLOR_TITLE,
                )));
            if let Some(live) = this.live.as_mut() {
                live.driver.download_background_emoji_icons();
                live.driver.download_wearable_gift_stickers();
            }
            let Some(body) = this.peer_color_body(cx) else {
                return dialog.on_close(on_close);
            };
            let premium = this.session().is_some_and(|s| s.my_is_premium());
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("peer-color-close")
                        .label(tr(keys::CLOSE))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_peer_color_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::PeerColor, window, cx);
                        })),
                )
                .child(
                    Button::new("peer-color-apply")
                        .label(if premium {
                            tr(keys::SETTINGS_COLOR_APPLY)
                        } else {
                            tr(keys::Q_MISC_APPLY_STYLE_PREMIUM)
                        })
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_peer_color(cx);
                            this.close_kit_dialog_if_done(DialogKind::PeerColor, window, cx);
                        })),
                );
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
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

    /// One reply-icon tile: the custom emoji's thumbnail, or its emoji.
    fn peer_color_icon_visual(&self, custom_emoji_id: i64, size: f32) -> AnyElement {
        let session = self.session();
        let roots = self.media_display_roots();
        let sticker = session.and_then(|s| {
            s.users_state
                .background_emoji_icons
                .iter()
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

    /// The icon chooser shared by both tabs: Off plus the default icons.
    fn peer_color_icon_grid(&self, selected: i64, cx: &mut Context<Self>) -> AnyElement {
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
        let mut grid = div()
            .id("peer-color-icon-grid")
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(160.))
            .overflow_y_scroll()
            .role(Role::List)
            .aria_label(tr(keys::Q_MISC_ICONS))
            .child(
                tile(selected == 0)
                    .id("peer-color-icon-off")
                    .pressable(cx.theme())
                    .role(Role::Button)
                    .aria_label(tr(keys::Q_MISC_NO_ICON))
                    .tab_index(0)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.edit_peer_color(cx, |draft, tab| match tab {
                            PeerColorTab::Name => draft.name_emoji_id = 0,
                            PeerColorTab::Profile => draft.profile_emoji_id = 0,
                        });
                    }))
                    .child(div().text_sm().child(tr(keys::SETTINGS_COLOR_EMOJI_OFF))),
            );
        let icons: Vec<(usize, i64, String)> = self
            .session()
            .map(|s| {
                s.users_state
                    .background_emoji_icons
                    .iter()
                    .enumerate()
                    .filter_map(|(ix, sticker)| {
                        Some((ix, sticker.custom_emoji_id?, sticker.emoji.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (ix, id, emoji) in icons {
            grid = grid.child(
                tile(selected == id)
                    .id(("peer-color-icon", ix))
                    .pressable(cx.theme())
                    .role(Role::Button)
                    .aria_label(if emoji.is_empty() {
                        tr(keys::Q_MISC_ICON).to_string()
                    } else {
                        tr_with(keys::Q_MISC_ICON_2, &[("emoji", &emoji.to_string())])
                    })
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_peer_color(cx, |draft, tab| match tab {
                            PeerColorTab::Name => draft.name_emoji_id = id,
                            PeerColorTab::Profile => draft.profile_emoji_id = id,
                        });
                    }))
                    .child(self.peer_color_icon_visual(id, 28.)),
            );
        }
        grid.into_any_element()
    }

    fn peer_color_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialogs.peer_color_dialog.as_ref()?;
        let tab = dialog.tab;
        let draft = dialog.draft;
        let sent = dialog.sent;
        let session = self.session();
        let my_name = session
            .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
            .map(|u| u.display_name())
            .unwrap_or_default();
        let premium = session.is_some_and(|s| s.my_is_premium());
        let busy = session.is_some_and(|s| {
            s.requests.has_purpose(RequestPurpose::SetAccentColor)
                || s.requests
                    .has_purpose(RequestPurpose::SetProfileAccentColor)
        });
        let error = session.and_then(|s| s.users_state.profile_edit_error.clone());
        let icons_failed = session.is_some_and(|s| s.users_state.background_emoji_icons_failed);
        let icons_empty = session.is_some_and(|s| s.users_state.background_emoji_icons.is_empty());
        let dark = super::chat_theme::is_dark_palette();
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let name_swatch_rows = name_swatches(
            &session
                .map(|s| s.chats_state.available_name_accent_ids.clone())
                .unwrap_or_default(),
            dark,
        );
        let (profile_ids, profile_palette) = session
            .map(|s| {
                (
                    s.chats_state.available_accent_color_ids.clone(),
                    s.chats_state.profile_accent_colors.clone(),
                )
            })
            .unwrap_or_default();
        let profile_swatch_rows = profile_swatches(&profile_ids, &profile_palette);

        let worn_gift = (draft.gift_colors_id != 0)
            .then(|| {
                session.and_then(|s| {
                    s.users_state
                        .wearable_gifts
                        .iter()
                        .filter_map(|gift| gift.gift.colors.as_ref())
                        .find(|colors| colors.id == draft.gift_colors_id)
                })
            })
            .flatten();
        let name_color: Hsla = match worn_gift {
            Some(colors) => rgb_to_hsla(colors.accent(dark)),
            None => super::chat_theme::peer_name_color(draft.name_color_id),
        };
        let profile_rgb = profile_swatch_rows
            .iter()
            .find(|s| s.id == draft.profile_color_id)
            .map(|s| s.rgb);

        // Preview: a profile header on the Profile tab, a reply on Name.
        let preview = match tab {
            PeerColorTab::Profile => {
                let band: Hsla = profile_rgb.map(rgb_to_hsla).unwrap_or(theme.muted);
                let on_band = if profile_rgb.is_some() {
                    gpui_kit::white()
                } else {
                    theme.foreground
                };
                let mut pattern = div().flex().gap_2().opacity(0.45);
                if draft.profile_emoji_id != 0 {
                    for _ in 0..5 {
                        pattern =
                            pattern.child(self.peer_color_icon_visual(draft.profile_emoji_id, 22.));
                    }
                }
                div()
                    .id("peer-color-preview-profile")
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
                    .child(
                        div()
                            .text_color(on_band)
                            .font_semibold()
                            .child(my_name.clone()),
                    )
                    .into_any_element()
            }
            PeerColorTab::Name => {
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
                                            .child(my_name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .child(tr(keys::Q_MISC_REPLYING_TO_THIS_MESSAGE)),
                                    ),
                            ),
                    );
                if draft.name_emoji_id != 0 {
                    quote = quote.child(
                        div()
                            .px_2()
                            .opacity(0.6)
                            .child(self.peer_color_icon_visual(draft.name_emoji_id, 24.)),
                    );
                }
                div()
                    .id("peer-color-preview-name")
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
                            .child(my_name.clone()),
                    )
                    .child(quote)
                    .child(div().child(tr(keys::Q_MISC_LOOKS_GOOD)))
                    .into_any_element()
            }
        };

        let tab_button = |id: &'static str, label: &'static str, which: PeerColorTab| {
            Button::new(id)
                .label(label)
                .ghost()
                .selected(tab == which)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(dialog) = this.dialogs.peer_color_dialog.as_mut() {
                        dialog.tab = which;
                    }
                    cx.notify();
                }))
        };
        let tabs = div()
            .flex()
            .gap_2()
            .child(tab_button(
                "peer-color-tab-profile",
                tr(keys::SETTINGS_COLOR_TAB_PROFILE),
                PeerColorTab::Profile,
            ))
            .child(tab_button(
                "peer-color-tab-name",
                tr(keys::SETTINGS_COLOR_TAB_NAME),
                PeerColorTab::Name,
            ));

        let mut swatches = div().flex().flex_wrap().gap_2();
        match tab {
            PeerColorTab::Name => {
                for swatch in &name_swatch_rows {
                    let id = swatch.id;
                    swatches = swatches.child(self.appearance_swatch(
                        format!("peer-color-name-{id}"),
                        swatch.rgb,
                        "",
                        draft.gift_colors_id == 0 && draft.name_color_id == id,
                        cx,
                        move |this, cx| {
                            this.edit_peer_color(cx, |d, _| {
                                d.name_color_id = id;
                                d.gift_colors_id = 0;
                            })
                        },
                    ));
                }
            }
            PeerColorTab::Profile => {
                for swatch in &profile_swatch_rows {
                    let id = swatch.id;
                    swatches = swatches.child(self.appearance_swatch(
                        format!("peer-color-profile-{id}"),
                        swatch.rgb,
                        "",
                        draft.profile_color_id == id,
                        cx,
                        move |this, cx| this.edit_peer_color(cx, |d, _| d.profile_color_id = id),
                    ));
                }
            }
        }

        let (about, icon_title, icon_about, selected_icon) = match tab {
            PeerColorTab::Name => (
                tr(keys::SETTINGS_COLOR_ABOUT),
                tr(keys::SETTINGS_COLOR_EMOJI),
                tr(keys::SETTINGS_COLOR_EMOJI_ABOUT),
                draft.name_emoji_id,
            ),
            PeerColorTab::Profile => (
                tr(keys::Q_MISC_THIS_COLOR_WILL_BE_USED),
                tr(keys::SETTINGS_COLOR_PROFILE_EMOJI),
                tr(keys::Q_MISC_CHOOSE_AN_ICON_TO_REPEAT_2),
                draft.profile_emoji_id,
            ),
        };

        let status = if busy {
            Some((tr(keys::Q_MISC_APPLYING_STYLE).to_string(), false))
        } else if let Some(error) = error {
            Some((error, true))
        } else if sent {
            Some((tr(keys::Q_MISC_STYLE_UPDATED).to_string(), false))
        } else {
            None
        };

        let mut panel = div()
            .id("peer-color-panel")
            .flex()
            .flex_col()
            .gap_3()
            .child(tabs)
            .child(preview)
            .child(swatches);
        if tab == PeerColorTab::Profile && draft.profile_color_id != -1 {
            panel = panel.child(
                div().flex().child(
                    Button::new("peer-color-profile-reset")
                        .label(tr(keys::SETTINGS_COLOR_RESET))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_peer_color(cx, |d, _| {
                                d.profile_color_id = -1;
                                d.profile_emoji_id = 0;
                            });
                        })),
                ),
            );
        }
        panel = panel.child(div().text_xs().text_color(muted).child(about));
        if tab == PeerColorTab::Name {
            panel = panel.child(self.peer_color_gift_row(draft.gift_colors_id, cx));
        }
        // A worn gift brings its own icon, so the picker hides.
        if tab == PeerColorTab::Name && draft.gift_colors_id != 0 {
            return Some(self.peer_color_finish(panel, premium, status, muted));
        }
        panel = panel
            .child(div().font_semibold().child(icon_title))
            .child(self.peer_color_icon_grid(selected_icon, cx))
            .when(icons_empty && !icons_failed, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MISC_LOADING_ICONS)),
                )
            })
            .when(icons_failed, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MISC_COULD_NOT_LOAD_ICONS_REOPEN)),
                )
            })
            .child(div().text_xs().text_color(muted).child(icon_about));
        Some(self.peer_color_finish(panel, premium, status, muted))
    }

    /// The closing notes shared by every layout of the body.
    fn peer_color_finish(
        &self,
        mut panel: Stateful<Div>,
        premium: bool,
        status: Option<(String, bool)>,
        muted: Hsla,
    ) -> AnyElement {
        if !premium {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MISC_CHANGING_COLORS_NEEDS_TELEGRAM_PREMIUM)),
            );
        }
        if let Some((text, is_error)) = status {
            panel = panel.child(
                div()
                    .text_sm()
                    .when(is_error, |d| d.text_color(danger()))
                    .child(text),
            );
        }
        panel.into_any_element()
    }

    /// Collectible gifts whose colors can be worn: one tile each.
    fn peer_color_gift_row(&self, worn: i64, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let roots = self.media_display_roots();
        let mut row = div()
            .id("peer-color-gifts")
            .flex()
            .flex_wrap()
            .gap_2()
            .role(Role::List)
            .aria_label(tr(keys::Q_MISC_COLLECTIBLE_GIFTS));
        let mut count = 0;
        for (ix, gift) in session.users_state.wearable_gifts.iter().enumerate() {
            let Some(colors) = gift.gift.colors.clone() else {
                continue;
            };
            count += 1;
            let selected = worn == colors.id;
            let backdrop = match gift.gift.backdrop {
                Some((center, edge)) => div().bg(linear_gradient(
                    180.,
                    linear_color_stop(rgb(center), 0.),
                    linear_color_stop(rgb(edge), 1.),
                )),
                None => div().bg(cx.theme().muted),
            };
            row = row.child(
                backdrop
                    .id(("peer-color-gift", ix))
                    .w(px(110.))
                    .h(px(92.))
                    .rounded_lg()
                    .border_2()
                    .border_color(if selected {
                        cx.theme().primary
                    } else {
                        Hsla::from(rgba(0x0000_0000))
                    })
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(tr_with(
                        keys::Q_LEFTOVERS_WEAR_ITEM,
                        &[
                            ("name", &gift.headline().to_string()),
                            (
                                "selected",
                                if selected {
                                    tr(keys::Q_MISC_SELECTED)
                                } else {
                                    ""
                                },
                            ),
                        ],
                    ))
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let colors = colors.clone();
                        this.edit_peer_color(cx, move |d, _| {
                            d.gift_colors_id = colors.id;
                            d.name_emoji_id = colors.model_emoji_id;
                        });
                    }))
                    .child(super::premium_ui::sticker_tile(
                        ("peer-color-gift-sticker", ix as u64),
                        gift.gift.sticker.as_ref(),
                        &session.media.files,
                        &roots,
                        52.,
                        "\u{1F381}",
                    ))
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(gpui_kit::white())
                            .child(gift.headline()),
                    ),
            );
        }
        if count == 0 {
            return div().into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .font_semibold()
                    .child(tr(keys::Q_MISC_WEAR_A_COLLECTIBLE)),
            )
            .child(row)
            .into_any_element()
    }
}

fn rgb_to_hsla(color: u32) -> Hsla {
    rgb(color).into()
}

crate::ui::shell::register_dialogs! {
    PeerColor => DialogSpec::new(
        5950,
        |app| app.dialogs.peer_color_dialog.is_some(),
        QuillApp::build_peer_color_dialog,
    ),
}

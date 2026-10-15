//! Custom emoji pack settings, preview and confirmed removal.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::emoji::EmojiSetTab;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::state::RequestPurpose;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    pub(super) fn open_emoji_sets(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_emoji_sets().is_err() {
                self.connection.status_note = tr(keys::Q_MEDIA_COULD_NOT_LOAD_EMOJI_PACKS).into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.emoji.open = true;
        }
        cx.notify();
    }
    fn close_emoji_sets(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.stickers.emoji.open = false;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.emoji.open = false;
        }
        cx.notify();
    }
    fn select_emoji_sets_tab(&mut self, tab: EmojiSetTab, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.select_emoji_set_tab(tab).is_err() {
                self.connection.status_note = tr(keys::Q_MEDIA_COULD_NOT_LOAD_EMOJI_PACKS).into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.emoji.tab = tab;
        }
        cx.notify();
    }
    fn preview_emoji_set(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.preview_emoji_pack(id).is_err()
        {
            self.connection.status_note = tr(keys::Q_MEDIA_COULD_NOT_LOAD_EMOJI_PACK).into();
        }
        cx.notify();
    }
    fn install_emoji_set(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.set_emoji_pack_installed(id, true) {
                Ok(Some(_)) => tr(keys::Q_MEDIA_INSTALLING_EMOJI_PACK).into(),
                Ok(None) => tr(keys::Q_MEDIA_AN_EMOJI_PACK_IS_ALREADY).into(),
                Err(_) => tr(keys::Q_MEDIA_COULD_NOT_INSTALL_EMOJI_PACK).into(),
            };
        }
        cx.notify();
    }
    fn search_emoji_sets(&mut self, cx: &mut Context<Self>) {
        let query = self
            .pickers
            .emoji_set_search_input
            .read(cx)
            .value()
            .to_string();
        if let Some(live) = self.live.as_mut()
            && live.driver.search_emoji_packs(&query).is_err()
        {
            self.connection.status_note = tr(keys::Q_MEDIA_COULD_NOT_SEARCH_EMOJI_PACKS).into();
        }
        cx.notify();
    }
    fn change_emoji_status(&mut self, id: Option<i64>, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let duration = live.driver.session.stickers.emoji.status_duration_secs;
            self.connection.status_note = match live.driver.change_emoji_status(id, duration) {
                Ok(Some(_)) => tr(keys::Q_MEDIA_UPDATING_EMOJI_STATUS).into(),
                Ok(None) => tr(keys::Q_MEDIA_AN_EMOJI_STATUS_IS_ALREADY).into(),
                Err(_) => tr(keys::Q_MEDIA_COULD_NOT_UPDATE_EMOJI_STATUS).into(),
            };
        }
        cx.notify();
    }

    fn emoji_status_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let panel = session
            .map(|s| s.stickers.emoji.clone())
            .unwrap_or_default();
        let premium = session.is_some_and(|s| {
            s.my_user_id
                .and_then(|id| s.user(id))
                .is_some_and(|u| u.is_premium)
        });
        let busy = session.is_some_and(|s| {
            s.requests.has_purpose(RequestPurpose::SetEmojiStatus)
                || s.requests
                    .has_purpose(RequestPurpose::ClearRecentEmojiStatuses)
        });
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .font_semibold()
                    .child(tr(keys::PROFILE_BOT_EMOJI_STATUS_ACCESS)),
            )
            .child(
                Button::new("load-emoji-statuses")
                    .label(tr(keys::Q_MEDIA_LOAD_OR_REFRESH_STATUSES))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut()
                            && live.driver.load_emoji_status_choices().is_err()
                        {
                            this.connection.status_note =
                                tr(keys::Q_MEDIA_COULD_NOT_LOAD_EMOJI_STATUSES).into();
                        }
                        cx.notify();
                    })),
            );
        if !premium {
            section = section.child(
                div()
                    .text_xs()
                    .child(tr(keys::Q_MEDIA_SETTING_AN_EMOJI_STATUS_REQUIRES)),
            );
        }
        if !panel.status_open {
            return section.into_any_element();
        }
        let mut durations = div().flex().flex_wrap().gap_2();
        for (title, secs) in [
            (tr(keys::Q_MEDIA_FOREVER), 0),
            (tr(keys::Q_MEDIA_1_HOUR), 3600),
            (tr(keys::Q_MEDIA_2_HOURS), 7200),
            (tr(keys::Q_MEDIA_8_HOURS), 28800),
            (tr(keys::Q_MEDIA_2_DAYS), 172800),
        ] {
            durations = durations.child(
                Button::new(format!("status-duration-{secs}"))
                    .label(title)
                    .selected(panel.status_duration_secs == secs)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            live.driver.session.stickers.emoji.status_duration_secs = secs;
                        }
                        cx.notify();
                    })),
            );
        }
        section = section
            .child(durations)
            .child(
                Textarea::new(&self.pickers.emoji_status_hours_input)
                    .aria_label(tr(keys::Q_MEDIA_EMOJI_STATUS_DURATION_IN_HOURS)),
            )
            .child(
                Button::new("status-custom-duration")
                    .label(tr(keys::Q_MEDIA_USE_CUSTOM_DURATION))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let hours = this
                            .pickers
                            .emoji_status_hours_input
                            .read(cx)
                            .value()
                            .trim()
                            .parse::<u32>()
                            .ok();
                        let secs = hours
                            .filter(|h| *h > 0)
                            .and_then(|h| h.checked_mul(3600))
                            .and_then(|s| i32::try_from(s).ok())
                            .filter(|s| {
                                quill::state::unix_ms_now() / 1000 + *s as u64 <= i32::MAX as u64
                            });
                        if let Some(secs) = secs {
                            if let Some(id) = this.chat_list.status_other_for.take() {
                                this.change_emoji_status_for(id, secs, cx);
                            } else if let Some(live) = this.live.as_mut() {
                                live.driver.session.stickers.emoji.status_duration_secs = secs;
                            }
                        } else {
                            this.connection.status_note =
                                tr(keys::Q_MEDIA_ENTER_POSITIVE_WHOLE_HOURS_WITHIN).into();
                        }
                        cx.notify();
                    })),
            )
            .child(div().text_xs().child(if panel.status_duration_secs == 0 {
                tr(keys::Q_MEDIA_SELECTED_DURATION_FOREVER).into()
            } else {
                tr_with(
                    keys::Q_MEDIA_SELECTED_DURATION_HOURS,
                    &[(
                        "status_duration_secs_3600",
                        &(panel.status_duration_secs / 3600).to_string(),
                    )],
                )
            }));
        for (title, ids) in [
            (
                tr(keys::Q_MEDIA_RECENT),
                panel
                    .recent_statuses
                    .iter()
                    .map(|s| s.custom_emoji_id)
                    .collect::<Vec<_>>(),
            ),
            (
                tr(keys::STICKERS_FEATURED_TAB),
                panel.themed_status_ids.clone(),
            ),
            (tr(keys::Q_MEDIA_DEFAULT), panel.default_status_ids.clone()),
        ] {
            let mut choices = div().flex().flex_wrap().gap_2();
            let mut seen = std::collections::HashSet::new();
            for id in ids {
                if id <= 0 || !seen.insert(id) {
                    continue;
                }
                let Some(sticker) = panel
                    .custom_emoji_stickers
                    .iter()
                    .find(|s| s.custom_emoji_id == Some(id))
                else {
                    continue;
                };
                let display = sticker.thumb_file_id.or_else(|| {
                    (sticker.format == quill::telegram::envelope::StickerFormat::Webp)
                        .then_some(sticker.file_id)
                });
                let path = display
                    .and_then(|id| {
                        session
                            .and_then(|s| s.media.files.get(&id.0))
                            .and_then(|f| f.usable_path())
                    })
                    .and_then(|p| sandboxed_display_path(p, &self.media_display_roots()));
                let visual = if let Some(path) = path {
                    img(path)
                        .w(px(32.))
                        .h(px(32.))
                        .aspect_ratio(px(32.) / px(32.))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element()
                } else {
                    div().child(sticker.emoji.clone()).into_any_element()
                };
                let cell = div()
                    .id(SharedString::from(format!("status-cell-{title}-{id}")))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(visual)
                    .child(
                        Button::new(format!("status-{title}-{id}"))
                            .label(if sticker.emoji.is_empty() {
                                tr(keys::EMOJI_STATUS_FOR_SUBMIT).into()
                            } else {
                                tr_with(keys::Q_MEDIA_SET, &[("emoji", &sticker.emoji.to_string())])
                            })
                            .disabled(!premium || busy)
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change_emoji_status(Some(id), cx)
                            })),
                    );
                choices =
                    choices.child(self.with_status_duration_menu(cell, id, premium && !busy, cx));
            }
            section = section.child(div().text_xs().child(title)).child(choices);
        }
        section
            .child(
                Button::new("remove-emoji-status")
                    .label(tr(keys::EMOJI_STATUS_REMOVE))
                    .disabled(!premium || busy)
                    .on_click(cx.listener(|this, _, _, cx| this.change_emoji_status(None, cx))),
            )
            .child(
                Button::new("clear-recent-statuses")
                    .label(tr(keys::Q_MEDIA_CLEAR_RECENT_STATUSES))
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            this.connection.status_note = match live
                                .driver
                                .clear_recent_emoji_statuses()
                            {
                                Ok(Some(_)) => tr(keys::Q_MEDIA_CLEARING_RECENT_STATUSES).into(),
                                Ok(None) => tr(keys::Q_MEDIA_AN_EMOJI_STATUS_IS_ALREADY).into(),
                                Err(_) => tr(keys::Q_MEDIA_COULD_NOT_CLEAR_RECENT_STATUSES).into(),
                            };
                        }
                        cx.notify();
                    })),
            )
            .child(div().text_xs().child(panel.status_note.unwrap_or_else(|| {
                if busy {
                    tr(keys::Q_MEDIA_UPDATING_EMOJI_STATUS).into()
                } else if session.is_some_and(|s| {
                    [
                        RequestPurpose::GetRecentEmojiStatuses,
                        RequestPurpose::GetThemedEmojiStatuses,
                        RequestPurpose::GetDefaultEmojiStatuses,
                        RequestPurpose::GetCustomEmojiStickers,
                    ]
                    .iter()
                    .any(|p| s.requests.has_purpose(*p))
                }) {
                    tr(keys::Q_MEDIA_LOADING_EMOJI_STATUSES).into()
                } else {
                    tr(keys::Q_MEDIA_CHOOSE_A_DURATION_THEN_A).into()
                }
            })))
            .into_any_element()
    }

    fn emoji_sets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let panel = self
            .session()
            .map(|s| s.stickers.emoji.clone())
            .unwrap_or_default();
        let session = self.session();
        let mutating =
            session.is_some_and(|s| s.requests.has_purpose(RequestPurpose::ChangeEmojiSet));
        let loading = session.is_some_and(|s| {
            s.requests.has_purpose(match panel.tab {
                EmojiSetTab::Installed => RequestPurpose::GetInstalledEmojiSets,
                EmojiSetTab::Trending => RequestPurpose::GetTrendingEmojiSets,
                EmojiSetTab::Search => RequestPurpose::SearchEmojiSets,
            }) || s.requests.has_purpose(RequestPurpose::GetEmojiSet)
        });
        let mut tabs = div().flex().gap_2();
        for (title, tab) in [
            (tr(keys::Q_MEDIA_INSTALLED), EmojiSetTab::Installed),
            (tr(keys::STICKERS_FEATURED_TAB), EmojiSetTab::Trending),
            (tr(keys::Q_MEDIA_SEARCH), EmojiSetTab::Search),
        ] {
            tabs = tabs.child(
                Button::new(format!("emoji-sets-tab-{title}"))
                    .label(title)
                    .selected(panel.tab == tab)
                    .ghost()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_emoji_sets_tab(tab, cx)),
                    ),
            );
        }
        let sets: Vec<_> = match panel.tab {
            EmojiSetTab::Installed => session.map(|s| s.ordered_emoji_packs()).unwrap_or_default(),
            EmojiSetTab::Trending => panel.trending_sets.iter().collect(),
            EmojiSetTab::Search => panel.found_sets.iter().collect(),
        };
        let mut rows = div().id("emoji-pack-rows").flex().flex_col().gap_2();
        for set in &sets {
            let id = set.id;
            let installed = set.is_installed;
            let state = session
                .map(|s| s.emoji_pack_download_state(id))
                .unwrap_or(tr(keys::Q_MEDIA_NOT_DOWNLOADED));
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(format!("emoji-pack-preview-{id}"))
                            .label(set.title.clone())
                            .ghost()
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.preview_emoji_set(id, cx)),
                            ),
                    )
                    .child(div().text_xs().child(format!("{} emoji", set.size)))
                    .child(
                        div()
                            .id(format!("emoji-pack-state-{id}"))
                            .text_xs()
                            .role(Role::Label)
                            .aria_label(state)
                            .child(state),
                    )
                    .child(
                        Button::new(format!("emoji-pack-manage-{id}"))
                            .label(if installed {
                                tr(keys::BOX_REMOVE)
                            } else {
                                tr(keys::Q_MEDIA_INSTALL)
                            })
                            .disabled(mutating)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if installed {
                                    this.open_group_confirm(
                                        ChatId(0),
                                        GroupConfirmAction::RemoveEmojiSet { set_id: id },
                                        cx,
                                    );
                                } else {
                                    this.install_emoji_set(id, cx);
                                }
                            })),
                    ),
            );
        }
        if panel.tab == EmojiSetTab::Trending && panel.trending_has_more {
            rows = rows.child(
                Button::new("emoji-packs-more")
                    .label(tr(keys::Q_MEDIA_MORE_PACKS))
                    .disabled(loading)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut()
                            && live.driver.more_trending_emoji_packs().is_err()
                        {
                            this.connection.status_note =
                                tr(keys::Q_MEDIA_COULD_NOT_LOAD_MORE_EMOJI).into();
                        }
                        cx.notify();
                    })),
            );
        }
        let mut preview = div().flex().flex_col().gap_2();
        if let Some(id) = panel.selected_set_id {
            preview = preview.child(div().font_semibold().child(panel.preview_title.clone()));
            let roots = self.media_display_roots();
            let mut grid = div().flex().flex_wrap().gap_2();
            for item in &panel.preview {
                let display = item.thumb_file_id.or_else(|| {
                    (item.format == quill::telegram::envelope::StickerFormat::Webp)
                        .then_some(item.file_id)
                });
                let path = display
                    .and_then(|id| {
                        session
                            .and_then(|s| s.media.files.get(&id.0))
                            .and_then(|f| f.usable_path())
                    })
                    .and_then(|p| sandboxed_display_path(p, &roots));
                let cell = if let Some(path) = path {
                    img(path)
                        .w(px(56.))
                        .h(px(56.))
                        .aspect_ratio(px(56.) / px(56.))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element()
                } else {
                    div()
                        .w(px(56.))
                        .h(px(56.))
                        .child(item.emoji.clone())
                        .into_any_element()
                };
                grid = grid.child(cell);
            }
            preview = preview.child(grid);
            preview = preview.child(
                Button::new("download-emoji-pack")
                    .label(tr(keys::Q_MEDIA_DOWNLOAD_OR_RETRY_PACK))
                    .disabled(
                        loading || panel.outdated_packs.contains(&id) || panel.preview.is_empty(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            this.connection.status_note = match live.driver.download_emoji_pack(id)
                            {
                                Ok(()) => tr(keys::Q_MEDIA_EMOJI_PACK_DOWNLOAD_REQUESTED).into(),
                                Err(_) => tr(keys::Q_MEDIA_COULD_NOT_DOWNLOAD_EMOJI_PACK).into(),
                            };
                        }
                        cx.notify();
                    })),
            );
        }
        let status = if panel.failed || panel.mutation_failed {
            tr(keys::Q_MEDIA_COULD_NOT_UPDATE_EMOJI_PACKS)
        } else if mutating {
            tr(keys::Q_MEDIA_UPDATING_EMOJI_PACK)
        } else if loading {
            tr(keys::Q_MEDIA_LOADING_EMOJI_PACKS)
        } else if sets.is_empty() {
            tr(keys::Q_MEDIA_NO_EMOJI_PACKS_FOUND)
        } else {
            tr(keys::Q_MEDIA_OPEN_A_PACK_TO_PREVIEW)
        };
        div()
            .id("emoji-sets-settings")
            .flex()
            .flex_col()
            .gap_2()
            .max_h(px(460.))
            .overflow_y_scroll()
            .child(self.emoji_status_panel(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("dynamic-emoji-pack-order")
                            .checked(
                                session.is_none_or(|s| {
                                    s.settings.media_prefs.dynamic_emoji_pack_order
                                }),
                            )
                            .accessibility_label(tr(keys::Q_MEDIA_DYNAMIC_EMOJI_PACK_ORDER))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_media_pref(|prefs| prefs.dynamic_emoji_pack_order = on, cx)
                            })),
                    )
                    .child(div().child(tr(keys::Q_MEDIA_DYNAMIC_EMOJI_PACK_ORDER))),
            )
            .child(tabs)
            .child(
                Textarea::new(&self.pickers.emoji_set_search_input)
                    .aria_label(tr(keys::Q_MEDIA_SEARCH_EMOJI_PACKS)),
            )
            .child(
                Button::new("emoji-pack-search")
                    .label(tr(keys::Q_MEDIA_SEARCH_PACKS))
                    .on_click(cx.listener(|this, _, _, cx| this.search_emoji_sets(cx))),
            )
            .child(div().text_xs().child(status))
            .child(rows)
            .child(preview)
            .into_any_element()
    }
    pub(super) fn build_emoji_sets_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let close = QuillShell::on_close_kind(app, shell, DialogKind::EmojiSets, |this, _, cx| {
            this.close_emoji_sets(cx)
        });
        app.update(cx, |this, cx| {
            let body = Rc::new(RefCell::new(Some(this.emoji_sets_panel(cx))));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MEDIA_EMOJI_PACKS_AND_STATUS,
                )))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    },
                ))
                .on_close(close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    EmojiSets => DialogSpec::new(
        5100,
        |app| app.session().is_some_and(|s| s.stickers.emoji.open),
        QuillApp::build_emoji_sets_dialog,
    ),
}

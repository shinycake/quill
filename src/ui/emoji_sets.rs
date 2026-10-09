//! Custom emoji pack settings, preview and confirmed removal.
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
use quill::local_path::sandboxed_display_path;
use quill::state::RequestPurpose;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    pub(super) fn open_emoji_sets(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_emoji_sets().is_err() {
                self.status_note = "could not load emoji packs".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.emoji.open = true;
        }
        cx.notify();
    }
    fn close_emoji_sets(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.emoji.open = false;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.emoji.open = false;
        }
        cx.notify();
    }
    fn select_emoji_sets_tab(&mut self, tab: EmojiSetTab, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.select_emoji_set_tab(tab).is_err() {
                self.status_note = "could not load emoji packs".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.emoji.tab = tab;
        }
        cx.notify();
    }
    fn preview_emoji_set(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.preview_emoji_pack(id).is_err()
        {
            self.status_note = "could not load emoji pack".into();
        }
        cx.notify();
    }
    fn install_emoji_set(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.set_emoji_pack_installed(id, true) {
                Ok(Some(_)) => "installing emoji pack…".into(),
                Ok(None) => "an emoji pack is already updating".into(),
                Err(_) => "could not install emoji pack".into(),
            };
        }
        cx.notify();
    }
    fn search_emoji_sets(&mut self, cx: &mut Context<Self>) {
        let query = self.emoji_set_search_input.read(cx).value().to_string();
        if let Some(live) = self.live.as_mut()
            && live.driver.search_emoji_packs(&query).is_err()
        {
            self.status_note = "could not search emoji packs".into();
        }
        cx.notify();
    }
    fn change_emoji_status(&mut self, id: Option<i64>, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let duration = live.driver.session.emoji.status_duration_secs;
            self.status_note = match live.driver.change_emoji_status(id, duration) {
                Ok(Some(_)) => "Updating emoji status…".into(),
                Ok(None) => "An emoji status is already updating.".into(),
                Err(_) => "Could not update emoji status. Retry the action.".into(),
            };
        }
        cx.notify();
    }

    fn emoji_status_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let panel = session.map(|s| s.emoji.clone()).unwrap_or_default();
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
            .child(div().font_semibold().child("Emoji status"))
            .child(
                Button::new("load-emoji-statuses")
                    .label("Load or refresh statuses")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut()
                            && live.driver.load_emoji_status_choices().is_err()
                        {
                            this.status_note =
                                "Could not load emoji statuses. Retry the action.".into();
                        }
                        cx.notify();
                    })),
            );
        if !premium {
            section = section.child(
                div()
                    .text_xs()
                    .child("Setting an emoji status requires Telegram Premium."),
            );
        }
        if !panel.status_open {
            return section.into_any_element();
        }
        let mut durations = div().flex().flex_wrap().gap_2();
        for (title, secs) in [
            ("Forever", 0),
            ("1 hour", 3600),
            ("2 hours", 7200),
            ("8 hours", 28800),
            ("2 days", 172800),
        ] {
            durations = durations.child(
                Button::new(format!("status-duration-{secs}"))
                    .label(title)
                    .selected(panel.status_duration_secs == secs)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            live.driver.session.emoji.status_duration_secs = secs;
                        }
                        cx.notify();
                    })),
            );
        }
        section = section
            .child(durations)
            .child(
                Textarea::new(&self.emoji_status_hours_input)
                    .aria_label("Emoji status duration in hours"),
            )
            .child(
                Button::new("status-custom-duration")
                    .label("Use custom duration")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let hours = this
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
                            if let Some(live) = this.live.as_mut() {
                                live.driver.session.emoji.status_duration_secs = secs;
                            }
                        } else {
                            this.status_note =
                                "Enter positive whole hours within the supported date range."
                                    .into();
                        }
                        cx.notify();
                    })),
            )
            .child(div().text_xs().child(if panel.status_duration_secs == 0 {
                "Selected duration: forever".into()
            } else {
                format!(
                    "Selected duration: {} hours",
                    panel.status_duration_secs / 3600
                )
            }));
        for (title, ids) in [
            (
                "Recent",
                panel
                    .recent_statuses
                    .iter()
                    .map(|s| s.custom_emoji_id)
                    .collect::<Vec<_>>(),
            ),
            ("Trending", panel.themed_status_ids.clone()),
            ("Default", panel.default_status_ids.clone()),
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
                            .and_then(|s| s.files.get(&id.0))
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
                choices = choices.child(
                    div().flex().flex_col().items_center().child(visual).child(
                        Button::new(format!("status-{title}-{id}"))
                            .label(if sticker.emoji.is_empty() {
                                "Set status".into()
                            } else {
                                format!("Set {}", sticker.emoji)
                            })
                            .disabled(!premium || busy)
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change_emoji_status(Some(id), cx)
                            })),
                    ),
                );
            }
            section = section.child(div().text_xs().child(title)).child(choices);
        }
        section
            .child(
                Button::new("remove-emoji-status")
                    .label("Remove status")
                    .disabled(!premium || busy)
                    .on_click(cx.listener(|this, _, _, cx| this.change_emoji_status(None, cx))),
            )
            .child(
                Button::new("clear-recent-statuses")
                    .label("Clear recent statuses")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            this.status_note = match live.driver.clear_recent_emoji_statuses() {
                                Ok(Some(_)) => "Clearing recent statuses…".into(),
                                Ok(None) => "An emoji status is already updating.".into(),
                                Err(_) => {
                                    "Could not clear recent statuses. Retry the action.".into()
                                }
                            };
                        }
                        cx.notify();
                    })),
            )
            .child(div().text_xs().child(panel.status_note.unwrap_or_else(|| {
                if busy {
                    "Updating emoji status…".into()
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
                    "Loading emoji statuses…".into()
                } else {
                    "Choose a duration, then a status. Refresh to retry missing choices.".into()
                }
            })))
            .into_any_element()
    }

    fn emoji_sets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let panel = self.session().map(|s| s.emoji.clone()).unwrap_or_default();
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
            ("Installed", EmojiSetTab::Installed),
            ("Trending", EmojiSetTab::Trending),
            ("Search", EmojiSetTab::Search),
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
                .unwrap_or("Not downloaded");
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
                            .label(if installed { "Remove" } else { "Install" })
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
                    .label("More packs")
                    .disabled(loading)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut()
                            && live.driver.more_trending_emoji_packs().is_err()
                        {
                            this.status_note = "could not load more emoji packs".into();
                        }
                        cx.notify();
                    })),
            );
        }
        let mut preview = div().flex().flex_col().gap_2();
        if panel.selected_set_id.is_some() {
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
                            .and_then(|s| s.files.get(&id.0))
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
            let id = panel.selected_set_id.unwrap();
            preview = preview.child(
                Button::new("download-emoji-pack")
                    .label("Download or retry pack")
                    .disabled(
                        loading || panel.outdated_packs.contains(&id) || panel.preview.is_empty(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            this.status_note = match live.driver.download_emoji_pack(id) {
                                Ok(()) => "Emoji pack download requested.".into(),
                                Err(_) => {
                                    "Could not download emoji pack. Refresh the preview and retry."
                                        .into()
                                }
                            };
                        }
                        cx.notify();
                    })),
            );
        }
        let status = if panel.failed || panel.mutation_failed {
            "Could not update emoji packs. Retry the action."
        } else if mutating {
            "Updating emoji pack…"
        } else if loading {
            "Loading emoji packs…"
        } else if sets.is_empty() {
            "No emoji packs found."
        } else {
            "Open a pack to preview its emoji."
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
                            .checked(session.is_none_or(|s| s.media_prefs.dynamic_emoji_pack_order))
                            .accessibility_label("Dynamic emoji pack order")
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_media_pref(|prefs| prefs.dynamic_emoji_pack_order = on, cx)
                            })),
                    )
                    .child(div().child("Dynamic emoji pack order")),
            )
            .child(tabs)
            .child(Textarea::new(&self.emoji_set_search_input).aria_label("Search emoji packs"))
            .child(
                Button::new("emoji-pack-search")
                    .label("Search packs")
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
                .title(crate::ui::shell::dialog_title("Emoji Packs and Status"))
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

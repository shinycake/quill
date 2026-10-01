//! Sticker collection picker and set management.
use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::pressable::PressableDiv;
use super::*;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use quill::local_path::sandboxed_display_path;
use quill::state::{RequestPurpose, StickerTab};
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    fn batch_install_sticker_sets(&mut self, ids: &[i64], cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.manage_sticker_sets(ids, true) {
                Ok(sent) => format!("installing {sent} sticker sets…"),
                Err(_) => "could not start sticker batch; wait for pending updates".into(),
            };
        }
        cx.notify();
    }

    pub(super) fn sync_sticker_suggestions(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.update_sticker_suggestions(text).is_err() {
                self.status_note = "could not load sticker suggestions".into();
            }
        }
        cx.notify();
    }

    pub(super) fn sync_animated_emoji_suggestion(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.update_animated_emoji_suggestion(text).is_err() {
                self.status_note = "could not load animated emoji suggestion".into();
            }
        }
        cx.notify();
    }

    pub(super) fn animated_emoji_suggestion(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let sticker = session.emoji.animated_emoji.as_ref()?;
        let row = div()
            .id("animated-emoji-suggestion")
            .flex()
            .gap_2()
            .child(self.sticker_cell(sticker, "animated-emoji", cx));
        Some(row.into_any_element())
    }

    fn set_sticker_suggest_mode(
        &mut self,
        mode: quill::sticker_suggest::StickerSuggestMode,
        cx: &mut Context<Self>,
    ) {
        self.set_media_pref(|prefs| prefs.sticker_suggest_mode = mode, cx);
        if let Some(live) = self.live.as_mut() {
            drop(
                live.driver
                    .session
                    .requests
                    .take_purpose(RequestPurpose::SuggestStickers),
            );
            live.driver.session.clear_sticker_suggestions();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.clear_sticker_suggestions();
        }
        let text = self.composer.read(cx).value().to_string();
        self.sync_sticker_suggestions(&text, cx);
    }

    pub(super) fn sticker_suggestions_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if session.stickers.suggestions.is_empty() {
            return None;
        }
        let row = div()
            .id("sticker-suggestions")
            .flex()
            .gap_2()
            .overflow_x_scroll()
            .children(
                session
                    .stickers
                    .suggestions
                    .iter()
                    .map(|sticker| self.sticker_cell(sticker, "suggest", cx)),
            );
        Some(row.into_any_element())
    }

    fn sticker_cell(
        &self,
        sticker: &quill::telegram::envelope::StickerItem,
        prefix: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let roots = self.media_display_roots();
        let file_id = sticker.file_id;
        let emoji = sticker.emoji.clone();
        let width = sticker.width;
        let height = sticker.height;
        let thumb = sticker
            .thumb_file_id
            .filter(|id| id.0 != 0)
            .map(|id| (id, sticker.thumb_width, sticker.thumb_height));
        let display_id = sticker.thumb_file_id.filter(|id| id.0 != 0).or_else(|| {
            (sticker.format == quill::telegram::envelope::StickerFormat::Webp && file_id.0 != 0)
                .then_some(file_id)
        });
        let path = display_id.and_then(|id| {
            self.session()
                .map(|s| &s.files)?
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, &roots))
        });
        let label = if emoji.is_empty() {
            "Sticker".to_string()
        } else {
            emoji.clone()
        };
        let cell_id = format!("sticker-{prefix}-{}-{}", sticker.set_id, sticker.id);
        let cell = if let Some(path) = path {
            img(path)
                .id(SharedString::from(cell_id.clone()))
                .w(px(72.))
                .h(px(72.))
                .rounded_md()
                .object_fit(ObjectFit::Contain)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.send_sticker_pick(file_id, emoji.clone(), width, height, thumb, cx);
                }))
                .with_fallback({
                    let label = label.clone();
                    move || {
                        div()
                            .w(px(72.))
                            .h(px(72.))
                            .rounded_md()
                            .bg(fill_muted())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(label.clone())
                            .into_any_element()
                    }
                })
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(cell_id))
                .w(px(72.))
                .h(px(72.))
                .rounded_md()
                .bg(bg_subtle())
                .border_1()
                .border_color(text_muted())
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.send_sticker_pick(file_id, emoji.clone(), width, height, thumb, cx);
                }))
                .child(label)
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(cell)
            .when(
                sticker.requires_premium && !self.session().is_some_and(|s| s.my_is_premium()),
                |cell| cell.child(div().text_xs().child("Premium")),
            )
            .into_any_element()
    }

    pub(super) fn open_archived_stickers(&mut self, cx: &mut Context<Self>) {
        self.sticker_settings_open = true;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.stickers.open = true;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.open = true;
        }
        self.select_sticker_tab(StickerTab::Archived, cx);
    }

    pub(super) fn build_archived_stickers_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ArchivedStickers, |this, _, cx| {
                this.close_sticker_panel(cx)
            });
        app.update(cx, |this, cx| {
            let body = Rc::new(RefCell::new(Some(
                this.sticker_picker_panel(cx).into_any_element(),
            )));
            dialog
                .overlay(true)
                .title("Archived stickers")
                .content(move |content, _, _| {
                    content.child(
                        body.borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element()),
                    )
                })
                .on_close(on_close)
        })
    }

    fn more_archived_stickers(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.fetch_archived_stickers(true) {
                Ok(_) => "loading archived stickers…".into(),
                Err(_) => "could not load archived stickers".into(),
            };
        }
        cx.notify();
    }

    fn archive_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.manage_sticker_set(set_id, false, true) {
                Ok(Some(_)) => "archiving sticker set…".into(),
                Ok(None) => "sticker set update already pending".into(),
                Err(_) => "could not archive sticker set".into(),
            };
        }
        cx.notify();
    }

    fn select_sticker_tab(&mut self, tab: StickerTab, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.select_sticker_tab(tab) {
                Ok(_) => "stickers".into(),
                Err(_) => "could not load sticker tab".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.tab = tab;
        }
        cx.notify();
    }

    fn more_trending_stickers(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.fetch_trending_stickers(true) {
                Ok(_) => "loading more trending stickers…".into(),
                Err(_) => "could not load more trending stickers".into(),
            };
        }
        cx.notify();
    }

    fn favorite_sticker(&mut self, id: FileId, favorite: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.set_favorite_sticker(id, favorite) {
                Ok(_) => "updating favorite stickers…".into(),
                Err(_) => "could not update favorite sticker".into(),
            };
        }
        cx.notify();
    }

    fn clear_recent_stickers(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.clear_recent_stickers() {
                Ok(_) => "clearing recent stickers…".into(),
                Err(_) => "could not clear recent stickers".into(),
            };
        }
        cx.notify();
    }

    fn search_sticker_picker(&mut self, cx: &mut Context<Self>) {
        let query = self.sticker_search_input.read(cx).value().to_string();
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.search_sticker_picker(&query) {
                Ok(_) => "searching stickers…".into(),
                Err(_) => "could not search stickers".into(),
            };
        }
        cx.notify();
    }

    fn more_sticker_search_results(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.more_sticker_search_results() {
                Ok(_) => "loading more stickers…".into(),
                Err(_) => "could not load more stickers".into(),
            };
        }
        cx.notify();
    }

    fn install_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.manage_sticker_set(set_id, true, false) {
                Ok(Some(_)) => "installing sticker set…".into(),
                Ok(None) => "sticker set update already in progress".into(),
                Err(_) => "could not install sticker set".into(),
            };
        }
        cx.notify();
    }

    fn reorder_sticker_set(&mut self, source: i64, target: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.reorder_sticker_set(source, target) {
                Ok(Some(_)) => "saving sticker set order…".into(),
                Ok(None) => "sticker order unchanged or update already pending".into(),
                Err(_) => "could not reorder sticker sets".into(),
            };
        }
        cx.notify();
    }

    fn sticker_set_action(
        &self,
        set: &quill::telegram::envelope::StickerSetInfo,
        prefix: &str,
        cx: &mut Context<Self>,
    ) -> Button {
        let set_id = set.id;
        let pending = self.session().is_some_and(|session| {
            [(true, false), (false, true), (false, false)]
                .into_iter()
                .any(|(installed, archived)| {
                    session
                        .requests
                        .has_purpose(RequestPurpose::ManageStickerSet {
                            set_id,
                            installed,
                            archived,
                        })
                })
        });
        let button = Button::new(format!("sticker-{prefix}-action-{set_id}"))
            .disabled(pending)
            .ghost();
        if set.is_installed {
            button
                .label(if pending { "Updating…" } else { "Remove" })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_group_confirm(
                        ChatId(0),
                        GroupConfirmAction::RemoveStickerSet { set_id },
                        cx,
                    )
                }))
        } else {
            button
                .label(if pending {
                    "Installing…"
                } else if self
                    .session()
                    .is_some_and(|s| s.stickers.archived.iter().any(|set| set.id == set_id))
                {
                    "Restore"
                } else {
                    "Install"
                })
                .on_click(cx.listener(move |this, _, _, cx| this.install_sticker_set(set_id, cx)))
        }
    }

    pub(super) fn sticker_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self
            .session()
            .map(|session| session.stickers.clone())
            .unwrap_or_default();
        let mut sets = div()
            .id("sticker-set-row")
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(96.))
            .overflow_y_scroll();
        let mut tabs = div().flex().flex_wrap().gap_1();
        for (tab, label) in [
            (StickerTab::Installed, "Installed"),
            (StickerTab::Recent, "Recent"),
            (StickerTab::Favorites, "Favorites"),
            (StickerTab::Trending, "Trending"),
            (StickerTab::Search, "Search"),
            (StickerTab::Archived, "Archived"),
        ] {
            tabs = tabs.child(
                Button::new(format!("sticker-tab-{tab:?}"))
                    .label(if panel.tab == tab {
                        format!("{label} · open")
                    } else {
                        label.into()
                    })
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| this.select_sticker_tab(tab, cx))),
            );
        }
        if panel.tab == StickerTab::Recent {
            tabs = tabs.child(
                Button::new("clear-recent-stickers")
                    .label("Clear recent")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.clear_recent_stickers(cx))),
            );
        }
        let mut suggest_modes = div().flex().flex_wrap().gap_1().child("Suggest by emoji:");
        let mode = self
            .session()
            .map(|s| s.media_prefs.sticker_suggest_mode)
            .unwrap_or_default();
        use quill::sticker_suggest::StickerSuggestMode;
        for (value, label) in [
            (
                StickerSuggestMode::InstalledAndRecommended,
                "Installed + recommended",
            ),
            (StickerSuggestMode::InstalledOnly, "Only installed"),
            (StickerSuggestMode::None, "None"),
        ] {
            suggest_modes = suggest_modes.child(
                Button::new(format!("sticker-suggest-{value:?}"))
                    .label(if mode == value {
                        format!("{label} · selected")
                    } else {
                        label.into()
                    })
                    .ghost()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.set_sticker_suggest_mode(value, cx)),
                    ),
            );
        }
        let query = panel.search_query.to_lowercase();
        let mut seen = std::collections::HashSet::new();
        let visible_sets: Vec<_> = match panel.tab {
            StickerTab::Installed => panel.sets.iter().collect(),
            StickerTab::Trending => panel.trending.iter().collect(),
            StickerTab::Archived => panel.archived.iter().collect(),
            StickerTab::Search => panel
                .found_sets
                .iter()
                .chain(panel.sets.iter().filter(|set| {
                    !query.is_empty()
                        && (set.title.to_lowercase().contains(&query)
                            || set.name.to_lowercase().contains(&query))
                }))
                .filter(|set| seen.insert(set.id))
                .collect(),
            _ => vec![],
        };
        let batch_ids: Vec<_> = visible_sets
            .iter()
            .filter(|set| !set.is_installed)
            .map(|set| set.id)
            .collect();
        if !batch_ids.is_empty() {
            tabs = tabs.child(
                Button::new("install-displayed-sticker-sets")
                    .label(format!("Install {} displayed sets", batch_ids.len()))
                    .disabled(!panel.batch_pending.is_empty())
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.batch_install_sticker_sets(&batch_ids, cx)
                    })),
            );
        }
        if panel.tab == StickerTab::Installed && !panel.sets.is_empty() {
            tabs = tabs.child(
                Button::new("remove-installed-sticker-sets")
                    .label("Remove all installed")
                    .disabled(!panel.batch_pending.is_empty())
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_group_confirm(
                            ChatId(0),
                            GroupConfirmAction::RemoveInstalledStickerSets,
                            cx,
                        );
                    })),
            );
        }
        for set in &visible_sets {
            let set_id = set.id;
            let selected = panel.selected_set_id == Some(set_id);
            let title = if set.title.is_empty() {
                set.name.clone()
            } else {
                set.title.clone()
            };
            let row = div()
                .id(format!("sticker-set-drag-{set_id}"))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new(format!("sticker-set-{set_id}"))
                        .label(format!(
                            "{title} · {} stickers{}",
                            set.size,
                            if selected { " · open" } else { "" }
                        ))
                        .ghost()
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_sticker_set(set_id, cx)),
                        ),
                )
                .child(self.sticker_set_action(set, "row", cx))
                .when(set.is_installed, |row| {
                    row.child(
                        Button::new(format!("archive-set-{set_id}"))
                            .label("Archive")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.archive_sticker_set(set_id, cx)
                            })),
                    )
                })
                .when(panel.tab == StickerTab::Installed, |row| {
                    row.cursor_move()
                        .on_drag(
                            StickerSetDrag {
                                id: set_id,
                                title: title.clone(),
                            },
                            |drag: &StickerSetDrag, _, _, cx| cx.new(|_| drag.clone()),
                        )
                        .on_drop(cx.listener(move |this, drag: &StickerSetDrag, _, cx| {
                            this.reorder_sticker_set(drag.id, set_id, cx);
                        }))
                });
            sets = sets.child(row);
        }
        if panel.tab == StickerTab::Trending && panel.trending_next_offset < panel.trending_total {
            sets = sets.child(
                Button::new("sticker-trending-more")
                    .label("More trending sets")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_trending_stickers(cx))),
            );
        }
        if panel.tab == StickerTab::Archived && panel.archived_has_more {
            sets = sets.child(
                Button::new("more-archived-stickers")
                    .label("More archived sets")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_archived_stickers(cx))),
            );
        }
        let mut grid = div().id("sticker-grid").flex().flex_wrap().gap_2();
        for sticker in panel.visible_stickers() {
            let file_id = sticker.file_id;
            let cell = self.sticker_cell(sticker, "pick", cx);
            let favorite = panel.favorites.iter().any(|item| item.file_id == file_id);
            grid = grid.child(
                div().flex().flex_col().gap_1().child(cell).child(
                    Button::new(format!(
                        "sticker-favorite-{}-{}",
                        sticker.set_id, sticker.id
                    ))
                    .label(if favorite { "Unfavorite" } else { "Favorite" })
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.favorite_sticker(file_id, !favorite, cx)
                    })),
                ),
            );
        }
        if panel.tab == StickerTab::Search
            && panel.search_has_more
            && panel.selected_set_id.is_none()
        {
            grid = grid.child(
                Button::new("sticker-search-more")
                    .label("More stickers")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_sticker_search_results(cx))),
            );
        }
        let mut preview = div().flex().items_center().justify_between().gap_2();
        if let Some(set) = visible_sets
            .iter()
            .find(|set| Some(set.id) == panel.selected_set_id)
        {
            preview = preview
                .child(
                    div()
                        .font_semibold()
                        .child(format!("{} · {} stickers", set.title, set.size)),
                )
                .child(self.sticker_set_action(set, "preview", cx));
            if panel.tab == StickerTab::Search {
                preview = preview.child(
                    Button::new("sticker-search-back")
                        .label("Back to results")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.select_sticker_tab(StickerTab::Search, cx)
                        })),
                );
            }
        }
        let loading_tab = self.session().is_some_and(|session| {
            let purpose = match panel.tab {
                StickerTab::Installed => RequestPurpose::GetInstalledStickerSets,
                StickerTab::Recent => RequestPurpose::GetRecentStickers,
                StickerTab::Favorites => RequestPurpose::GetFavoriteStickers,
                StickerTab::Trending => RequestPurpose::GetTrendingStickerSets,
                StickerTab::Archived => RequestPurpose::GetArchivedStickerSets,
                StickerTab::Search => RequestPurpose::SearchStickerSets,
            };
            session.requests.has_purpose(purpose)
                || panel.tab == StickerTab::Search
                    && session.requests.has_purpose(RequestPurpose::SearchStickers)
        });
        let status = if loading_tab || panel.loading_set {
            "Loading stickers…".to_string()
        } else if panel.failed {
            "Could not update stickers. Try the action again.".into()
        } else {
            match panel.tab {
                StickerTab::Installed if panel.sets.is_empty() => {
                    "No sticker sets installed.".into()
                }
                StickerTab::Installed => format!(
                    "{} sets installed · Drag sets to reorder. Tap a sticker to send it.",
                    panel.sets.len()
                ),
                StickerTab::Archived if panel.archived.is_empty() => {
                    "No archived sticker sets.".into()
                }
                StickerTab::Archived => "Restore a set to use it again.".into(),
                StickerTab::Recent if panel.recent.is_empty() => "No recent stickers.".into(),
                StickerTab::Favorites if panel.favorites.is_empty() => {
                    "No favorite stickers. Add one from an installed set.".into()
                }
                StickerTab::Trending if panel.trending.is_empty() => {
                    "No trending sticker sets.".into()
                }
                StickerTab::Trending => "Select a trending set to preview its stickers.".into(),
                StickerTab::Search if panel.search_query.is_empty() => {
                    "Search by emoji, title or keyword.".into()
                }
                StickerTab::Search
                    if panel.found_stickers.is_empty() && visible_sets.is_empty() =>
                {
                    "No stickers found.".into()
                }
                _ => "Tap a sticker to send it.".into(),
            }
        };
        div()
            .id("sticker-picker")
            .max_h(px(420.))
            .overflow_y_scroll()
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
                    .child(div().font_semibold().child("Stickers"))
                    .child(
                        Button::new("close-sticker-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_sticker_panel(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            .child(tabs)
            .child(suggest_modes)
            .when(panel.batch_total > 0, |body| {
                body.child(div().text_sm().child(format!(
                    "Batch: {} of {} updated · {} pending · {} failed",
                    panel.batch_completed,
                    panel.batch_total,
                    panel.batch_pending.len(),
                    panel.batch_failed
                )))
            })
            .when(panel.tab == StickerTab::Search, |body| {
                body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(Textarea::new(&self.sticker_search_input).h(px(36.))),
                        )
                        .child(
                            Button::new("sticker-search-submit")
                                .label("Search")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.search_sticker_picker(cx)),
                                ),
                        ),
                )
            })
            .child(sets)
            .child(preview)
            .child(
                div()
                    .id("sticker-scroll")
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .child(grid),
            )
    }
}

#[derive(Clone)]
struct StickerSetDrag {
    id: i64,
    title: String,
}

impl Render for StickerSetDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.15))
            .text_sm()
            .child(self.title.clone())
    }
}

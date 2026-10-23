//! The stickers manager box (Telegram Desktop `boxes/stickers_box.cpp`),
//! opened from Settings > Chats > "Manage sticker sets" and from the photo
//! editor's masks strip.
//!
//! Tabs: Stickers (installed), Trending and Archived; the mask box has
//! Masks and Archived. Rows show the first sticker of the set, its title and
//! count and open the set box. On the installed tab a row has a reorder
//! handle and a remove button; removing only marks the row (Undo takes the
//! mark off) and the marks and the dragged order are sent when the box
//! closes. Trending and archived rows have Add; more sets load when the list
//! is scrolled to its end.
//! Decision: docs/decisions/stickers-manager-box.md.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::{MasksTab, RequestPurpose, StickerTab, StickersPurpose};
use quill::stickers_manager::ManagerEdits;
use quill::telegram::envelope::StickerSetInfo;
use rows::RowState;
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(feature = "demo-capture")]
mod demo;
mod rows;
mod settings;
mod state;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

pub(crate) use state::StickersManagerUi;

/// Distance from the end of the list at which the next page is requested.
const LOAD_MORE_DISTANCE: f32 = 160.;

/// Which sets the box manages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Stickers,
    Masks,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ManagerTab {
    Installed,
    Trending,
    Archived,
}

/// The lists of one kind, copied out of the session for a frame.
struct ManagerView {
    tab: ManagerTab,
    tabs: Vec<ManagerTab>,
    installed: Vec<StickerSetInfo>,
    trending: Vec<StickerSetInfo>,
    archived: Vec<StickerSetInfo>,
    /// The list on screen is still being fetched.
    loading: bool,
    failed: bool,
    unread: usize,
}

impl QuillApp {
    fn manager_edits(&mut self, kind: Kind) -> &mut ManagerEdits {
        let manager = &mut self.pickers.sticker_manager;
        match kind {
            Kind::Stickers => &mut manager.stickers,
            Kind::Masks => &mut manager.masks,
        }
    }

    fn manager_view(&self, kind: Kind) -> ManagerView {
        let Some(session) = self.session() else {
            return ManagerView {
                tab: ManagerTab::Installed,
                tabs: vec![ManagerTab::Installed],
                installed: Vec::new(),
                trending: Vec::new(),
                archived: Vec::new(),
                loading: false,
                failed: false,
                unread: 0,
            };
        };
        let panel = &session.stickers.stickers;
        let masks = &session.stickers.masks;
        let (tab, installed, trending, archived, loading, failed) = match kind {
            Kind::Stickers => {
                let tab = match panel.tab {
                    StickerTab::Trending => ManagerTab::Trending,
                    StickerTab::Archived => ManagerTab::Archived,
                    _ => ManagerTab::Installed,
                };
                let purpose = match tab {
                    ManagerTab::Installed => RequestPurpose::GetInstalledStickerSets,
                    ManagerTab::Trending => RequestPurpose::GetTrendingStickerSets,
                    ManagerTab::Archived => RequestPurpose::GetArchivedStickerSets,
                };
                let loading = session.requests.has_purpose(purpose)
                    || tab == ManagerTab::Installed && !panel.installed_loaded;
                (
                    tab,
                    panel.sets.clone(),
                    panel.trending.clone(),
                    panel.archived.clone(),
                    loading,
                    panel.failed,
                )
            }
            Kind::Masks => {
                let tab = match masks.tab {
                    MasksTab::Installed => ManagerTab::Installed,
                    MasksTab::Archived => ManagerTab::Archived,
                };
                let loading = session
                    .requests
                    .has_purpose(StickersPurpose::GetInstalledMaskSets.into())
                    || session.requests.has_purpose(
                        StickersPurpose::GetArchivedMaskSets { first_page: true }.into(),
                    )
                    || session.requests.has_purpose(
                        StickersPurpose::GetArchivedMaskSets { first_page: false }.into(),
                    )
                    || match tab {
                        ManagerTab::Installed => !masks.installed_loaded,
                        _ => !masks.archived_loaded,
                    };
                (
                    tab,
                    masks.installed.clone(),
                    Vec::new(),
                    masks.archived.clone(),
                    loading,
                    masks.failed,
                )
            }
        };
        // Like tdesktop's `refreshTabs`: Trending and Archived show only
        // while they have sets, or while they are the tab on screen.
        let mut tabs = vec![ManagerTab::Installed];
        if kind == Kind::Stickers && (!trending.is_empty() || tab == ManagerTab::Trending) {
            tabs.push(ManagerTab::Trending);
        }
        if !archived.is_empty() || tab == ManagerTab::Archived {
            tabs.push(ManagerTab::Archived);
        }
        let unread = trending.iter().filter(|set| !set.is_viewed).count();
        ManagerView {
            tab,
            tabs,
            installed,
            trending,
            archived,
            loading,
            failed,
            unread,
        }
    }

    /// Open the box on the installed stickers.
    pub(super) fn open_archived_stickers(&mut self, cx: &mut Context<Self>) {
        self.settings.sticker_settings_open = true;
        self.pickers.sticker_manager.stickers.clear();
        self.pickers
            .sticker_manager
            .scroll
            .set_offset(point(px(0.), px(0.)));
        if let Some(live) = self.live.as_mut() {
            live.driver.set_sticker_manager_open(true);
            let driver = &mut live.driver;
            let loaded = driver
                .select_sticker_tab(StickerTab::Installed)
                .and_then(|_| {
                    // Trending and Archived tabs appear once they have sets.
                    driver.fetch_trending_stickers(false)?;
                    driver.fetch_archived_stickers(false)
                });
            if loaded.is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_LOAD_STICKER_TAB,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.open = true;
            session.stickers.stickers.tab = StickerTab::Installed;
            session.stickers.manager.open = true;
        }
        cx.notify();
    }

    /// Close the box and send what it changed (tdesktop `saveChanges`).
    pub(super) fn close_stickers_manager(&mut self, cx: &mut Context<Self>) {
        self.settings.sticker_settings_open = false;
        self.finish_manager(Kind::Stickers);
        if let Some(live) = self.live.as_mut() {
            live.driver.close_sticker_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.close();
        }
        cx.notify();
    }

    pub(super) fn close_mask_sets(&mut self, cx: &mut Context<Self>) {
        self.finish_manager(Kind::Masks);
        if let Some(masks) = self.masks_panel_mut() {
            masks.open = false;
            masks.confirm_remove = None;
        }
        cx.notify();
    }

    /// Send the removals and the order, mark the trending sets seen and
    /// forget the box's local state.
    fn finish_manager(&mut self, kind: Kind) {
        let installed: Vec<i64> = self
            .manager_view(kind)
            .installed
            .iter()
            .map(|set| set.id)
            .collect();
        let (removed, order) = self.manager_edits(kind).changes(&installed);
        self.manager_edits(kind).clear();
        if let Some(live) = self.live.as_mut() {
            if kind == Kind::Stickers {
                let _ = live.driver.view_shown_trending_stickers();
            }
            if live
                .driver
                .commit_sticker_manager(kind == Kind::Masks, removed, order)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN,
                )));
            }
            live.driver.set_sticker_manager_open(false);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.manager.open = false;
            apply_demo_commit(session, kind, &removed, order.as_deref());
        }
    }

    fn manager_select_tab(&mut self, kind: Kind, tab: ManagerTab, cx: &mut Context<Self>) {
        self.pickers
            .sticker_manager
            .scroll
            .set_offset(point(px(0.), px(0.)));
        if let Some(live) = self.live.as_mut() {
            let result = match (kind, tab) {
                (Kind::Stickers, tab) => {
                    if tab != ManagerTab::Trending {
                        let _ = live.driver.view_shown_trending_stickers();
                    }
                    live.driver.select_sticker_tab(match tab {
                        ManagerTab::Installed => StickerTab::Installed,
                        ManagerTab::Trending => StickerTab::Trending,
                        ManagerTab::Archived => StickerTab::Archived,
                    })
                }
                (Kind::Masks, ManagerTab::Archived) => {
                    live.driver.select_mask_tab(MasksTab::Archived)
                }
                (Kind::Masks, _) => live.driver.select_mask_tab(MasksTab::Installed),
            };
            if result.is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_LOAD_STICKER_TAB,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            match kind {
                Kind::Stickers => {
                    session.stickers.stickers.tab = match tab {
                        ManagerTab::Installed => StickerTab::Installed,
                        ManagerTab::Trending => StickerTab::Trending,
                        ManagerTab::Archived => StickerTab::Archived,
                    }
                }
                Kind::Masks => {
                    session.stickers.masks.tab = if tab == ManagerTab::Archived {
                        MasksTab::Archived
                    } else {
                        MasksTab::Installed
                    }
                }
            }
        }
        cx.notify();
    }

    pub(super) fn manager_set_removed(
        &mut self,
        kind: Kind,
        id: i64,
        removed: bool,
        cx: &mut Context<Self>,
    ) {
        self.manager_edits(kind).set_removed(id, removed);
        cx.notify();
    }

    /// Add on a trending or archived row. A set that is still installed
    /// and only marked removed here just loses the mark.
    pub(super) fn manager_add(&mut self, kind: Kind, id: i64, cx: &mut Context<Self>) {
        if self.manager_edits(kind).is_removed(id) {
            self.manager_edits(kind).set_removed(id, false);
            cx.notify();
            return;
        }
        match kind {
            Kind::Stickers => self.install_sticker_set(id, cx),
            Kind::Masks => self.change_mask_set(id, true, false, cx),
        }
    }

    pub(super) fn manager_dropped(
        &mut self,
        kind: Kind,
        dragged: i64,
        target: i64,
        cx: &mut Context<Self>,
    ) {
        let installed: Vec<i64> = self
            .manager_view(kind)
            .installed
            .iter()
            .map(|set| set.id)
            .collect();
        self.manager_edits(kind)
            .drag_onto(&installed, dragged, target);
        cx.notify();
    }

    /// The list was scrolled: ask for the next page near its end.
    fn manager_load_more(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let scroll = &self.pickers.sticker_manager.scroll;
        if !near_end(
            scroll.bounds().size.height,
            scroll.max_offset().y,
            scroll.offset().y,
        ) {
            return;
        }
        let view = self.manager_view(kind);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let session = &live.driver.session;
        let result = match (kind, view.tab) {
            (Kind::Stickers, ManagerTab::Trending)
                if session.stickers.stickers.trending_next_offset
                    < session.stickers.stickers.trending_total =>
            {
                live.driver.fetch_trending_stickers(true).map(drop)
            }
            (Kind::Stickers, ManagerTab::Archived)
                if session.stickers.stickers.archived_has_more =>
            {
                live.driver.fetch_archived_stickers(true).map(drop)
            }
            (Kind::Masks, ManagerTab::Archived) if session.stickers.masks.archived_has_more => {
                live.driver.more_archived_masks().map(drop)
            }
            _ => return,
        };
        if result.is_err() {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_LOAD_MORE_TRENDING,
            )));
        }
        cx.notify();
    }

    fn manager_panel(&self, kind: Kind, cx: &mut Context<Self>) -> AnyElement {
        let view = self.manager_view(kind);
        let edits = match kind {
            Kind::Stickers => &self.pickers.sticker_manager.stickers,
            Kind::Masks => &self.pickers.sticker_manager.masks,
        };
        let sets: Vec<StickerSetInfo> = match view.tab {
            ManagerTab::Installed => {
                let ids: Vec<i64> = view.installed.iter().map(|set| set.id).collect();
                edits
                    .arranged(&ids)
                    .into_iter()
                    .filter_map(|id| view.installed.iter().find(|set| set.id == id).cloned())
                    .collect()
            }
            ManagerTab::Trending => view.trending.clone(),
            ManagerTab::Archived => view.archived.clone(),
        };
        let owner = cx.entity().downgrade();
        let tab_labels = view.tabs.clone();
        let tabs = TabBar::new("stickers-manager-tabs")
            .underline()
            .children(view.tabs.iter().map(|tab| {
                let label = match (tab, kind) {
                    (ManagerTab::Installed, Kind::Stickers) => tr(keys::STICKERS_INSTALLED_TAB),
                    (ManagerTab::Installed, Kind::Masks) => tr(keys::STICKERS_MASKS_TAB),
                    (ManagerTab::Trending, _) => tr(keys::STICKERS_FEATURED_TAB),
                    (ManagerTab::Archived, _) => tr(keys::STICKERS_ARCHIVED_TAB),
                };
                Tab::new().label(label).when(
                    *tab == ManagerTab::Trending && view.unread > 0,
                    |tab| {
                        tab.suffix(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(cx.theme().primary)
                                .child(view.unread.to_string()),
                        )
                    },
                )
            }))
            .selected_index(
                view.tabs
                    .iter()
                    .position(|tab| *tab == view.tab)
                    .unwrap_or(0),
            )
            .on_click(move |ix, _, cx| {
                if let Some(tab) = tab_labels.get(*ix).copied() {
                    let _ = owner.update(cx, |this, cx| this.manager_select_tab(kind, tab, cx));
                }
            });
        let rows = sets.iter().fold(v_flex().w_full(), |rows, set| {
            rows.child(self.manager_row(
                set,
                RowState {
                    kind,
                    tab: view.tab,
                    removed: edits.is_removed(set.id),
                    pending: self.manager_set_pending(kind, set.id),
                },
                cx,
            ))
        });
        let status = if view.failed {
            Some(tr(keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN))
        } else if view.loading && sets.is_empty() {
            Some(tr(keys::CONTACTS_LOADING))
        } else if sets.is_empty() {
            Some(match (view.tab, kind) {
                (ManagerTab::Installed, Kind::Stickers) => tr(keys::Q_MEDIA_NO_STICKER_SETS_YET),
                (ManagerTab::Installed, Kind::Masks) => tr(keys::Q_MISC_NO_MASKS_YET_ADD_SOME),
                (ManagerTab::Trending, _) => tr(keys::Q_MEDIA_NO_TRENDING_STICKER_SETS),
                (ManagerTab::Archived, Kind::Stickers) => {
                    tr(keys::Q_MEDIA_NO_ARCHIVED_STICKER_SETS)
                }
                (ManagerTab::Archived, Kind::Masks) => tr(keys::Q_MISC_NO_ARCHIVED_MASKS),
            })
        } else {
            None
        };
        v_flex()
            .id("stickers-manager")
            .w(px(420.))
            .max_w_full()
            .gap_2()
            .child(tabs)
            .when_some(status, |body, status| {
                body.child(
                    div()
                        .px_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
            })
            .child(
                div()
                    .id("stickers-manager-rows")
                    .role(Role::ScrollView)
                    .aria_label(tr(keys::STICKERS_YOU_HAVE))
                    .h(px(420.))
                    .overflow_y_scroll()
                    .track_scroll(&self.pickers.sticker_manager.scroll)
                    .on_scroll_wheel(
                        cx.listener(move |this, _, _, cx| this.manager_load_more(kind, cx)),
                    )
                    .child(rows),
            )
            .into_any_element()
    }

    pub(super) fn build_stickers_manager_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let close =
            QuillShell::on_close_kind(app, shell, DialogKind::ArchivedStickers, |this, _, cx| {
                this.close_stickers_manager(cx)
            });
        app.update(cx, |this, cx| {
            let body = Rc::new(RefCell::new(Some(this.manager_panel(Kind::Stickers, cx))));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::STICKERS_YOU_HAVE)))
                .content(move |content, _, _| {
                    content.child(
                        body.borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element()),
                    )
                })
                .on_close(close)
        })
    }

    pub(super) fn build_mask_sets_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let close = QuillShell::on_close_kind(app, shell, DialogKind::MaskSets, |this, _, cx| {
            this.close_mask_sets(cx)
        });
        app.update(cx, |this, cx| {
            let body = Rc::new(RefCell::new(Some(this.manager_panel(Kind::Masks, cx))));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::SWITCH_MASKS)))
                .content(move |content, _, _| {
                    content.child(
                        body.borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element()),
                    )
                })
                .on_close(close)
        })
    }
}

/// The list is scrolled to within `LOAD_MORE_DISTANCE` of its end.
fn near_end(viewport_height: Pixels, max_offset_y: Pixels, offset_y: Pixels) -> bool {
    viewport_height > px(0.) && max_offset_y + offset_y <= px(LOAD_MORE_DISTANCE)
}

/// With no account behind the window (the screenshot demos), closing the
/// box applies its changes to the demo session itself.
fn apply_demo_commit(
    session: &mut quill::state::Session,
    kind: Kind,
    removed: &[i64],
    order: Option<&[i64]>,
) {
    let sets = match kind {
        Kind::Stickers => &mut session.stickers.stickers.sets,
        Kind::Masks => &mut session.stickers.masks.installed,
    };
    sets.retain(|set| !removed.contains(&set.id));
    if let Some(order) = order {
        sets.sort_by_key(|set| {
            order
                .iter()
                .position(|id| *id == set.id)
                .unwrap_or(usize::MAX)
        });
    }
}

crate::ui::shell::register_dialogs! {
    /// Settings > Chats > "Manage sticker sets".
    ArchivedStickers => DialogSpec::new(
        5000,
        |app| app.settings.sticker_settings_open,
        QuillApp::build_stickers_manager_dialog,
    ),
    /// The photo editor's "Manage masks".
    MaskSets => DialogSpec::new(
        5110,
        |app| app.session().is_some_and(|s| s.stickers.masks.open),
        QuillApp::build_mask_sets_dialog,
    ),
}

#[cfg(test)]
mod near_end_tests {
    use super::near_end;
    use gpui_kit::px;

    #[test]
    fn the_next_page_is_asked_for_near_the_end_only() {
        // Offsets run negative as the list scrolls down.
        assert!(!near_end(px(420.), px(1000.), px(0.)));
        assert!(!near_end(px(420.), px(1000.), px(-800.)));
        assert!(near_end(px(420.), px(1000.), px(-850.)));
        assert!(near_end(px(420.), px(1000.), px(-1000.)));
    }

    #[test]
    fn a_list_not_laid_out_yet_asks_for_nothing() {
        assert!(!near_end(px(0.), px(0.), px(0.)));
    }
}

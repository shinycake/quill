//! Custom emoji pack settings, preview and confirmed removal.
use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
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
        if let Some(live) = self.live.as_mut() {
            if live.driver.preview_emoji_pack(id).is_err() {
                self.status_note = "could not load emoji pack".into();
            }
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
        if let Some(live) = self.live.as_mut() {
            if live.driver.search_emoji_packs(&query).is_err() {
                self.status_note = "could not search emoji packs".into();
            }
        }
        cx.notify();
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
        let sets = match panel.tab {
            EmojiSetTab::Installed => &panel.installed_sets,
            EmojiSetTab::Trending => &panel.trending_sets,
            EmojiSetTab::Search => &panel.found_sets,
        };
        let mut rows = div().id("emoji-pack-rows").flex().flex_col().gap_2();
        for set in sets {
            let id = set.id;
            let installed = set.is_installed;
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
                        if let Some(live) = this.live.as_mut() {
                            if live.driver.more_trending_emoji_packs().is_err() {
                                this.status_note = "could not load more emoji packs".into();
                            }
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
            .child(tabs)
            .child(Textarea::new(&self.emoji_set_search_input))
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
                .title("Emoji Sets")
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

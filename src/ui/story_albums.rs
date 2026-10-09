//! story albums page, privacy/cover editors.

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::connect::LiveConnect;
use quill::ids::ChatId;
use quill::story_composer::StoryPrivacy;
use quill::story_page::{StoryPageOpState, parse_story_id_list};
use quill::telegram::envelope::StoryAreaKind;
/// Phase 9.5: the viewer privacy editor's state — the story plus the
/// picked level/users. Prefilled from the story's `privacy_settings`.
#[derive(Debug, Clone)]
pub(super) struct StoryPrivacyEdit {
    pub(super) chat_id: i64,
    pub(super) story_id: i32,
    pub(super) privacy: StoryPrivacy,
    pub(super) selected_user_ids: Vec<i64>,
}

impl QuillApp {
    /// Phase 9.7: open the chat story page — story albums, chat-page
    /// stories (pin/unpin), and the archive list. The three loads fire
    /// through TDLib; in demo mode the live-TDLib-required notice shows
    /// instead of a silent no-op.
    pub(super) fn open_story_page(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.story_page = Some(StoryPage::new(chat_id, window, cx));
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.get_chat_story_albums(chat_id) {
                self.status_note = format!("could not load story albums: {err:?}");
            }
            if let Err(err) = live
                .driver
                .get_chat_posted_to_chat_page_stories(chat_id, 0, 50)
            {
                self.status_note = format!("could not load chat page stories: {err:?}");
            }
            if let Err(err) = live.driver.get_chat_archived_stories(chat_id, 0, 50) {
                self.status_note = format!("could not load archived stories: {err:?}");
            }
        } else {
            self.status_note =
                "demo — story albums, chat-page stories and the archive load with live TDLib"
                    .into();
        }
        cx.notify();
    }

    /// Phase 9.7: close the chat story page.
    pub(super) fn close_story_page(&mut self, cx: &mut Context<Self>) {
        self.story_page = None;
        cx.notify();
    }

    /// Phase 9.7: open one album — the story grid loads via
    /// `getStoryAlbumStories` (offset = already-cached ids so reopening
    /// after a mutation refetches).
    pub(super) fn open_story_album(
        &mut self,
        album_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat_id = match self.story_page.as_mut() {
            Some(page) => {
                page.open_album = Some(album_id);
                page.delete_confirm = None;
                page.chat_id
            }
            None => return,
        };
        let name = self
            .session()
            .and_then(|s| s.story_albums.get(&chat_id.0))
            .and_then(|albums| albums.iter().find(|a| a.id == album_id))
            .map(|a| a.name.clone())
            .unwrap_or_default();
        if let Some(page) = self.story_page.as_mut() {
            page.rename_input.update(cx, |input, cx| {
                input.set_value(&name, window, cx);
            });
        }
        let offset = self
            .session()
            .and_then(|s| s.story_album_stories.get(&(chat_id.0, album_id)))
            .map(|ids| ids.len() as i32)
            .unwrap_or(0);
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live
                .driver
                .get_story_album_stories(chat_id, album_id, offset, 50)
            {
                self.status_note = format!("could not load album stories: {err:?}");
            }
        } else {
            self.status_note = "demo — album stories load with live TDLib".into();
        }
        cx.notify();
    }

    /// Phase 9.7: back from the opened album to the album list.
    pub(super) fn back_to_story_albums(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = self.story_page.as_mut() {
            page.open_album = None;
            page.delete_confirm = None;
        }
        cx.notify();
    }

    /// Phase 9.7: run a story-page mutation against the driver; demo mode
    /// shows the live-TDLib-required notice, driver rejections surface in
    /// `status_note`. The honest Sending/Succeeded/Failed states ride on
    /// `Session::story_page_op`.
    pub(super) fn story_page_mutate(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut LiveConnect) -> Result<(), quill::connect::ConnectSendError>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.status_note = format!("demo — {what} runs with live TDLib");
            cx.notify();
            return;
        };
        if let Err(err) = f(live) {
            self.status_note = format!("could not {what}: {err:?}");
        }
        cx.notify();
    }

    /// Phase 9.7: create an album from the page's name + story-ids inputs
    /// (`createStoryAlbum`). Ids the server marks as not addable to albums
    /// (`story.can_be_added_to_album`, `schema/td_api.tl:6724`) are skipped
    /// up front instead of being sent to fail.
    pub(super) fn create_story_album(&mut self, cx: &mut Context<Self>) {
        let (chat_id, name, story_ids) = match self.story_page.as_ref() {
            Some(page) => {
                let name = page.new_album_name.read(cx).value().trim().to_string();
                let story_ids =
                    parse_story_id_list(page.new_album_story_ids.read(cx).value().as_ref());
                (page.chat_id, name, story_ids)
            }
            None => return,
        };
        let (story_ids, skipped) = self.partition_addable_stories(chat_id, &story_ids);
        if !skipped.is_empty() {
            self.status_note =
                format!("skipped stories that can't be added to albums: {skipped:?}");
        }
        self.story_page_mutate(
            "create story album",
            |live| {
                live.driver
                    .create_story_album(chat_id, &name, &story_ids)
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: rename the opened album (`setStoryAlbumName`).
    pub(super) fn rename_story_album(&mut self, cx: &mut Context<Self>) {
        let (chat_id, album_id, name) = match self.story_page.as_ref() {
            Some(page) => match page.open_album {
                Some(album_id) => (
                    page.chat_id,
                    album_id,
                    page.rename_input.read(cx).value().trim().to_string(),
                ),
                None => return,
            },
            None => return,
        };
        self.story_page_mutate(
            "rename story album",
            |live| {
                live.driver
                    .set_story_album_name(chat_id, album_id, &name)
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: two-click album delete — first click arms the confirm,
    /// second click sends `deleteStoryAlbum`.
    pub(super) fn delete_story_album(&mut self, album_id: i32, cx: &mut Context<Self>) {
        let chat_id = match self.story_page.as_mut() {
            Some(page) if page.delete_confirm == Some(album_id) => page.chat_id,
            Some(page) => {
                page.delete_confirm = Some(album_id);
                cx.notify();
                return;
            }
            None => return,
        };
        self.story_page_mutate(
            "delete story album",
            |live| {
                live.driver
                    .delete_story_album(chat_id, album_id)
                    .map(|_| ())
            },
            cx,
        );
        if let Some(page) = self.story_page.as_mut() {
            page.delete_confirm = None;
            if page.open_album == Some(album_id) {
                page.open_album = None;
            }
        }
    }

    /// Phase 9.7: move an album up/down in the list
    /// (`reorderStoryAlbums` with the full new order).
    pub(super) fn move_story_album(&mut self, album_id: i32, up: bool, cx: &mut Context<Self>) {
        let (chat_id, order) = match self.session().zip(self.story_page.as_ref()) {
            Some((session, page)) => {
                let mut ids: Vec<i32> = session
                    .story_albums
                    .get(&page.chat_id.0)
                    .map(|albums| albums.iter().map(|a| a.id).collect())
                    .unwrap_or_default();
                let pos = ids.iter().position(|id| *id == album_id);
                let Some(pos) = pos else { return };
                let swap = if up {
                    pos.checked_sub(1)
                } else {
                    Some(pos + 1)
                };
                let Some(swap) = swap else { return };
                if swap >= ids.len() {
                    return;
                }
                ids.swap(pos, swap);
                (page.chat_id, ids)
            }
            None => return,
        };
        self.story_page_mutate(
            "reorder story albums",
            |live| {
                live.driver
                    .reorder_story_albums(chat_id, &order)
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: split story ids into (addable, skipped) using the parsed
    /// `story.can_be_added_to_album` (`schema/td_api.tl:6724`). Ids not in
    /// the local story cache are treated as addable — the server still
    /// validates them.
    pub(super) fn partition_addable_stories(
        &self,
        chat_id: ChatId,
        story_ids: &[i32],
    ) -> (Vec<i32>, Vec<i32>) {
        let stories = self.live.as_ref().map(|live| &live.driver.session.stories);
        story_ids.iter().copied().partition(|id| {
            stories
                .and_then(|cached| cached.get(&(chat_id.0, *id)))
                .map(|story| story.can_be_added_to_album)
                .unwrap_or(true)
        })
    }

    /// Phase 9.7: add the ids from the album's input to the opened album
    /// (`addStoryAlbumStories`). Ids the server marks as not addable to
    /// albums are skipped up front (see `partition_addable_stories`).
    pub(super) fn add_stories_to_album(&mut self, cx: &mut Context<Self>) {
        let (chat_id, album_id, story_ids) = match self.story_page.as_ref() {
            Some(page) => match page.open_album {
                Some(album_id) => (
                    page.chat_id,
                    album_id,
                    parse_story_id_list(page.add_story_ids.read(cx).value().as_ref()),
                ),
                None => return,
            },
            None => return,
        };
        if story_ids.is_empty() {
            self.status_note = "enter at least one story id".into();
            cx.notify();
            return;
        }
        let (story_ids, skipped) = self.partition_addable_stories(chat_id, &story_ids);
        if story_ids.is_empty() {
            self.status_note =
                format!("none of these stories can be added to an album (skipped: {skipped:?})");
            cx.notify();
            return;
        }
        if !skipped.is_empty() {
            self.status_note =
                format!("skipped stories that can't be added to albums: {skipped:?}");
        }
        self.story_page_mutate(
            "add stories to album",
            |live| {
                live.driver
                    .add_story_album_stories(chat_id, album_id, &story_ids)
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: remove one story from the opened album
    /// (`removeStoryAlbumStories`).
    pub(super) fn remove_story_from_album(
        &mut self,
        album_id: i32,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        let chat_id = match self.story_page.as_ref() {
            Some(page) => page.chat_id,
            None => return,
        };
        self.story_page_mutate(
            "remove story from album",
            |live| {
                live.driver
                    .remove_story_album_stories(chat_id, album_id, &[story_id])
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: move one story to the beginning of the opened album
    /// (`reorderStoryAlbumStories` — the listed ids move to the front).
    pub(super) fn move_story_to_album_top(
        &mut self,
        album_id: i32,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        let chat_id = match self.story_page.as_ref() {
            Some(page) => page.chat_id,
            None => return,
        };
        self.story_page_mutate(
            "reorder album stories",
            |live| {
                live.driver
                    .reorder_story_album_stories(chat_id, album_id, &[story_id])
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: next archive page (`getChatArchivedStories` from the
    /// smallest loaded id).
    pub(super) fn load_more_archived_stories(&mut self, cx: &mut Context<Self>) {
        let (chat_id, from_story_id) = match self.story_page.as_ref().zip(self.session()) {
            Some((page, session)) => match session.archived_stories.get(&page.chat_id.0) {
                Some(archived) => (page.chat_id, archived.next_from_story_id.unwrap_or(0)),
                None => (page.chat_id, 0),
            },
            None => return,
        };
        self.story_page_mutate(
            "load archived stories",
            |live| {
                live.driver
                    .get_chat_archived_stories(chat_id, from_story_id, 50)
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: next chat-page-stories page (from the smallest loaded
    /// id).
    pub(super) fn load_more_chat_page_stories(&mut self, cx: &mut Context<Self>) {
        let (chat_id, from_story_id) = match self.story_page.as_ref().zip(self.session()) {
            Some((page, session)) => match session.chat_page_stories.get(&page.chat_id.0) {
                Some(chat_page) => (
                    page.chat_id,
                    chat_page.story_ids.iter().copied().min().unwrap_or(0),
                ),
                None => (page.chat_id, 0),
            },
            None => return,
        };
        self.story_page_mutate(
            "load chat page stories",
            |live| {
                live.driver
                    .get_chat_posted_to_chat_page_stories(chat_id, from_story_id, 50)
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.7: pin/unpin one story — `setChatPinnedStories` takes the
    /// full new list, so the current pinned ids are adjusted locally and
    /// the `ok` answer applies them (correlated via the pending request).
    pub(super) fn toggle_story_pin(&mut self, story_id: i32, cx: &mut Context<Self>) {
        let (chat_id, pinned) = match self.story_page.as_ref().zip(self.session()) {
            Some((page, session)) => {
                let mut pinned: Vec<i32> = session
                    .chat_page_stories
                    .get(&page.chat_id.0)
                    .map(|state| state.pinned_story_ids.clone())
                    .unwrap_or_default();
                if pinned.contains(&story_id) {
                    pinned.retain(|id| *id != story_id);
                } else {
                    pinned.push(story_id);
                }
                (page.chat_id, pinned)
            }
            None => return,
        };
        self.story_page_mutate(
            "pin stories",
            |live| {
                live.driver
                    .set_chat_pinned_stories(chat_id, &pinned)
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Phase 9.5: one-line status for posted-story management —
    /// pending spinner or the sanitized failure. `None` when idle.
    pub(super) fn story_manage_status(&self) -> Option<String> {
        let manage = self.session()?.story_manage.clone();
        if manage.pending {
            Some("Saving…".into())
        } else {
            manage.error
        }
    }

    /// Phase 9.5: toggle the cover-frame editor for a video story.
    pub(super) fn toggle_story_cover_edit(
        &mut self,
        chat_id: i64,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        if self.story_cover_target == Some((chat_id, story_id)) {
            self.story_cover_target = None;
        } else {
            self.story_cover_target = Some((chat_id, story_id));
            self.story_cover_sent = false;
        }
        cx.notify();
    }

    /// Phase 9.5: the cover-frame editor row — seconds input + Set.
    pub(super) fn story_cover_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        // Review fix-up: shared `story_manage.pending` slot — no second
        // op while one is in flight.
        let manage_busy = self.session().is_some_and(|s| s.story_manage.pending);
        div()
            .flex()
            .gap_2()
            .items_center()
            .w(px(360.))
            .child(
                div().flex_1().child(
                    Textarea::new(&self.story_cover_input)
                        .aria_label("Story cover file path")
                        .h(px(32.)),
                ),
            )
            .child(
                Button::new("story-cover-set")
                    .label(if self.story_cover_sent {
                        "Saving…"
                    } else {
                        "Set"
                    })
                    .disabled(manage_busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.story_cover_save(cx);
                    })),
            )
            .into_any_element()
    }

    /// Phase 9.5: parse the seconds input and send `editStoryCover`
    /// (`td_api.tl:13738`). The driver gates on `can_be_edited`; the
    /// tick closes the editor on success.
    pub(super) fn story_cover_save(&mut self, cx: &mut Context<Self>) {
        if self.story_cover_sent {
            return;
        }
        let Some((chat_id, story_id)) = self.story_cover_target else {
            return;
        };
        // Demo mode: surface the same notice as `story_composer_save_edit`
        // — management calls need live TDLib.
        if self.live.is_none() {
            if let Some(demo) = self.demo_session.as_mut() {
                demo.story_manage.error = Some("demo — editing runs with live TDLib".into());
            }
            cx.notify();
            return;
        }
        let raw = self.story_cover_input.read(cx).value().trim().to_string();
        let timestamp: f64 = match raw.parse() {
            Ok(seconds) if seconds >= 0.0 => seconds,
            _ => {
                if let Some(live) = self.live.as_mut() {
                    live.driver.session.story_manage.error =
                        Some("Enter the cover time in seconds (0 or more)".into());
                }
                cx.notify();
                return;
            }
        };
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .edit_story_cover(ChatId(chat_id), story_id, timestamp)
            {
                Ok(_) => self.story_cover_sent = true,
                Err(_) => {
                    live.driver.session.story_manage.error =
                        Some("Could not send the cover request".into());
                }
            }
        }
        cx.notify();
    }

    /// Phase 9.5: toggle the privacy editor, prefilled from the story's
    /// current `privacy_settings` when the schema type is known.
    pub(super) fn toggle_story_privacy_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.story_privacy_edit.is_some() {
            self.story_privacy_edit = None;
            return;
        }
        let Some(item) = self.story_viewer.current().cloned() else {
            return;
        };
        let (privacy, selected_user_ids) = self
            .session()
            .and_then(|s| s.stories.get(&(item.chat_id.0, item.story_id)))
            .and_then(|story| story.privacy_settings.as_ref())
            .and_then(StoryPrivacy::from_settings_json)
            .unwrap_or((StoryPrivacy::Everyone, Vec::new()));
        self.story_privacy_edit = Some(StoryPrivacyEdit {
            chat_id: item.chat_id.0,
            story_id: item.story_id,
            privacy,
            selected_user_ids,
        });
        self.story_privacy_sent = false;
        self.story_privacy_user_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Phase 9.5: toggle a contact in the privacy editor's
    /// "Selected users" picker.
    pub(super) fn toggle_story_privacy_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(edit) = self.story_privacy_edit.as_mut() {
            if edit.selected_user_ids.contains(&user_id) {
                edit.selected_user_ids.retain(|id| *id != user_id);
            } else {
                edit.selected_user_ids.push(user_id);
            }
        }
        cx.notify();
    }

    /// Phase 9.5: the privacy editor panel — the 4-way selector plus
    /// the contact picker for "Selected users".
    pub(super) fn story_privacy_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let edit = match self.story_privacy_edit.as_ref() {
            Some(edit) => edit,
            None => return div().into_any_element(),
        };
        let mut panel = div().flex().flex_col().gap_1().w(px(360.)).child(
            div()
                .text_sm()
                .font_semibold()
                .text_color(text_bright())
                .child("Who can see this story"),
        );
        // Phase 6: kit RadioGroup (was: buttons with a ☑/☐ prefix).
        // Controlled: the chosen index writes the value.
        let privacy_selected = StoryPrivacy::ALL
            .iter()
            .position(|option| edit.privacy == *option);
        panel = panel.child(
            RadioGroup::vertical("story-privacy")
                .selected_index(privacy_selected)
                .children(StoryPrivacy::ALL.iter().map(|option| {
                    Radio::new(format!("story-privacy-{}", option.label())).label(option.label())
                }))
                .on_click(cx.listener(move |this, &ix, _, cx| {
                    if let Some(edit) = this.story_privacy_edit.as_mut() {
                        edit.privacy = StoryPrivacy::ALL[ix];
                    }
                    cx.notify();
                })),
        );
        if edit.privacy == StoryPrivacy::SelectedUsers {
            let query = self.story_privacy_user_search.read(cx).value();
            let rows = self.g1_contact_rows(&query, cx);
            let selected = edit.selected_user_ids.clone();
            let mut list = div()
                .id("story-privacy-users")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(140.))
                .overflow_y_scroll();
            if rows.is_empty() {
                list = list.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("No contacts found"),
                );
            }
            for row in rows.iter().take(50) {
                list = list.child(self.g1_contact_checkbox(
                    "story-privacy".to_string(),
                    row,
                    selected.contains(&row.user_id),
                    row.user_id,
                    cx,
                ));
            }
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        Textarea::new(&self.story_privacy_user_search)
                            .aria_label("Search story privacy exceptions")
                            .h(px(32.)),
                    )
                    .child(list),
            );
        }
        let busy = self.story_privacy_sent;
        // Review fix-up: shared `story_manage.pending` slot — no second
        // op while one is in flight.
        let manage_busy = self.session().is_some_and(|s| s.story_manage.pending);
        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("story-privacy-save")
                        .label(if busy { "Saving…" } else { "Save" })
                        .disabled(manage_busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.story_privacy_save(cx);
                        })),
                )
                .child(
                    Button::new("story-privacy-cancel")
                        .label("Cancel")
                        .ghost()
                        .text_color(text_bright())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.story_privacy_edit = None;
                            cx.notify();
                        })),
                ),
        );
        panel.into_any_element()
    }

    /// Phase 9.5: send `setStoryPrivacySettings` (`td_api.tl:13743`)
    /// for the edited story. The driver gates on
    /// `can_set_privacy_settings`; the tick closes the panel on
    /// success.
    pub(super) fn story_privacy_save(&mut self, cx: &mut Context<Self>) {
        if self.story_privacy_sent {
            return;
        }
        let Some(edit) = self.story_privacy_edit.clone() else {
            return;
        };
        // Demo mode: surface the same notice as `story_composer_save_edit`
        // — management calls need live TDLib.
        if self.live.is_none() {
            if let Some(demo) = self.demo_session.as_mut() {
                demo.story_manage.error = Some("demo — editing runs with live TDLib".into());
            }
            cx.notify();
            return;
        }
        if edit.privacy == StoryPrivacy::SelectedUsers && edit.selected_user_ids.is_empty() {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.story_manage.error =
                    Some("Pick at least one user for \"Selected users\"".into());
            }
            cx.notify();
            return;
        }
        let settings = edit.privacy.settings_json(&edit.selected_user_ids);
        if let Some(live) = self.live.as_mut() {
            match live.driver.set_story_privacy_settings(
                ChatId(edit.chat_id),
                edit.story_id,
                settings,
            ) {
                Ok(_) => self.story_privacy_sent = true,
                Err(_) => {
                    live.driver.session.story_manage.error =
                        Some("Could not send the privacy request".into());
                }
            }
        }
        cx.notify();
    }

    /// Phase 9.8: shared map-pin label for location/venue areas — "📍"
    /// plus the first non-empty of the given parts (bare "📍" when all
    /// are empty). Used by both the chip labels and the tap status
    /// notes.
    pub(super) fn story_area_pin_label(parts: &[&str]) -> String {
        let detail = parts
            .iter()
            .copied()
            .find(|part| !part.is_empty())
            .unwrap_or("");
        if detail.is_empty() {
            "📍".to_string()
        } else {
            format!("📍 {detail}")
        }
    }

    /// Phase 9.8: the chip label for a story area kind — what the official
    /// clients paint on the area (glyph + one-line summary).
    pub(super) fn story_area_label(kind: &StoryAreaKind) -> String {
        match kind {
            StoryAreaKind::Location { address, location } => {
                Self::story_area_pin_label(&[address, &location.coords_label()])
            }
            StoryAreaKind::Venue { title, .. } => Self::story_area_pin_label(&[title]),
            StoryAreaKind::SuggestedReaction { emoji, total_count } => {
                if *total_count > 0 {
                    format!("{emoji} {total_count}")
                } else {
                    emoji.clone()
                }
            }
            StoryAreaKind::Message { .. } => "💬".to_string(),
            StoryAreaKind::Link { .. } => "🔗".to_string(),
            StoryAreaKind::Weather { temperature, emoji } => {
                format!("{emoji} {temperature:.0}°")
            }
            StoryAreaKind::Gift { gift_name } => format!("🎁 {gift_name}"),
            StoryAreaKind::Unsupported { .. } => "·".to_string(),
        }
    }

    /// Phase 9.7: the chat story page overlay — story albums (list +
    /// opened album), chat-page stories with pin/unpin, and the paginated
    /// archive list. The status line renders the honest
    /// `Session::story_page_op` state (Sending / Succeeded / Failed).
    pub(super) fn story_page_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(page) = self.story_page.as_ref() else {
            return div().into_any_element();
        };
        let chat_id = page.chat_id;
        let open_album = page.open_album;
        let delete_confirm = page.delete_confirm;
        let new_album_name = page.new_album_name.clone();
        let new_album_story_ids = page.new_album_story_ids.clone();
        let rename_input = page.rename_input.clone();
        let add_story_ids = page.add_story_ids.clone();
        let title = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|c| c.title.clone())
            .unwrap_or_else(|| format!("Chat {}", chat_id.0));
        // Honest operation state: Checking shows as `Sending` (the load
        // already fired), mutations label their action, failures carry
        // the TDLib reason.
        let op_status = self
            .session()
            .and_then(|s| s.story_page_op.clone())
            .map(|op| match op.state {
                StoryPageOpState::Checking => format!("{}…", op.label),
                StoryPageOpState::Sending => format!("{}…", op.label),
                StoryPageOpState::Succeeded => format!("{} — done", op.label),
                StoryPageOpState::Failed(reason) => format!("{} — failed: {reason}", op.label),
            });
        let label_for = |story_id: i32| {
            let caption = self
                .session()
                .and_then(|s| s.stories.get(&(chat_id.0, story_id)))
                .map(|story| story.caption.clone())
                .unwrap_or_default();
            let snippet: String = caption.chars().take(40).collect();
            if snippet.is_empty() {
                format!("Story {story_id}")
            } else {
                format!("Story {story_id} — {snippet}")
            }
        };

        let mut body = div().flex().flex_col().gap_3();
        if let Some(album_id) = open_album {
            // ---- Opened album: rename, delete, stories, add. ----
            let name = self
                .session()
                .and_then(|s| s.story_albums.get(&chat_id.0))
                .and_then(|albums| albums.iter().find(|a| a.id == album_id))
                .map(|a| a.name.clone())
                .unwrap_or_else(|| format!("Album {album_id}"));
            let story_ids: Vec<i32> = self
                .session()
                .and_then(|s| s.story_album_stories.get(&(chat_id.0, album_id)))
                .cloned()
                .unwrap_or_default();
            let mut detail = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("story-page-back")
                                .label("← Albums")
                                .ghost()
                                .text_color(rgb(0xffffff))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.back_to_story_albums(cx);
                                })),
                        )
                        .child(
                            div()
                                .font_semibold()
                                .text_color(rgb(0xffffff))
                                .child(name.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Textarea::new(&rename_input)
                                .aria_label("Story album name")
                                .h(px(36.))
                                .flex_1(),
                        )
                        .child(Button::new("story-page-rename").label("Rename").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.rename_story_album(cx);
                            }),
                        )),
                )
                .child(
                    Button::new("story-page-delete")
                        .label(if delete_confirm == Some(album_id) {
                            "Confirm delete"
                        } else {
                            "Delete album"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.delete_story_album(album_id, cx);
                        })),
                );
            for story_id in story_ids {
                let label = label_for(story_id);
                detail = detail.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(0x21262d))
                        .child(div().text_sm().text_color(rgb(0xffffff)).child(label))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new(("story-page-album-top", story_id as u64))
                                        .label("↑ Top")
                                        .ghost()
                                        .text_color(rgb(0xffffff))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_story_to_album_top(album_id, story_id, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("story-page-album-remove", story_id as u64))
                                        .label("Remove")
                                        .ghost()
                                        .text_color(rgb(0xffffff))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.remove_story_from_album(album_id, story_id, cx);
                                        })),
                                ),
                        ),
                );
            }
            detail = detail.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Textarea::new(&add_story_ids)
                            .aria_label("Story identifiers to add")
                            .h(px(36.))
                            .flex_1(),
                    )
                    .child(Button::new("story-page-add-stories").label("Add").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.add_stories_to_album(cx);
                        }),
                    )),
            );
            body = body.child(detail);
        } else {
            // ---- Album list + create form. ----
            let albums: Vec<(i32, String)> = self
                .session()
                .and_then(|s| s.story_albums.get(&chat_id.0))
                .map(|albums| albums.iter().map(|a| (a.id, a.name.clone())).collect())
                .unwrap_or_default();
            let mut list = div().flex().flex_col().gap_2().child(
                div()
                    .font_semibold()
                    .text_color(rgb(0xffffff))
                    .child("Albums"),
            );
            for (index, (album_id, name)) in albums.iter().enumerate() {
                let album_id = *album_id;
                let at_top = index == 0;
                let at_bottom = index + 1 == albums.len();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(0x21262d))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .child(name.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new(("story-page-open", album_id as u64))
                                        .label("Open")
                                        .ghost()
                                        .text_color(rgb(0xffffff))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_story_album(album_id, window, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("story-page-up", album_id as u64))
                                        .label("↑")
                                        .ghost()
                                        .text_color(rgb(0xffffff))
                                        .disabled(at_top)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_story_album(album_id, true, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("story-page-down", album_id as u64))
                                        .label("↓")
                                        .ghost()
                                        .text_color(rgb(0xffffff))
                                        .disabled(at_bottom)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_story_album(album_id, false, cx);
                                        })),
                                ),
                        ),
                );
            }
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_sm().text_color(rgb(0x9aa0a6)).child("New album"))
                    .child(
                        Textarea::new(&new_album_name)
                            .aria_label("New story album name")
                            .h(px(36.)),
                    )
                    .child(
                        Textarea::new(&new_album_story_ids)
                            .aria_label("Story identifiers for new album")
                            .h(px(36.)),
                    )
                    .child(
                        Button::new("story-page-create")
                            .label("Create album")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.create_story_album(cx);
                            })),
                    ),
            );
            // ---- Chat-page stories with pin/unpin. ----
            let (pinned, chat_page_ids): (Vec<i32>, Vec<i32>) = self
                .session()
                .and_then(|s| s.chat_page_stories.get(&chat_id.0))
                .map(|state| (state.pinned_story_ids.clone(), state.story_ids.clone()))
                .unwrap_or_default();
            let mut chat_page = div().flex().flex_col().gap_2().child(
                div()
                    .font_semibold()
                    .text_color(rgb(0xffffff))
                    .child("Chat page stories"),
            );
            for story_id in &chat_page_ids {
                let story_id = *story_id;
                let label = label_for(story_id);
                let is_pinned = pinned.contains(&story_id);
                chat_page = chat_page.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(0x21262d))
                        .child(div().text_sm().text_color(rgb(0xffffff)).child(label))
                        .child(
                            Button::new(("story-page-pin", story_id as u64))
                                .label(if is_pinned { "Unpin" } else { "Pin" })
                                .ghost()
                                .text_color(rgb(0xffffff))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_story_pin(story_id, cx);
                                })),
                        ),
                );
            }
            chat_page = chat_page.child(
                Button::new("story-page-load-chat-page")
                    .label("Load more")
                    .ghost()
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.load_more_chat_page_stories(cx);
                    })),
            );
            // ---- Archive. ----
            let archived_ids: Vec<i32> = self
                .session()
                .and_then(|s| s.archived_stories.get(&chat_id.0))
                .map(|state| state.story_ids.clone())
                .unwrap_or_default();
            let mut archive = div().flex().flex_col().gap_2().child(
                div()
                    .font_semibold()
                    .text_color(rgb(0xffffff))
                    .child("Archive"),
            );
            for story_id in &archived_ids {
                let label = label_for(*story_id);
                archive = archive.child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(0x21262d))
                        .child(div().text_sm().text_color(rgb(0xffffff)).child(label)),
                );
            }
            archive = archive.child(
                Button::new("story-page-load-archive")
                    .label("Load more")
                    .ghost()
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.load_more_archived_stories(cx);
                    })),
            );
            body = body.child(list).child(chat_page).child(archive);
        }

        div()
            .id("story-page-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("story-page-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_story_page(cx);
                    })),
            )
            .child(
                div()
                    .id("story-page-panel")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .max_w(px(560.))
                    .max_h_full()
                    .overflow_y_scroll()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(rgb(0xffffff))
                                    .child(format!("{title} — Stories")),
                            )
                            .child(
                                Button::new("story-page-close")
                                    .icon(gpui_kit::assets::IconName::X)
                                    .tooltip("Close")
                                    .accessibility_label("Close")
                                    .ghost()
                                    .text_color(rgb(0xffffff))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_story_page(cx);
                                    })),
                            ),
                    )
                    .when_some(op_status, |this, status| {
                        this.child(div().text_sm().text_color(rgb(0x9aa0a6)).child(status))
                    })
                    .child(body),
            )
            .into_any_element()
    }
}

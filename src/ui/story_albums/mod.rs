//! story albums page, privacy/cover editors.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::{user_error_doing, user_error_for};
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::connect::LiveConnect;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::story_composer::StoryPrivacy;
use quill::story_album_picker::AlbumPickerTarget;
use quill::story_page::StoryPageOpState;
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
        self.stories.page = Some(StoryPage::new(chat_id, window, cx));
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.get_chat_story_albums(chat_id) {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_STORY_ALBUMS,
                    &err,
                )));
            }
            if let Err(err) = live
                .driver
                .get_chat_posted_to_chat_page_stories(chat_id, 0, 50)
            {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_CHAT_PAGE_STORIES,
                    &err,
                )));
            }
            if let Err(err) = live.driver.get_chat_archived_stories(chat_id, 0, 50) {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_ARCHIVED_STORIES,
                    &err,
                )));
            }
        } else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_DEMO_STORY_ALBUMS_CHAT_PAGE,
            )));
        }
        cx.notify();
    }

    /// Phase 9.7: close the chat story page.
    pub(super) fn close_story_page(&mut self, cx: &mut Context<Self>) {
        self.stories.page = None;
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
        let chat_id = match self.stories.page.as_mut() {
            Some(page) => {
                page.open_album = Some(album_id);
                page.delete_confirm = None;
                page.chat_id
            }
            None => return,
        };
        let name = self
            .session()
            .and_then(|s| s.stories.albums.get(&chat_id.0))
            .and_then(|albums| albums.iter().find(|a| a.id == album_id))
            .map(|a| a.name.clone())
            .unwrap_or_default();
        if let Some(page) = self.stories.page.as_mut() {
            page.rename_input.update(cx, |input, cx| {
                input.set_value(&name, window, cx);
            });
        }
        let offset = self
            .session()
            .and_then(|s| s.stories.album_stories.get(&(chat_id.0, album_id)))
            .map(|ids| ids.len() as i32)
            .unwrap_or(0);
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live
                .driver
                .get_story_album_stories(chat_id, album_id, offset, 50)
            {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_ALBUM_STORIES,
                    &err,
                )));
            }
        } else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_DEMO_ALBUM_STORIES_LOAD_WITH,
            )));
        }
        cx.notify();
    }

    /// Phase 9.7: back from the opened album to the album list.
    pub(super) fn back_to_story_albums(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = self.stories.page.as_mut() {
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
            self.connection.set_note(StatusNote::silent(tr_with(
                keys::Q_MEDIA_DEMO_RUNS_WITH_LIVE_TDLIB,
                &[("what", what)],
            )));
            cx.notify();
            return;
        };
        if let Err(err) = f(live) {
            self.connection
                .set_note(StatusNote::error(user_error_doing(what, &err)));
        }
        cx.notify();
    }

    /// Phase 9.7: create an album from the page's name input and the
    /// stories ticked in the picker (`createStoryAlbum`). Ids the server
    /// marks as not addable to albums (`story.can_be_added_to_album`,
    /// `schema/td_api.tl:6724`) are skipped up front instead of being sent
    /// to fail.
    pub(super) fn create_story_album(&mut self, cx: &mut Context<Self>) {
        let (chat_id, name, story_ids) = match self.stories.page.as_ref() {
            Some(page) => {
                let name = page.new_album_name.read(cx).value().trim().to_string();
                let story_ids = match page.picker.as_ref() {
                    Some(picker) if picker.target == AlbumPickerTarget::NewAlbum => {
                        picker.selected_ids()
                    }
                    _ => Vec::new(),
                };
                (page.chat_id, name, story_ids)
            }
            None => return,
        };
        if let Some(page) = self.stories.page.as_mut() {
            page.picker = None;
        }
        let (story_ids, skipped) = self.partition_addable_stories(chat_id, &story_ids);
        if !skipped.is_empty() {
            self.connection.set_note(StatusNote::info(trn_with(
                keys::Q_NOTES_STORIES_SKIPPED,
                skipped.len(),
                &[("count", &skipped.len().to_string())],
            )));
        }
        self.story_page_mutate(
            tr(keys::Q_MEDIA_CREATE_STORY_ALBUM),
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
        let (chat_id, album_id, name) = match self.stories.page.as_ref() {
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
            tr(keys::Q_MEDIA_RENAME_STORY_ALBUM),
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
        let chat_id = match self.stories.page.as_mut() {
            Some(page) if page.delete_confirm == Some(album_id) => page.chat_id,
            Some(page) => {
                page.delete_confirm = Some(album_id);
                cx.notify();
                return;
            }
            None => return,
        };
        self.story_page_mutate(
            tr(keys::Q_MEDIA_DELETE_STORY_ALBUM),
            |live| {
                live.driver
                    .delete_story_album(chat_id, album_id)
                    .map(|_| ())
            },
            cx,
        );
        if let Some(page) = self.stories.page.as_mut() {
            page.delete_confirm = None;
            if page.open_album == Some(album_id) {
                page.open_album = None;
            }
        }
    }

    /// Phase 9.7: move an album up/down in the list
    /// (`reorderStoryAlbums` with the full new order).
    pub(super) fn move_story_album(&mut self, album_id: i32, up: bool, cx: &mut Context<Self>) {
        let (chat_id, order) = match self.session().zip(self.stories.page.as_ref()) {
            Some((session, page)) => {
                let mut ids: Vec<i32> = session
                    .stories
                    .albums
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
            tr(keys::Q_MEDIA_REORDER_STORY_ALBUMS),
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
        let stories = self
            .live
            .as_ref()
            .map(|live| &live.driver.session.stories.stories);
        story_ids.iter().copied().partition(|id| {
            stories
                .and_then(|cached| cached.get(&(chat_id.0, *id)))
                .map(|story| story.can_be_added_to_album)
                .unwrap_or(true)
        })
    }

    /// tdesktop `EditAlbumBox` save: send the picker's changes to the
    /// opened album, adds then removes (`addStoryAlbumStories`,
    /// `removeStoryAlbumStories`). Ids the server marks as not addable to
    /// albums are skipped up front (see `partition_addable_stories`).
    pub(super) fn save_album_picker(&mut self, cx: &mut Context<Self>) {
        let (chat_id, album_id, added, removed) = match self.stories.page.as_ref() {
            Some(page) => match page.picker.as_ref() {
                Some(picker) => match picker.target {
                    AlbumPickerTarget::Album(album_id) => {
                        (page.chat_id, album_id, picker.added(), picker.removed())
                    }
                    AlbumPickerTarget::NewAlbum => return,
                },
                None => return,
            },
            None => return,
        };
        if added.is_empty() && removed.is_empty() {
            self.close_album_picker(cx);
            return;
        }
        let (added, skipped) = self.partition_addable_stories(chat_id, &added);
        if !skipped.is_empty() {
            self.connection.set_note(StatusNote::info(trn_with(
                keys::Q_NOTES_STORIES_SKIPPED,
                skipped.len(),
                &[("count", &skipped.len().to_string())],
            )));
        }
        if added.is_empty() && removed.is_empty() {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_NOTES_NO_STORY_CAN_BE_ADDED)));
            cx.notify();
            return;
        }
        self.close_album_picker(cx);
        self.story_page_mutate(
            tr(keys::Q_MEDIA_ADD_STORIES_TO_ALBUM),
            |live| {
                if !added.is_empty() {
                    live.driver
                        .add_story_album_stories(chat_id, album_id, &added)?;
                }
                if !removed.is_empty() {
                    live.driver
                        .remove_story_album_stories(chat_id, album_id, &removed)?;
                }
                Ok(())
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
        let chat_id = match self.stories.page.as_ref() {
            Some(page) => page.chat_id,
            None => return,
        };
        self.story_page_mutate(
            tr(keys::Q_MEDIA_REMOVE_STORY_FROM_ALBUM),
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
        let chat_id = match self.stories.page.as_ref() {
            Some(page) => page.chat_id,
            None => return,
        };
        self.story_page_mutate(
            tr(keys::Q_MEDIA_REORDER_ALBUM_STORIES),
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
        let (chat_id, from_story_id) = match self.stories.page.as_ref().zip(self.session()) {
            Some((page, session)) => match session.stories.archived_stories.get(&page.chat_id.0) {
                Some(archived) => (page.chat_id, archived.next_from_story_id.unwrap_or(0)),
                None => (page.chat_id, 0),
            },
            None => return,
        };
        self.story_page_mutate(
            tr(keys::Q_MEDIA_LOAD_ARCHIVED_STORIES),
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
        let (chat_id, from_story_id) = match self.stories.page.as_ref().zip(self.session()) {
            Some((page, session)) => match session.stories.chat_page_stories.get(&page.chat_id.0) {
                Some(chat_page) => (
                    page.chat_id,
                    chat_page.story_ids.iter().copied().min().unwrap_or(0),
                ),
                None => (page.chat_id, 0),
            },
            None => return,
        };
        self.story_page_mutate(
            tr(keys::Q_MEDIA_LOAD_CHAT_PAGE_STORIES),
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
        let (chat_id, pinned) = match self.stories.page.as_ref().zip(self.session()) {
            Some((page, session)) => {
                let mut pinned: Vec<i32> = session
                    .stories
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
            tr(keys::Q_MEDIA_PIN_STORIES),
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
        let manage = self.session()?.stories.manage.clone();
        if manage.pending {
            Some(tr(keys::Q_MEDIA_SAVING).into())
        } else {
            manage.error
        }
    }

    /// Phase 9.5: toggle the privacy editor, prefilled from the story's
    /// current `privacy_settings` when the schema type is known.
    pub(super) fn toggle_story_privacy_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.stories.privacy_edit.is_some() {
            self.stories.privacy_edit = None;
            return;
        }
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let (privacy, selected_user_ids) = self
            .session()
            .and_then(|s| s.stories.stories.get(&(item.chat_id.0, item.story_id)))
            .and_then(|story| story.privacy_settings.as_ref())
            .and_then(StoryPrivacy::from_settings_json)
            .unwrap_or((StoryPrivacy::Everyone, Vec::new()));
        self.stories.privacy_edit = Some(StoryPrivacyEdit {
            chat_id: item.chat_id.0,
            story_id: item.story_id,
            privacy,
            selected_user_ids,
        });
        self.stories.privacy_sent = false;
        self.stories
            .privacy_user_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Phase 9.5: toggle a contact in the privacy editor's
    /// "Selected users" picker.
    pub(super) fn toggle_story_privacy_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(edit) = self.stories.privacy_edit.as_mut() {
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
        let edit = match self.stories.privacy_edit.as_ref() {
            Some(edit) => edit,
            None => return div().into_any_element(),
        };
        let mut panel = div().flex().flex_col().gap_1().w(px(360.)).child(
            div()
                .text_sm()
                .font_semibold()
                .text_color(text_bright())
                .child(tr(keys::Q_MEDIA_WHO_CAN_SEE_THIS_STORY)),
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
                    if let Some(edit) = this.stories.privacy_edit.as_mut()
                        && let Some(&privacy) = StoryPrivacy::ALL.get(ix)
                    {
                        edit.privacy = privacy;
                    }
                    cx.notify();
                })),
        );
        if edit.privacy == StoryPrivacy::CloseFriends {
            // B14: the list behind "Close friends" is edited here too.
            panel = panel.child(
                Button::new("story-privacy-edit-close-friends")
                    .label(tr(keys::Q_MEDIA_EDIT_CLOSE_FRIENDS))
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_close_friends_editor(window, cx);
                    })),
            );
        }
        if edit.privacy == StoryPrivacy::SelectedUsers {
            let query = self.stories.privacy_user_search.read(cx).value();
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
                        .child(tr(keys::Q_MEDIA_NO_CONTACTS_FOUND)),
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
                        Textarea::new(&self.stories.privacy_user_search)
                            .aria_label(tr(keys::Q_MEDIA_SEARCH_STORY_PRIVACY_EXCEPTIONS))
                            .h(px(32.)),
                    )
                    .child(list),
            );
        }
        let busy = self.stories.privacy_sent;
        // Review fix-up: shared `story_manage.pending` slot — no second
        // op while one is in flight.
        let manage_busy = self.session().is_some_and(|s| s.stories.manage.pending);
        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("story-privacy-save")
                        .label(if busy {
                            tr(keys::Q_MEDIA_SAVING)
                        } else {
                            tr(keys::SETTINGS_SAVE)
                        })
                        .disabled(manage_busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.story_privacy_save(cx);
                        })),
                )
                .child(
                    Button::new("story-privacy-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .text_color(text_bright())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.stories.privacy_edit = None;
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
        if self.stories.privacy_sent {
            return;
        }
        let Some(edit) = self.stories.privacy_edit.clone() else {
            return;
        };
        // Demo mode: surface the same notice as `story_composer_save_edit`
        // — management calls need live TDLib.
        if self.live.is_none() {
            if let Some(demo) = self.demo_session.as_mut() {
                demo.stories.manage.error =
                    Some(tr(keys::Q_MEDIA_DEMO_EDITING_RUNS_WITH_LIVE).into());
            }
            cx.notify();
            return;
        }
        if edit.privacy == StoryPrivacy::SelectedUsers && edit.selected_user_ids.is_empty() {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.stories.manage.error =
                    Some(tr(keys::Q_MEDIA_PICK_AT_LEAST_ONE_USER).into());
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
                Ok(_) => self.stories.privacy_sent = true,
                Err(_) => {
                    live.driver.session.stories.manage.error =
                        Some(tr(keys::Q_MEDIA_COULD_NOT_SEND_THE_PRIVACY).into());
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
            StoryAreaKind::Location {
                address, location, ..
            } => Self::story_area_pin_label(&[address, &location.coords_label()]),
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
}

#[cfg(feature = "demo-capture")]
mod demo;
mod cover_strip;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod picker_tests;
pub(in crate::ui) use cover_strip::CoverPicker;
mod picker;
mod story_page_overlay;

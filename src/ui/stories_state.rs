//! Stories UI state: the strip, viewer, composer, story page and their boxes.
#![deny(clippy::indexing_slicing)]

use super::story_albums::StoryPrivacyEdit;
use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::story_composer::StoryComposer;
use quill::story_viewer::{StoryPlayback, StoryViewer};

/// The facts the custom-emoji warm-up of the story viewer depends on.
pub(super) type EmojiWarm = (Option<(quill::ids::ChatId, i32)>, Vec<i64>, usize, u32);

/// What the story viewer has open over the story it shows: its panels,
/// editors and their sent marks, the pause and the last result line. One
/// value, held only while the viewer is open (`StoryUi::viewer_ui_mut`
/// makes it then), and dropped by `release_story_viewer`: every close,
/// the deleted-story and passcode ones included, ends all of it, so the
/// next viewer cannot open on the last story's editor and Save cannot
/// send the old `(chat_id, story_id)`.
#[derive(Default)]
pub(crate) struct StoryViewerUi {
    /// Phase 9.2: story reaction picker open above the viewer overlay.
    pub(super) reaction_picker_open: bool,
    /// Phase 9.2: story reply input open in the viewer overlay.
    pub(super) reply_open: bool,
    /// The shared emoji / sticker / GIF panel was opened from the reply row.
    pub(super) reply_panel: bool,
    /// Phase 9.5: viewers panel open in the viewer overlay
    /// (`getStoryInteractions`).
    pub(super) viewers_open: bool,
    /// The statistics panel of the story open in the viewer.
    pub(super) stats_open: bool,
    /// Phase 9.5: report flow UI open in the viewer overlay
    /// (`reportStory`).
    pub(super) report_open: bool,
    /// Phase 9.5: cover-frame picker (viewer) — open on a video story
    /// with `can_be_edited`: a preview over a strip of frames.
    pub(super) cover: Option<super::story_albums::CoverPicker>,
    /// The `editStoryCover` call on its way; its answer closes the picker.
    pub(super) cover_request: Option<quill::ids::RequestId>,
    /// Phase 9.5: privacy editor (viewer) — open on a story with
    /// `can_set_privacy_settings`; reuses the 4-way privacy selector
    /// + contact checkboxes.
    pub(super) privacy_edit: Option<StoryPrivacyEdit>,
    /// The `setStoryPrivacySettings` call on its way; its answer closes
    /// the editor.
    pub(super) privacy_request: Option<quill::ids::RequestId>,
    /// B14: the press-and-hold / Space pause, and the viewer's extra
    /// panels (close friends, share) with their last result line.
    pub(super) pause: quill::story_extras::StoryUserPause,
    pub(super) close_friends_edit: Option<quill::story_extras::CloseFriendsEdit>,
    pub(super) close_friends_saving: bool,
    /// The story-page call sent from here (Hide, the profile toggle, the
    /// close-friends list): only its status line shows under the story.
    pub(super) page_op: Option<quill::ids::RequestId>,
    pub(super) share_open: bool,
    pub(super) notice: Option<String>,
    /// The ⋯ menu is open: the story holds still under it, and its entries
    /// act on the story it was opened for (`MenuAct` carries it).
    pub(super) menu_open: bool,
}

/// A panel the viewer draws over its story. One is open at a time:
/// opening one closes the rest (`StoryViewerUi::close_other_panels`), so they
/// never stack and push the story out of its frame, or fight over the one
/// search field they share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ViewerPanel {
    Viewers,
    Stats,
    Report,
    Share,
    Cover,
    Privacy,
    CloseFriends,
}

impl StoryViewerUi {
    /// `keep` is being opened: every other panel closes. A call a closed
    /// panel already sent (`cover_request`, `privacy_request`, a saving
    /// close-friends list) stays tracked, so its answer still settles.
    pub(super) fn close_other_panels(&mut self, keep: ViewerPanel) {
        if keep != ViewerPanel::Viewers {
            self.viewers_open = false;
        }
        if keep != ViewerPanel::Stats {
            self.stats_open = false;
        }
        if keep != ViewerPanel::Report {
            self.report_open = false;
        }
        if keep != ViewerPanel::Share {
            self.share_open = false;
        }
        if keep != ViewerPanel::Cover {
            self.cover = None;
        }
        if keep != ViewerPanel::Privacy {
            self.privacy_edit = None;
        }
        if keep != ViewerPanel::CloseFriends && !self.close_friends_saving {
            self.close_friends_edit = None;
        }
    }
}

pub(crate) struct StoryUi {
    /// Stories strip tiles and sideways scroll (`quill::stories_strip`).
    pub(super) strip: super::stories_strip_ui::StoryStripState,
    /// Phase 9.1: fullscreen story viewer (active-story tray → overlay).
    pub(super) viewer: StoryViewer,
    /// Phase 9.6: playback clock + segmented progress bar for the viewer;
    /// `tick_active` guards the at-most-one 100ms tick task (same
    /// pattern as `ensure_call_tick`).
    pub(super) playback: StoryPlayback,
    pub(super) tick_active: bool,
    /// What the tick last warmed custom-emoji reaction stickers for (the
    /// story, the ids it needs, how many stickers were known): the work
    /// repeats only when this changes.
    pub(super) emoji_warm: Option<EmojiWarm>,
    /// Phase 9.2: reply-to-story draft (the viewer overlay's reply row).
    pub(super) reply_input: Entity<TextareaState>,
    /// The live story the viewer plays (its picture, field and boxes).
    pub(super) live: super::story_viewer::LiveStoryUi,
    /// The viewers list's scroll position: the next page loads near its end.
    pub(super) viewers_scroll: ScrollHandle,
    /// The statistics panel and the sidebar's public-story hits page near
    /// their end (`scroll_load`).
    pub(super) stats_scroll: ScrollHandle,
    pub(super) stats_asked: super::scroll_load::Asked,
    pub(super) found_scroll: ScrollHandle,
    pub(super) found_asked: super::scroll_load::Asked,
    /// Phase 9.5: report details draft (the
    /// `reportStoryResultTextRequired` step).
    pub(super) report_text_input: Entity<TextareaState>,
    /// Phase 9.7: the chat story page overlay (albums / chat-page
    /// stories / archive); `None` when closed.
    pub(super) page: Option<StoryPage>,
    /// Phase 9.3: story posting composer state (pure) + its caption /
    /// user-search inputs.
    pub(super) composer: StoryComposer,
    pub(super) composer_caption: Entity<TextareaState>,
    pub(super) composer_user_search: Entity<TextareaState>,
    /// Phase 9.4: story areas — link URL + suggested-reaction emoji
    /// inputs.
    pub(super) composer_link: Entity<TextareaState>,
    pub(super) composer_reaction: Entity<TextareaState>,
    pub(super) privacy_user_search: Entity<TextareaState>,
    /// B14: the current video story's clip: waiting for it, its player
    /// (`Purpose::Story`, drawn by `video_frame` in the story frame), or
    /// the fallback to the duration clock (`story_video_state`).
    pub(super) video: super::story_video_state::StoryVideo,
    /// B14: story sound muted, and the search field the viewer's extra
    /// panels (close friends, share) share.
    pub(super) muted: bool,
    pub(super) more_search: Entity<TextareaState>,
    pub(super) stealth_box: Option<super::story_stealth_ui::StealthBox>,
    pub(super) stealth_tick: Option<Task<()>>,
    pub(super) stealth_toast: Option<super::story_stealth_ui::ShownStealthToast>,
    /// What the viewer has open over its story; `Some` only while the
    /// viewer is open (`viewer_ui_mut`), dropped by every close
    /// (`release_story_viewer`).
    open: Option<StoryViewerUi>,
    /// What reads see while the viewer is closed: always empty, since only
    /// `viewer_ui` (shared) hands it out.
    closed: StoryViewerUi,
}

/// The story viewer's `closeStory` and a live story's leave go to the old
/// client (`leave_live_story_for_switch`).
impl super::on_screen::Farewell for StoryUi {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        app.leave_live_story_for_switch(cx);
    }
}

impl StoryUi {
    /// What the viewer has open, or nothing while it is closed.
    pub(super) fn viewer_ui(&self) -> &StoryViewerUi {
        self.open.as_ref().unwrap_or(&self.closed)
    }

    /// What the viewer has open, made on first use while it is open.
    /// `None` while it is closed: nothing opens over a closed viewer.
    pub(super) fn viewer_ui_mut(&mut self) -> Option<&mut StoryViewerUi> {
        if !self.viewer.is_open() {
            return None;
        }
        Some(self.open.get_or_insert_with(StoryViewerUi::default))
    }

    /// Change what the viewer has open; nothing while it is closed.
    pub(super) fn with_viewer_ui(&mut self, f: impl FnOnce(&mut StoryViewerUi)) {
        if let Some(open) = self.viewer_ui_mut() {
            f(open);
        }
    }

    /// The viewer closed: what it had open goes, as one value.
    pub(super) fn drop_viewer_ui(&mut self) {
        self.open = None;
    }

    /// The viewer moved on from the story it showed: what was open over
    /// that story goes, as one value. The close friends editor stays: it
    /// edits the user's list, not the story. The caller ends what other
    /// owners hold for the story first (`QuillApp::leave_story_item`).
    pub(super) fn leave_item(&mut self) {
        let Some(open) = self.open.take() else {
            return;
        };
        if self.viewer.is_open() {
            // The close-friends editor is about the list, not the story,
            // so it stays, and with it the call that loads or saves it.
            let close_friends = open.close_friends_edit.is_some();
            self.open = Some(StoryViewerUi {
                close_friends_edit: open.close_friends_edit,
                close_friends_saving: open.close_friends_saving,
                page_op: open.page_op.filter(|_| close_friends),
                ..StoryViewerUi::default()
            });
        }
    }

    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let story_reply_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_REPLY_TO_STORY))
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.5: report details draft — shown when the server answers
        // `reportStoryResultTextRequired`.
        let story_report_text_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_REPORT_DETAILS_OPTIONAL))
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.3: story composer inputs — caption and the
        // selected-users search.
        let story_composer_caption = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_CAPTION_BOLD_MARKUP_SUPPORTED))
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let story_composer_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_SEARCH_CONTACTS))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.4: story area inputs — link sticker URL and
        // suggested-reaction emoji (space-separated).
        let story_composer_link = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_STORIES_LINK_URL_PLACEHOLDER))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_composer_reaction = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_OPTIONAL_SPACE_SEPARATED))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.5: the privacy editor's contact search.
        let story_more_search = cx.new(|cx| {
            // No placeholder: the viewer draws its own on the dark chrome
            // (`chrome::search_field`).
            TextareaState::new(window, cx)
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_privacy_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_SEARCH_CONTACTS))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            strip: Default::default(),
            reply_input: story_reply_input,
            live: super::story_viewer::LiveStoryUi::new(window, cx),
            viewers_scroll: ScrollHandle::new(),
            stats_scroll: ScrollHandle::new(),
            stats_asked: Default::default(),
            found_scroll: ScrollHandle::new(),
            found_asked: Default::default(),
            report_text_input: story_report_text_input,
            page: None,
            composer: StoryComposer::default(),
            composer_caption: story_composer_caption,
            composer_user_search: story_composer_user_search,
            composer_link: story_composer_link,
            composer_reaction: story_composer_reaction,
            privacy_user_search: story_privacy_user_search,
            video: Default::default(),
            muted: false,
            more_search: story_more_search,
            stealth_box: None,
            stealth_tick: None,
            stealth_toast: None,
            viewer: StoryViewer::closed(),
            playback: StoryPlayback::default(),
            tick_active: false,
            emoji_warm: None,
            open: None,
            closed: StoryViewerUi::default(),
        }
    }
}

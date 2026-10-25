//! Stories UI state: the strip, viewer, composer, story page and their boxes.
#![deny(clippy::indexing_slicing)]

use super::story_albums::StoryPrivacyEdit;
use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::story_composer::StoryComposer;
use quill::story_viewer::{StoryPlayback, StoryViewer};
use std::time::Instant;

/// The facts the custom-emoji warm-up of the story viewer depends on.
pub(super) type EmojiWarm = (Option<(quill::ids::ChatId, i32)>, Vec<i64>, usize, u32);

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
    /// Phase 9.2: story reaction picker open above the viewer overlay.
    pub(super) reaction_picker_open: bool,
    /// Phase 9.2: story reply input open in the viewer overlay.
    pub(super) reply_open: bool,
    /// Phase 9.2: reply-to-story draft (the viewer overlay's reply row).
    pub(super) reply_input: Entity<TextareaState>,
    /// The live story the viewer plays (its picture, field and boxes).
    pub(super) live: super::story_viewer::LiveStoryUi,
    /// The shared emoji / sticker / GIF panel was opened from the reply row.
    pub(super) reply_panel: bool,
    /// Phase 9.5: viewers panel open in the viewer overlay
    /// (`getStoryInteractions`).
    pub(super) viewers_open: bool,
    /// The viewers list's scroll position: the next page loads near its end.
    pub(super) viewers_scroll: ScrollHandle,
    /// The statistics panel of the story open in the viewer.
    pub(super) stats_open: bool,
    /// The statistics panel and the sidebar's public-story hits page near
    /// their end (`scroll_load`).
    pub(super) stats_scroll: ScrollHandle,
    pub(super) stats_asked: super::scroll_load::Asked,
    pub(super) found_scroll: ScrollHandle,
    pub(super) found_asked: super::scroll_load::Asked,
    /// Phase 9.5: report flow UI open in the viewer overlay
    /// (`reportStory`).
    pub(super) report_open: bool,
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
    /// Phase 9.5: cover-frame picker (viewer) — open on a video story
    /// with `can_be_edited`: a preview over a strip of frames.
    pub(super) cover: Option<super::story_albums::CoverPicker>,
    pub(super) cover_sent: bool,
    /// Phase 9.5: privacy editor (viewer) — open on a story with
    /// `can_set_privacy_settings`; reuses the 4-way privacy selector
    /// + contact checkboxes.
    pub(super) privacy_edit: Option<StoryPrivacyEdit>,
    pub(super) privacy_user_search: Entity<TextareaState>,
    pub(super) privacy_sent: bool,
    /// B14: the player of the current video story (`Purpose::Story`, drawn
    /// by `video_frame` in the story frame), the story it belongs to, and
    /// the story whose clip could not play.
    pub(super) player: Option<super::video_player::VideoPlayer>,
    pub(super) native_key: Option<(i64, i32)>,
    pub(super) native_failed: Option<(i64, i32)>,
    pub(super) native_play_at: Instant,
    pub(super) native_paused_by_us: bool,
    pub(super) video_wait_since: Option<Instant>,
    /// B14: story sound muted, the press-and-hold / Space pause, and the
    /// viewer's extra panels (close friends, share) with their shared
    /// search field and last result line.
    pub(super) muted: bool,
    pub(super) pause: quill::story_extras::StoryUserPause,
    pub(super) close_friends_edit: Option<quill::story_extras::CloseFriendsEdit>,
    pub(super) close_friends_saving: bool,
    pub(super) share_open: bool,
    pub(super) more_search: Entity<TextareaState>,
    pub(super) notice: Option<String>,
    pub(super) stealth_box: Option<super::story_stealth_ui::StealthBox>,
    pub(super) stealth_tick: Option<Task<()>>,
    pub(super) stealth_toast: Option<super::story_stealth_ui::ShownStealthToast>,
}

impl StoryUi {
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
            viewers_open: false,
            viewers_scroll: ScrollHandle::new(),
            stats_open: false,
            stats_scroll: ScrollHandle::new(),
            stats_asked: Default::default(),
            found_scroll: ScrollHandle::new(),
            found_asked: Default::default(),
            report_open: false,
            report_text_input: story_report_text_input,
            page: None,
            composer: StoryComposer::default(),
            composer_caption: story_composer_caption,
            composer_user_search: story_composer_user_search,
            composer_link: story_composer_link,
            composer_reaction: story_composer_reaction,
            cover: None,
            cover_sent: false,
            privacy_edit: None,
            privacy_user_search: story_privacy_user_search,
            player: None,
            native_key: None,
            native_failed: None,
            native_play_at: std::time::Instant::now(),
            native_paused_by_us: false,
            video_wait_since: None,
            muted: false,
            pause: Default::default(),
            close_friends_edit: None,
            close_friends_saving: false,
            share_open: false,
            more_search: story_more_search,
            notice: None,
            stealth_box: None,
            stealth_tick: None,
            stealth_toast: None,
            privacy_sent: false,
            viewer: StoryViewer::closed(),
            playback: StoryPlayback::default(),
            tick_active: false,
            emoji_warm: None,
            reaction_picker_open: false,
            reply_open: false,
            reply_panel: false,
        }
    }
}

//! Stories UI state: the strip, viewer, composer, story page and their boxes.

use super::story_albums::StoryPrivacyEdit;
use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::story_composer::StoryComposer;
use quill::story_viewer::{StoryPlayback, StoryViewer};
use std::time::Instant;

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
    /// Phase 9.2: story reaction picker open above the viewer overlay.
    pub(super) reaction_picker_open: bool,
    /// Phase 9.2: story reply input open in the viewer overlay.
    pub(super) reply_open: bool,
    /// Phase 9.2: reply-to-story draft (the viewer overlay's reply row).
    pub(super) reply_input: Entity<TextareaState>,
    /// Phase 9.5: viewers panel open in the viewer overlay
    /// (`getStoryInteractions`).
    pub(super) viewers_open: bool,
    /// The statistics panel of the story open in the viewer.
    pub(super) stats_open: bool,
    /// Phase 9.5: report flow UI open in the viewer overlay
    /// (`reportStory`).
    pub(super) report_open: bool,
    /// Phase 9.5: report details draft (the
    /// `reportStoryResultTextRequired` step).
    pub(super) report_text_input: Entity<TextareaState>,
    /// Phase 9.7: the chat story page overlay (albums / chat-page
    /// stories / archive); `None` when closed.
    pub(super) page: Option<StoryPage>,
    /// Phase 9.3: story posting composer state (pure) + its path /
    /// caption / user-search inputs.
    pub(super) composer: StoryComposer,
    pub(super) composer_path: Entity<TextareaState>,
    pub(super) composer_caption: Entity<TextareaState>,
    pub(super) composer_user_search: Entity<TextareaState>,
    /// Phase 9.4: story areas — link URL + suggested-reaction emoji
    /// inputs.
    pub(super) composer_link: Entity<TextareaState>,
    pub(super) composer_reaction: Entity<TextareaState>,
    /// Phase 9.5: cover-frame editor (viewer) — open on a video story
    /// with `can_be_edited`; the input takes seconds.
    pub(super) cover_target: Option<(i64, i32)>,
    pub(super) cover_input: Entity<TextareaState>,
    pub(super) cover_sent: bool,
    /// Phase 9.5: privacy editor (viewer) — open on a story with
    /// `can_set_privacy_settings`; reuses the 4-way privacy selector
    /// + contact checkboxes.
    pub(super) privacy_edit: Option<StoryPrivacyEdit>,
    pub(super) privacy_user_search: Entity<TextareaState>,
    pub(super) privacy_sent: bool,
    /// B14: native player of the current video story (a `RefCell` because
    /// the overlay renders from `&self` and pulling a frame needs `&mut`),
    /// the story it belongs to, and the story whose clip could not play.
    pub(super) native: std::cell::RefCell<Option<super::native_video::NativeVideo>>,
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
}

impl StoryUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let story_reply_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Reply to story")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.5: report details draft — shown when the server answers
        // `reportStoryResultTextRequired`.
        let story_report_text_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Report details (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // Phase 9.3: story composer inputs — media path (path entry; no
        // native file-picker infrastructure yet), caption, and the
        // selected-users search.
        let story_composer_path = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("/path/to/photo.jpg")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_composer_caption = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Caption… (**bold** markup supported)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let story_composer_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.4: story area inputs — link sticker URL and
        // suggested-reaction emoji (space-separated).
        let story_composer_link = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("https://… (optional, Premium)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_composer_reaction = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("❤️ (optional, space-separated)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // Phase 9.5: cover-frame seconds input (viewer cover editor) and
        // the privacy editor's contact search.
        let story_more_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_cover_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Cover frame time in seconds, e.g. 1.5")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let story_privacy_user_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search contacts")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            strip: Default::default(),
            reply_input: story_reply_input,
            viewers_open: false,
            stats_open: false,
            report_open: false,
            report_text_input: story_report_text_input,
            page: None,
            composer: StoryComposer::default(),
            composer_path: story_composer_path,
            composer_caption: story_composer_caption,
            composer_user_search: story_composer_user_search,
            composer_link: story_composer_link,
            composer_reaction: story_composer_reaction,
            cover_target: None,
            cover_input: story_cover_input,
            cover_sent: false,
            privacy_edit: None,
            privacy_user_search: story_privacy_user_search,
            native: std::cell::RefCell::new(None),
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
            privacy_sent: false,
            viewer: StoryViewer::closed(),
            playback: StoryPlayback::default(),
            tick_active: false,
            reaction_picker_open: false,
            reply_open: false,
        }
    }
}

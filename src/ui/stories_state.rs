//! Stories UI state: the strip, viewer, composer, story page and their boxes.
#![deny(clippy::indexing_slicing)]

use super::story_albums::StoryPrivacyEdit;
use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::story_composer::StoryComposer;
use quill::story_viewer::StoryPlayback;

/// The facts the custom-emoji warm-up of the story viewer depends on.
pub(super) type EmojiWarm = (Option<(quill::ids::ChatId, i32)>, Vec<i64>, usize, u32);

/// A panel the viewer draws over its story. One is open at a time: opening
/// one replaces the one before (`StoryViewerUi::show`), so they never stack
/// and push the story out of its frame, or fight over the one search field
/// they share. Each carries what only it needs.
#[derive(Default)]
pub(super) enum ViewerPanel {
    #[default]
    None,
    /// Phase 9.2: story reaction picker open above the viewer overlay.
    ReactionPicker,
    /// Phase 9.2: story reply input open in the viewer overlay.
    Reply(ReplyRow),
    /// Phase 9.5: viewers panel (`getStoryInteractions`).
    Viewers,
    /// The statistics panel of the story.
    Stats,
    /// Phase 9.5: report flow UI (`reportStory`).
    Report,
    Share,
    /// The close-friends editor. It is about the user's list, not the
    /// story, so it stays when the viewer steps to another story.
    CloseFriends(quill::story_extras::CloseFriendsStage),
    /// Phase 9.5: privacy editor, open on a story with
    /// `can_set_privacy_settings`; reuses the 4-way privacy selector +
    /// contact checkboxes.
    Privacy(StoryPrivacyEdit),
    /// Phase 9.5: cover-frame picker, open on a video story with
    /// `can_be_edited`: a preview over a strip of frames.
    Cover(Box<super::story_albums::CoverPicker>),
}

/// Whether the reply row has opened the shared emoji / sticker / GIF panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplyRow {
    Typing,
    MediaPanel,
}

/// Which panel, without what it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PanelKind {
    ReactionPicker,
    Reply,
    Viewers,
    Stats,
    Report,
    Share,
    CloseFriends,
    Privacy,
    Cover,
}

impl ViewerPanel {
    fn kind(&self) -> Option<PanelKind> {
        Some(match self {
            Self::None => return None,
            Self::ReactionPicker => PanelKind::ReactionPicker,
            Self::Reply(_) => PanelKind::Reply,
            Self::Viewers => PanelKind::Viewers,
            Self::Stats => PanelKind::Stats,
            Self::Report => PanelKind::Report,
            Self::Share => PanelKind::Share,
            Self::CloseFriends(_) => PanelKind::CloseFriends,
            Self::Privacy(_) => PanelKind::Privacy,
            Self::Cover(_) => PanelKind::Cover,
        })
    }
}

/// What the story viewer has open over the story it shows: its panel,
/// the calls its editors sent, the pause and the last result line. One
/// value, held only while the viewer is open (`StoryUi::viewer_ui_mut`
/// makes it then), and dropped by `release_story_viewer`: every close,
/// the deleted-story and passcode ones included, ends all of it, so the
/// next viewer cannot open on the last story's editor and Save cannot
/// send the old `(chat_id, story_id)`.
#[derive(Default)]
pub(crate) struct StoryViewerUi {
    /// The one panel open over the story.
    pub(super) panel: ViewerPanel,
    /// B14: the press-and-hold / Space pause.
    pub(super) pause: quill::story_extras::StoryUserPause,
    /// The story-page call sent from here (Hide, the profile toggle, the
    /// close-friends list): only its status line shows under the story.
    pub(super) page_op: Option<quill::ids::RequestId>,
    /// The copy of the story's file to Downloads (`save_story_media`).
    pub(super) notice: Option<String>,
    /// The ⋯ menu is open: the story holds still under it, and its entries
    /// act on the story it was opened for (`MenuAct` carries it). It
    /// mirrors the kit menu's own visibility (`on_open_change`) and can be
    /// up over any panel, so it is not a panel.
    pub(super) menu_open: bool,
    /// The "Stealth mode on" toast over this viewer until its deadline;
    /// it goes with the viewer (`release_story_viewer`).
    pub(super) stealth_toast: Option<super::story_stealth_ui::ShownStealthToast>,
}

static NO_CLOSE_FRIENDS: quill::story_extras::CloseFriendsStage =
    quill::story_extras::CloseFriendsStage::Closed;

impl StoryViewerUi {
    /// `panel` opens and replaces the one before. A call a replaced editor
    /// already sent stays tracked in `StoryUi::manage_calls`.
    pub(super) fn show(&mut self, panel: ViewerPanel) {
        self.panel = panel;
    }

    /// The call's editor closes when it is still open on the call's story.
    pub(super) fn close_manage(&mut self, call: ManageCall) {
        let (kind, open) = match call.kind {
            ManageKind::Cover => (PanelKind::Cover, self.cover().map(|c| c.story())),
            ManageKind::Privacy => (
                PanelKind::Privacy,
                self.privacy_edit().map(|e| (e.chat_id, e.story_id)),
            ),
        };
        if open == Some(call.story) {
            self.close(kind);
        }
    }

    /// The panel of `kind` closes; another panel stays.
    pub(super) fn close(&mut self, kind: PanelKind) {
        if self.panel.kind() == Some(kind) {
            self.panel = ViewerPanel::None;
        }
    }

    pub(super) fn is_open(&self, kind: PanelKind) -> bool {
        self.panel.kind() == Some(kind)
    }

    /// Any panel is open: the story holds still under it.
    pub(super) fn has_panel(&self) -> bool {
        !matches!(self.panel, ViewerPanel::None)
    }

    /// Open `panel` (a plain one), or close it when it is already open.
    pub(super) fn toggle(&mut self, kind: PanelKind, panel: ViewerPanel) {
        if self.is_open(kind) {
            self.panel = ViewerPanel::None;
        } else {
            self.panel = panel;
        }
    }

    pub(super) fn reply_open(&self) -> bool {
        matches!(self.panel, ViewerPanel::Reply(_))
    }

    /// The reply row has opened the shared media panel.
    pub(super) fn reply_panel(&self) -> bool {
        matches!(self.panel, ViewerPanel::Reply(ReplyRow::MediaPanel))
    }

    /// The reply row's media panel opens or closes with its row; with no
    /// row open there is nothing to change.
    pub(super) fn set_reply_panel(&mut self, open: bool) {
        if let ViewerPanel::Reply(row) = &mut self.panel {
            *row = if open {
                ReplyRow::MediaPanel
            } else {
                ReplyRow::Typing
            };
        }
    }

    pub(super) fn open_reply(&mut self) {
        if !self.reply_open() {
            self.panel = ViewerPanel::Reply(ReplyRow::Typing);
        }
    }

    pub(super) fn cover(&self) -> Option<&super::story_albums::CoverPicker> {
        match &self.panel {
            ViewerPanel::Cover(cover) => Some(cover),
            _ => None,
        }
    }

    pub(super) fn cover_mut(&mut self) -> Option<&mut super::story_albums::CoverPicker> {
        match &mut self.panel {
            ViewerPanel::Cover(cover) => Some(cover),
            _ => None,
        }
    }

    pub(super) fn privacy_edit(&self) -> Option<&StoryPrivacyEdit> {
        match &self.panel {
            ViewerPanel::Privacy(edit) => Some(edit),
            _ => None,
        }
    }

    pub(super) fn privacy_edit_mut(&mut self) -> Option<&mut StoryPrivacyEdit> {
        match &mut self.panel {
            ViewerPanel::Privacy(edit) => Some(edit),
            _ => None,
        }
    }

    /// The close-friends editor's stage; `Closed` while another panel (or
    /// none) is open.
    pub(super) fn close_friends(&self) -> &quill::story_extras::CloseFriendsStage {
        match &self.panel {
            ViewerPanel::CloseFriends(stage) => stage,
            _ => &NO_CLOSE_FRIENDS,
        }
    }

    pub(super) fn close_friends_mut(
        &mut self,
    ) -> Option<&mut quill::story_extras::CloseFriendsStage> {
        match &mut self.panel {
            ViewerPanel::CloseFriends(stage) => Some(stage),
            _ => None,
        }
    }

    /// The editor moves to `stage` while it is the open panel; a panel
    /// that replaced it is not brought back.
    pub(super) fn set_close_friends(&mut self, stage: quill::story_extras::CloseFriendsStage) {
        if matches!(stage, quill::story_extras::CloseFriendsStage::Closed) {
            self.close(PanelKind::CloseFriends);
        } else if let Some(open) = self.close_friends_mut() {
            *open = stage;
        }
    }
}

/// One story's file being copied to Downloads on a background thread
/// (`StoryUi::media_saves`, one per story item). `Saving` ends with the
/// copy's result, which waits in `Done` for the story it was for, so
/// stepping away and back still shows it. Saving again is allowed once it
/// is `Done`. `Saving` holds the continuation's task: dropping the entry
/// (a new account, the app going away) cancels it.
pub(super) enum MediaSave {
    Saving(Task<()>),
    Done(String),
}

impl MediaSave {
    pub(super) fn is_saving(&self) -> bool {
        matches!(self, Self::Saving(_))
    }
}

/// Which story editor sent a management call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ManageKind {
    Cover,
    Privacy,
}

/// An `editStoryCover` or `setStoryPrivacySettings` call on its way, for
/// the story it was sent for. It outlives the panel that sent it and a
/// step to another story, and ends with its own answer (matched by
/// `request`) in `story_frame_upkeep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ManageCall {
    pub kind: ManageKind,
    pub story: (i64, i32),
    pub request: quill::ids::RequestId,
}

/// The viewer's 100 ms tick. The task is owned here, so dropping the
/// state stops it; it ends its own stage with `finish` when the viewer
/// has closed.
#[derive(Default)]
pub(super) enum StoryTick {
    #[default]
    Off,
    Running(Task<()>),
}

impl StoryTick {
    pub(super) fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }

    /// The tick task is done: it ends its own stage without cancelling
    /// itself from inside.
    pub(super) fn finish(&mut self) {
        if let Self::Running(task) = std::mem::replace(self, Self::Off) {
            // This task is the one running this call: detach lets it end, dropping would cancel it.
            task.detach();
        }
    }
}

pub(crate) struct StoryUi {
    /// Stories strip tiles and sideways scroll (`quill::stories_strip`).
    pub(super) strip: super::stories_strip_ui::StoryStripState,
    /// Phase 9.1: fullscreen story viewer (active-story tray → overlay).
    pub(super) viewer: super::story_viewer::StoryOpen,
    /// Phase 9.6: playback clock + segmented progress bar for the viewer;
    /// `tick` is the at-most-one 100ms tick task.
    pub(super) playback: StoryPlayback,
    pub(super) tick: StoryTick,
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
    pub(super) composer: Box<StoryComposer>,
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
    /// The stealth-mode box and its once-a-second tick, one value: the tick
    /// ends with the box.
    pub(super) stealth_box: Option<super::story_stealth_ui::StealthBox>,
    /// What the viewer has open over its story; `Some` only while the
    /// viewer is open (`viewer_ui_mut`), dropped by every close
    /// (`release_story_viewer`).
    open: Option<StoryViewerUi>,
    /// The Save media copies, by the story they are for: they outlive a
    /// step to another story (a second press for the same story waits for
    /// the first), and the results are dropped with the viewer.
    pub(super) media_saves: std::collections::HashMap<(quill::ids::ChatId, i32), MediaSave>,
    /// The cover and privacy calls on their way, by story. Opening an
    /// editor or stepping to another story never drops one; each ends with
    /// its own answer.
    pub(super) manage_calls: Vec<ManageCall>,
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
        // Finished results go with the viewer; a copy still running stays
        // so a reopened viewer does not start a second one.
        self.media_saves.retain(|_, save| save.is_saving());
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
            let close_friends = open.is_open(PanelKind::CloseFriends);
            let panel = if close_friends {
                open.panel
            } else {
                ViewerPanel::None
            };
            self.open = Some(StoryViewerUi {
                panel,
                page_op: open.page_op.filter(|_| close_friends),
                ..StoryViewerUi::default()
            });
        }
    }

    /// A copy ended: its entry becomes the result for its story (`None`
    /// when the viewer closed meanwhile). The task of the finished entry
    /// is let go, not cancelled from inside itself.
    pub(super) fn finish_media_save(
        &mut self,
        key: (quill::ids::ChatId, i32),
        result: Option<String>,
    ) {
        let old = match result {
            Some(notice) => self.media_saves.insert(key, MediaSave::Done(notice)),
            None => self.media_saves.remove(&key),
        };
        if let Some(MediaSave::Saving(task)) = old {
            // This task is the one running this call: detach lets it end, dropping would cancel it.
            task.detach();
        }
    }

    /// A call of `kind` for `story` is out.
    pub(super) fn manage_call_busy(&self, kind: ManageKind, story: (i64, i32)) -> bool {
        self.manage_calls
            .iter()
            .any(|call| call.kind == kind && call.story == story)
    }

    /// The call is answered (or lost): it is no longer tracked.
    pub(super) fn end_manage_call(&mut self, request: quill::ids::RequestId) {
        self.manage_calls.retain(|call| call.request != request);
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
            composer: Box::default(),
            composer_caption: story_composer_caption,
            composer_user_search: story_composer_user_search,
            composer_link: story_composer_link,
            composer_reaction: story_composer_reaction,
            privacy_user_search: story_privacy_user_search,
            video: Default::default(),
            muted: false,
            more_search: story_more_search,
            stealth_box: None,
            viewer: super::story_viewer::StoryOpen::closed(),
            playback: StoryPlayback::default(),
            tick: StoryTick::Off,
            emoji_warm: None,
            open: None,
            media_saves: std::collections::HashMap::new(),
            manage_calls: Vec::new(),
            closed: StoryViewerUi::default(),
        }
    }
}

#[cfg(test)]
mod reply_tests {
    use super::{StoryViewerUi, ViewerPanel};

    #[test]
    fn the_panel_belongs_to_an_open_row_and_goes_with_it() {
        let mut ui = StoryViewerUi::default();
        ui.set_reply_panel(true);
        assert!(!ui.reply_open(), "a closed row has no panel");
        ui.open_reply();
        ui.set_reply_panel(true);
        assert!(ui.reply_open() && ui.reply_panel());
        ui.set_reply_panel(false);
        assert!(ui.reply_open() && !ui.reply_panel());
        ui.set_reply_panel(true);
        ui.panel = ViewerPanel::None;
        assert!(!ui.reply_open() && !ui.reply_panel());
        // Reopening starts without the old panel.
        ui.open_reply();
        assert!(!ui.reply_panel());
    }
}

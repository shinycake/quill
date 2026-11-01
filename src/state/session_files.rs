//! File tracking, downloads and auto-download policy.
use super::*;
use crate::text::TextEntityKind;

/// Clips of the inline results and GIF panels fetched for autoplay, and
/// the largest one.
const INLINE_CLIPS_PREFETCH: usize = 24;
const INLINE_CLIP_MAX: i64 = 5 * 1024 * 1024;

impl Session {
    pub(crate) fn upsert_file(&mut self, file: ParsedFile, from_file_update: bool) {
        // The file on disk changed: drop the sandbox verdicts for its path.
        if !file.local.path.is_empty() {
            crate::local_path::invalidate_display_path(&file.local.path);
        }
        if !from_file_update
            && self.media.files.get(&file.id.0).is_some_and(|current| {
                (current.usable_path().is_some() && file.usable_path().is_none())
                    || (current.local.is_downloading_active && file.local.is_idle_incomplete())
            })
        {
            return;
        }
        let id = file.id.0;
        let idle_incomplete = file.local.is_idle_incomplete();
        if file.local.is_downloading_completed {
            self.record_completed_user_download(id);
            self.media.downloads.forget(id);
        }
        if from_file_update && !file.local.is_downloading_active {
            self.media.downloads.confirm_cancel(id);
        }
        let paused = self.media.downloads.is_paused(id);
        if from_file_update && idle_incomplete && !paused && self.media.downloads.is_requested(id) {
            // A request went idle without completing (an error that did
            // not fail the request, or a stall): a user download offers
            // Retry, an automatic one is not re-sent per ingest. Explicit
            // cancels are no longer requested.
            self.media.downloads.end(id);
        }
        if file.local.is_downloading_completed
            || !file.local.can_be_downloaded
            || (from_file_update && idle_incomplete && !paused)
        {
            self.unstick_download(id);
        }
        let sound_id = self.settings.sound_file_ids.get(&file.id.0).copied();
        if let Some(sound_id) = sound_id {
            if let Some(path) = file.usable_path() {
                if self.settings.pending_sound_downloads.remove(&sound_id) {
                    // Parity slice: a completed notification-sound download
                    // with playback requested → hand the path to the UI for
                    // the in-process player. The reducer never spawns processes.
                    self.settings.pending_sound_plays.push(path.into());
                }
            } else if from_file_update && file.local.is_idle_incomplete() {
                // Parity slice: a sound download that errored/cancelled
                // (active → idle without completing) must not leave the id in
                // `pending_sound_downloads` — otherwise a stale late
                // completion could trigger a belated play.
                self.settings.pending_sound_downloads.remove(&sound_id);
            }
        }
        self.note_avatar_file_changed(file.id.0);
        self.media.files.insert(file);
        self.evict_unused_files();
    }

    /// `revision` minus the `updateFile`s that moved no usable path: what a
    /// view showing names, avatars and headers (not transfer progress)
    /// depends on.
    pub fn revision_without_file_progress(&self) -> u64 {
        self.revision.wrapping_sub(self.media.files.quiet_updates())
    }

    /// Bound `media.files` ([`FileMap::evict`]). Kept whatever they were
    /// last used for: a file transferring, asked for or paused, one the
    /// Downloads panel lists, a queued playlist track, an avatar the chat
    /// list shows, the open chat's automatic downloads and the
    /// notification sounds. A file a row shows is read on every render,
    /// so it is among the most recently used (kept by recency, not held).
    fn evict_unused_files(&mut self) {
        if !self.media.files.evict_due() {
            return;
        }
        let media = &mut self.media;
        let downloads = &media.downloads;
        let avatars = &media.avatar_file_refs;
        let open_chat = &media.open_chat_media_downloads;
        let extras = &media.download_extras;
        let sounds = &self.settings.sound_file_ids;
        // Queued tracks are not rendered, so no read keeps them recent.
        let queued: HashSet<i32> = media
            .playlists
            .values()
            .flat_map(|playlist| playlist.tracks.iter().map(|track| track.file_id.0))
            .collect();
        media.files.evict(|file| {
            let id = file.id.0;
            file.local.is_downloading_active
                || file.uploading
                || downloads.is_requested(id)
                || downloads.is_paused(id)
                || downloads.is_user(id)
                || downloads.is_listed(id)
                || queued.contains(&id)
                || avatars.contains_key(&id)
                || open_chat.contains(&id)
                || extras.has_file(id)
                || sounds.contains_key(&id)
        });
    }

    /// Stop tracking the in-flight request for `file_id`; an end already
    /// recorded (Retry, stalled) stays.
    pub(crate) fn unstick_download(&mut self, file_id: i32) {
        self.note_avatar_file_changed(file_id);
        self.media.downloads.settle(file_id);
        self.media.download_extras.remove_file(file_id);
    }

    /// Record a finished user-initiated download in the downloads manager's
    /// recent list. Shared by the `updateFile` completion path and the
    /// list-API `updateFileDownload` path.
    pub(crate) fn record_completed_user_download(&mut self, file_id: i32) {
        if self.media.downloads.is_user(file_id) {
            self.media.downloads.record_finished(file_id);
        }
    }

    pub fn file(&self, id: FileId) -> Option<&ParsedFile> {
        self.media.files.get(&id.0)
    }

    /// The display files of the custom emoji in `entities` that are known
    /// but not downloaded yet.
    fn custom_emoji_files_to_download(&self, entities: &[crate::text::TextEntity]) -> Vec<FileId> {
        let mut ids = Vec::new();
        for entity in entities {
            let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                continue;
            };
            let resolved = self
                .stickers
                .emoji
                .custom_emoji_stickers
                .lookup(custom_emoji_id);
            if let Some(file_id) = resolved.and_then(|s| s.display_file_id())
                && self.should_download(file_id)
            {
                ids.push(file_id);
            }
        }
        ids
    }

    pub fn should_download(&self, file_id: FileId) -> bool {
        if file_id.0 == 0 {
            return false;
        }
        if self.media.downloads.is_requested(file_id.0) || self.requests.has_download(file_id) {
            return false;
        }
        match self.media.files.get(&file_id.0) {
            // A transfer TDLib already runs with no request of ours (another
            // device, a resume) is not requested a second time. After our
            // own request ended, TDLib's idle update is what clears it.
            Some(file)
                if file.local.is_downloading_active
                    && !self.media.downloads.has_record(file_id.0) =>
            {
                false
            }
            // Cancelled, TDLib's idle update still on its way: an automatic
            // download must not undo the cancel.
            Some(file)
                if file.local.is_downloading_active
                    && self.media.downloads.is_cancelling(file_id.0) =>
            {
                false
            }
            Some(file) => file.needs_download(),
            None => true,
        }
    }

    /// An automatic download request goes out.
    pub fn begin_download(&mut self, file_id: FileId) {
        self.media.downloads.begin(file_id.0, DownloadOrigin::Auto);
    }

    /// A user-initiated (listed) download request goes out.
    pub fn begin_user_download(&mut self, file_id: FileId) {
        self.media.downloads.begin(file_id.0, DownloadOrigin::User);
    }

    /// The user cancelled the download: the row stops showing progress at
    /// once, TDLib's idle update confirms it.
    pub fn abort_download(&mut self, file_id: FileId) {
        self.note_avatar_file_changed(file_id.0);
        self.media.downloads.cancel(file_id.0);
        self.media.download_extras.remove_file(file_id.0);
    }

    /// Files the inline results panel shows, most urgent first: each
    /// result's still image and MP4 preview loop (small, so all of them),
    /// then the full clips of the first screens (tdesktop loads the GIFs of
    /// the panel). Downloaded ahead of the chat's own thumbnails: the panel
    /// is what the user is looking at.
    pub fn inline_result_file_ids_to_download(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        let Some(InlineQuerySlot {
            fetch: InlineQueryFetch::Loaded { results, .. },
            ..
        }) = &self.bots.inline_query
        else {
            return ids;
        };
        let prefs = &self.settings.media_prefs;
        let autoplay = prefs.autoplay_gifs && !prefs.data_saver;
        for result in results {
            if let Some(media) = result.media
                && self.should_download(media.file_id)
            {
                ids.push(media.file_id);
            }
            if autoplay
                && let Some(preview) = result.preview_clip
                && self.should_download(preview)
            {
                ids.push(preview);
            }
        }
        if autoplay {
            for clip in results
                .iter()
                .filter_map(|result| result.clip)
                .take(INLINE_CLIPS_PREFETCH)
            {
                let small = self
                    .file(clip)
                    .is_none_or(|file| file.size.max(file.expected_size) <= INLINE_CLIP_MAX);
                if small && self.should_download(clip) {
                    ids.push(clip);
                }
            }
        }
        ids
    }

    /// Files the GIF panel shows, most urgent first: every tile's still
    /// picture, then the MP4 preview loops of the first screens, then the
    /// small full clips of the first screens' GIFs that have no preview
    /// (tdesktop `VideoPreviewState::automaticLoad`). Downloaded ahead of
    /// the chat's own thumbnails: the panel is what the user is looking at.
    pub fn gif_panel_file_ids_to_download(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        if !self.stickers.gifs.open {
            return ids;
        }
        let shown = self.stickers.gifs.visible_animations();
        let prefs = &self.settings.media_prefs;
        let autoplay = prefs.autoplay_gifs && !prefs.data_saver;
        ids.extend(
            shown
                .iter()
                .filter_map(AnimationItem::still_thumb)
                .filter(|id| self.should_download(*id)),
        );
        if autoplay {
            let first = shown.iter().take(INLINE_CLIPS_PREFETCH);
            ids.extend(
                first
                    .clone()
                    .filter_map(AnimationItem::preview_clip)
                    .filter(|id| self.should_download(*id)),
            );
            ids.extend(
                first
                    .filter(|animation| animation.preview_clip().is_none())
                    .map(|animation| animation.file_id)
                    .filter(|clip| {
                        self.file(*clip)
                            .is_none_or(|file| file.size.max(file.expected_size) <= INLINE_CLIP_MAX)
                            && self.should_download(*clip)
                    }),
            );
        }
        ids
    }

    /// Photo thumbs in the open chat that are not secret/spoiler and still need a download.
    pub fn thumb_file_ids_to_download(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        // The open Saved Messages sublist / tag filter keeps its own rows.
        let saved_rows = self
            .threads
            .saved
            .sublist
            .iter()
            .map(|view| &view.history)
            .chain(
                self.threads
                    .saved
                    .tag_search
                    .iter()
                    .map(|search| &search.history),
            )
            .flat_map(|history| history.messages.values());
        for message in self
            .open_chat
            .and_then(|id| self.histories.get(&id.0))
            .into_iter()
            .flat_map(|history| history.messages.values())
            .chain(saved_rows)
        {
            // The effect badge on a message sent with an effect, and the
            // effect itself, ready to play when the message shows
            // (tdesktop `preloadEffectImageFor` and `preloadEffect`).
            if let Some(effect) = self.stickers.effects.get(message.extras.effect_id) {
                let animation = effect.animation().map(|animation| animation.file);
                for file in effect.icon_file().into_iter().chain(animation) {
                    if self.should_download(file) && !ids.contains(&file) {
                        ids.push(file);
                    }
                }
            }
            match &message.content {
                MessageContent::Photo(photo) => {
                    if photo.is_secret || photo.has_spoiler {
                        continue;
                    }
                    if let Some(size) = photo.thumb_size()
                        && self.should_download(size.file_id)
                    {
                        ids.push(size.file_id);
                    }
                }
                // A chat-photo change / suggested profile photo shows its
                // picture under the service row.
                MessageContent::Action(action) => {
                    if let crate::telegram::envelope::ServiceAction::ChatPhoto {
                        photo: Some(photo),
                        ..
                    }
                    | crate::telegram::envelope::ServiceAction::SuggestProfilePhoto {
                        photo: Some(photo),
                        ..
                    } = action.as_ref()
                        && let Some(size) = photo.thumb_size().or_else(|| photo.largest_size())
                        && self.should_download(size.file_id)
                    {
                        ids.push(size.file_id);
                    }
                    // A gift / giveaway card shows its sticker.
                    if let crate::telegram::envelope::ServiceAction::WithCard { card, .. } =
                        action.as_ref()
                        && let Some(file_id) =
                            card.sticker.as_ref().and_then(|s| s.display_file_id())
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::Text(text) => {
                    if let Some(preview) = &text.link_preview
                        && let Some(photo) = &preview.photo
                        && let Some(size) = photo.thumb_size()
                        && self.should_download(size.file_id)
                    {
                        ids.push(size.file_id);
                    }
                    // Wallpaper and theme cards paint from their document.
                    if let Some(preview) = &text.link_preview {
                        ids.extend(
                            preview
                                .kind
                                .picture_files()
                                .into_iter()
                                .filter(|id| self.should_download(*id)),
                        );
                    }
                    // Custom emoji stickers referenced by the text need
                    // their display file (thumbnail first, else static
                    // WEBP) before the inline image can render.
                    for entity in &text.entities {
                        let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                            continue;
                        };
                        let resolved = self
                            .stickers
                            .emoji
                            .custom_emoji_stickers
                            .lookup(custom_emoji_id);
                        if let Some(file_id) = resolved.and_then(|s| s.display_file_id())
                            && self.should_download(file_id)
                        {
                            ids.push(file_id);
                        }
                    }
                }
                MessageContent::Sticker(sticker) => {
                    if let Some(file_id) = sticker.display_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                // A dice's roll and landing: their thumbnails are the
                // stills shown until the animations decode.
                MessageContent::Dice(dice) => {
                    for file_id in [&dice.initial, &dice.landing]
                        .into_iter()
                        .flatten()
                        .flat_map(|stickers| stickers.layers())
                        .filter_map(|sticker| sticker.display_file_id())
                    {
                        if self.should_download(file_id) {
                            ids.push(file_id);
                        }
                    }
                }
                MessageContent::Animation(animation) => {
                    if animation.is_secret || animation.has_spoiler {
                        continue;
                    }
                    if let Some(file_id) = animation.thumb_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::Video(video) => {
                    if video.is_secret || video.has_spoiler {
                        continue;
                    }
                    if let Some(file_id) = video.thumb_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::VideoNote(note) => {
                    if note.is_secret {
                        continue;
                    }
                    if let Some(file_id) = note.thumb_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::Audio(audio) => {
                    if let Some(file_id) = audio.cover_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                // The picture of a thumbed file row (image files and the
                // other documents that carry a thumbnail).
                MessageContent::Document(doc) => {
                    if let Some(thumb) = &doc.thumbnail
                        && self.should_download(thumb.file_id)
                    {
                        ids.push(thumb.file_id);
                    }
                }
                // The pictures beside poll options, and the custom emoji in
                // their text.
                MessageContent::Poll(poll) => {
                    for option in &poll.poll.options {
                        let thumb = option
                            .media
                            .as_ref()
                            .and_then(|media| media.thumb_candidates().into_iter().next());
                        if let Some(file_id) = thumb
                            && self.should_download(file_id)
                        {
                            ids.push(file_id);
                        }
                        ids.extend(self.custom_emoji_files_to_download(&option.entities));
                    }
                }
                _ => {}
            }
        }
        // The received-gifts dialog shows each gift's sticker.
        if self.payments.hub.gifts_open {
            for gift in &self.payments.hub.gifts {
                if let Some(file_id) = gift
                    .gift
                    .sticker
                    .as_ref()
                    .and_then(|sticker| sticker.display_file_id())
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        {
            let visible = if self.stickers.stickers.open {
                self.stickers.stickers.visible_stickers()
            } else {
                &[]
            };
            for sticker in visible
                .iter()
                .chain(self.stickers.stickers.suggestions.iter())
            {
                let file_id = sticker.display_file_id();
                if let Some(file_id) = file_id
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        if self.stickers.emoji.open {
            for sticker in &self.stickers.emoji.custom_emoji_stickers {
                let file_id = sticker.display_file_id();
                if let Some(file_id) = file_id
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        // Sponsored rows in the open chat: content + sponsor thumbs at priority 1.
        if let Some(chat_id) = self.open_chat
            && let Some(entry) = self.messages.sponsored.get(&chat_id.0)
        {
            for message in &entry.messages {
                for file_id in message.thumb_file_ids() {
                    if self.should_download(file_id) {
                        ids.push(file_id);
                    }
                }
            }
        }
        // Thumbnails the reply strips draw (fetched originals, other-chat
        // replies).
        for file_id in self.reply_thumb_file_ids() {
            if self.should_download(file_id) {
                ids.push(file_id);
            }
        }
        // Recent-post thumbnails of the open statistics panel.
        for file_id in self.stats_thumb_file_ids() {
            if self.should_download(file_id) {
                ids.push(file_id);
            }
        }
        ids.extend(self.map_thumb_file_ids_to_download());
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }

    /// MED3: full media files in the open chat eligible for automatic
    /// download under the user's per-chat-kind × media-type prefs (TGX
    /// `settings_autodownload`). Unlike the thumbnail pass, each media type
    /// is gated on its own flag; secret and spoiler content is never
    /// auto-downloaded (same safeguard as the thumb hook), and files known
    /// to exceed `AUTO_DOWNLOAD_MAX_BYTES` are skipped (TGX
    /// `canAutomaticallyDownload` download limit).
    pub fn auto_download_media_file_ids(&self) -> Vec<FileId> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        if self.settings.media_prefs.data_saver {
            return Vec::new();
        }
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        let push = |flag: u8, file_id: FileId, ids: &mut Vec<FileId>| {
            // MED3 review: skip files known to exceed the auto-download cap
            // (TGX `canAutomaticallyDownload` download limit, WiFi default
            // 50 MiB); an unknown size (`display_size() == 0`) is not a
            // reason to block.
            let oversized = self
                .media
                .files
                .get(&file_id.0)
                .is_some_and(|file| file.display_size() > AUTO_DOWNLOAD_MAX_BYTES);
            if !oversized
                && self.auto_download_allowed(chat_id, flag)
                && self.should_download(file_id)
            {
                ids.push(file_id);
            }
        };
        for message in history.messages.values() {
            if let Some((flag, file_id)) = auto_download_target(&message.content) {
                push(flag, file_id, &mut ids);
            }
        }
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }

    /// MED3: auto-download gate for automatic (non-user-initiated) downloads
    /// in a chat. `flag` is one of the `settings::AUTO_DOWNLOAD_*` media-type
    /// bits. Data saver pauses every automatic download (TGX
    /// `settings_datasaver`); otherwise the chat kind selects the per-kind
    /// bitfield (TGX `settings_autodownload` private/group/channel shifts).
    /// Secret chats use the private bucket.
    pub fn auto_download_allowed(&self, chat_id: ChatId, flag: u8) -> bool {
        if self.settings.media_prefs.data_saver || self.media.disk_holds_auto_downloads {
            return false;
        }
        let bits = match self.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Supergroup {
                is_channel: true, ..
            }) => self.settings.media_prefs.auto_download_channels,
            Some(ChatKind::BasicGroup { .. }) | Some(ChatKind::Supergroup { .. }) => {
                self.settings.media_prefs.auto_download_groups
            }
            _ => self.settings.media_prefs.auto_download_private,
        };
        bits & flag != 0
    }
}

/// The file automatic download fetches for `content`, with the media-type
/// bit that allows it.
pub(super) fn auto_download_target(content: &MessageContent) -> Option<(u8, FileId)> {
    match content {
        MessageContent::Photo(photo) if !photo.is_secret && !photo.has_spoiler => {
            Some((AUTO_DOWNLOAD_PHOTO, photo.largest_size()?.file_id))
        }
        // A picture file that shows as a photo loads like one.
        MessageContent::Document(doc)
            if crate::media_viewer::shows_as_photo(doc)
                && (1..=crate::media_viewer::PICTURE_FILE_AUTO_DOWNLOAD_MAX)
                    .contains(&doc.size) =>
        {
            Some((AUTO_DOWNLOAD_PHOTO, doc.file_id))
        }
        MessageContent::Document(doc) => Some((AUTO_DOWNLOAD_FILE, doc.file_id)),
        MessageContent::Animation(animation) if !animation.is_secret && !animation.has_spoiler => {
            Some((AUTO_DOWNLOAD_GIF, animation.file_id))
        }
        MessageContent::Video(video) if !video.is_secret && !video.has_spoiler => {
            Some((AUTO_DOWNLOAD_VIDEO, video.file_id))
        }
        MessageContent::VideoNote(note) if !note.is_secret => {
            Some((AUTO_DOWNLOAD_VIDEO_NOTE, note.file_id))
        }
        MessageContent::VoiceNote(voice) => Some((AUTO_DOWNLOAD_VOICE, voice.file_id)),
        MessageContent::Audio(audio) => Some((AUTO_DOWNLOAD_MUSIC, audio.file_id)),
        _ => None,
    }
}

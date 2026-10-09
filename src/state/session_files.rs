//! File tracking, downloads and auto-download policy.
use super::*;
use crate::text::TextEntityKind;

impl Session {
    pub(crate) fn upsert_file(&mut self, file: ParsedFile, from_file_update: bool) {
        if !from_file_update
            && self.files.get(&file.id.0).is_some_and(|current| {
                (current.usable_path().is_some() && file.usable_path().is_none())
                    || (current.local.is_downloading_active && file.local.is_idle_incomplete())
            })
        {
            return;
        }
        let idle_incomplete = file.local.is_idle_incomplete();
        let paused = self.paused_downloads.contains(&file.id.0);
        if file.local.is_downloading_completed {
            self.failed_downloads.remove(&file.id.0);
            self.stalled_auto_downloads.remove(&file.id.0);
            self.record_completed_user_download(file.id.0);
        }
        if from_file_update
            && idle_incomplete
            && self.downloading.contains(&file.id.0)
            && !self.user_downloads.contains(&file.id.0)
        {
            // An automatic download we started went idle without
            // completing (explicit cancels already left `downloading`).
            self.stalled_auto_downloads.insert(file.id.0);
        }
        if from_file_update
            && idle_incomplete
            && !paused
            && self.user_downloads.contains(&file.id.0)
        {
            // MED3: a user-initiated download that went active → idle without
            // completing stalled (or errored without failing the request) —
            // surface it as failed so the row offers Retry. Explicit cancels
            // are excluded: `abort_download` already dropped them from
            // `user_downloads`.
            self.failed_downloads.insert(file.id.0);
        }
        if file.local.is_downloading_completed
            || !file.local.can_be_downloaded
            || (from_file_update && idle_incomplete && !paused)
        {
            self.unstick_download(file.id.0);
        }
        let sound_id = self.sound_file_ids.get(&file.id.0).copied();
        if let Some(sound_id) = sound_id {
            if let Some(path) = file.usable_path() {
                if self.pending_sound_downloads.remove(&sound_id) {
                    // Parity slice: a completed notification-sound download
                    // with playback requested → hand the path to the UI for
                    // the in-process player. The reducer never spawns processes.
                    self.pending_sound_plays.push(path.into());
                }
            } else if from_file_update && file.local.is_idle_incomplete() {
                // Parity slice: a sound download that errored/cancelled
                // (active → idle without completing) must not leave the id in
                // `pending_sound_downloads` — otherwise a stale late
                // completion could trigger a belated play.
                self.pending_sound_downloads.remove(&sound_id);
            }
        }
        self.note_avatar_file_changed(file.id.0);
        self.files.insert(file.id.0, file);
    }

    pub(crate) fn unstick_download(&mut self, file_id: i32) {
        self.note_avatar_file_changed(file_id);
        self.downloading.remove(&file_id);
        self.user_downloads.remove(&file_id);
        self.paused_downloads.remove(&file_id);
        self.download_extras.retain(|_, id| *id != file_id);
    }

    /// Record a finished user-initiated download in the downloads manager's
    /// recent list (deduped, capped at 50). Shared by the `updateFile`
    /// completion path and the list-API `updateFileDownload` path.
    pub(crate) fn record_completed_user_download(&mut self, file_id: i32) {
        if self.user_downloads.contains(&file_id) {
            self.completed_downloads.retain(|id| *id != file_id);
            self.completed_downloads.push_back(file_id);
            while self.completed_downloads.len() > 50 {
                self.completed_downloads.pop_front();
            }
        }
    }

    pub fn file(&self, id: FileId) -> Option<&ParsedFile> {
        self.files.get(&id.0)
    }

    pub fn should_download(&self, file_id: FileId) -> bool {
        if file_id.0 == 0 {
            return false;
        }
        if self.downloading.contains(&file_id.0) || self.requests.has_download(file_id) {
            return false;
        }
        match self.files.get(&file_id.0) {
            Some(file) => file.needs_download(),
            None => true,
        }
    }

    pub fn begin_download(&mut self, file_id: FileId) {
        if file_id.0 != 0 {
            self.failed_downloads.remove(&file_id.0);
            self.stalled_auto_downloads.remove(&file_id.0);
            self.downloading.insert(file_id.0);
        }
    }

    pub fn abort_download(&mut self, file_id: FileId) {
        self.unstick_download(file_id.0);
    }

    /// Photo thumbs in the open chat that are not secret/spoiler and still need a download.
    pub fn thumb_file_ids_to_download(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        // The open Saved Messages sublist / tag filter keeps its own rows.
        let saved_rows = self
            .saved
            .sublist
            .iter()
            .map(|view| &view.history)
            .chain(self.saved.tag_search.iter().map(|search| &search.history))
            .flat_map(|history| history.messages.values());
        for message in self
            .open_chat
            .and_then(|id| self.histories.get(&id.0))
            .into_iter()
            .flat_map(|history| history.messages.values())
            .chain(saved_rows)
        {
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
                    // Custom emoji stickers referenced by the text need
                    // their display file (thumbnail first, else static
                    // WEBP) before the inline image can render.
                    for entity in &text.entities {
                        let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                            continue;
                        };
                        let resolved = self
                            .emoji
                            .custom_emoji_stickers
                            .iter()
                            .find(|s| s.custom_emoji_id == Some(custom_emoji_id));
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
                // A dice's landing animation: its thumbnail is the still
                // shown until the animation decodes.
                MessageContent::Dice(dice) => {
                    if let Some(file_id) = dice
                        .final_sticker
                        .as_ref()
                        .and_then(|sticker| sticker.display_file_id())
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
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
                _ => {}
            }
        }
        // The received-gifts dialog shows each gift's sticker.
        if self.hub.gifts_open {
            for gift in &self.hub.gifts {
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
        if self.gifs.open {
            for animation in self.gifs.visible_animations() {
                let file_id = animation.thumb_file_id.filter(|id| id.0 != 0);
                if let Some(file_id) = file_id
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        {
            let visible = if self.stickers.open {
                self.stickers.visible_stickers()
            } else {
                &[]
            };
            for sticker in visible.iter().chain(self.stickers.suggestions.iter()) {
                let file_id = sticker.display_file_id();
                if let Some(file_id) = file_id
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        if self.emoji.open {
            for sticker in self
                .emoji
                .preview
                .iter()
                .chain(&self.emoji.custom_emoji_stickers)
            {
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
            && let Some(entry) = self.sponsored.get(&chat_id.0)
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
        if self.media_prefs.data_saver {
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
            match &message.content {
                MessageContent::Photo(photo) => {
                    if photo.is_secret || photo.has_spoiler {
                        continue;
                    }
                    if let Some(size) = photo.largest_size() {
                        push(AUTO_DOWNLOAD_PHOTO, size.file_id, &mut ids);
                    }
                }
                MessageContent::Document(doc) => {
                    push(AUTO_DOWNLOAD_FILE, doc.file_id, &mut ids);
                }
                MessageContent::Animation(animation) => {
                    if animation.is_secret || animation.has_spoiler {
                        continue;
                    }
                    push(AUTO_DOWNLOAD_GIF, animation.file_id, &mut ids);
                }
                MessageContent::Video(video) => {
                    if video.is_secret || video.has_spoiler {
                        continue;
                    }
                    push(AUTO_DOWNLOAD_VIDEO, video.file_id, &mut ids);
                }
                MessageContent::VideoNote(note) => {
                    if note.is_secret {
                        continue;
                    }
                    push(AUTO_DOWNLOAD_VIDEO_NOTE, note.file_id, &mut ids);
                }
                MessageContent::VoiceNote(voice) => {
                    push(AUTO_DOWNLOAD_VOICE, voice.file_id, &mut ids);
                }
                MessageContent::Audio(audio) => {
                    push(AUTO_DOWNLOAD_MUSIC, audio.file_id, &mut ids);
                }
                _ => {}
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
        if self.media_prefs.data_saver {
            return false;
        }
        let bits = match self.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Supergroup {
                is_channel: true, ..
            }) => self.media_prefs.auto_download_channels,
            Some(ChatKind::BasicGroup { .. }) | Some(ChatKind::Supergroup { .. }) => {
                self.media_prefs.auto_download_groups
            }
            _ => self.media_prefs.auto_download_private,
        };
        bits & flag != 0
    }
}

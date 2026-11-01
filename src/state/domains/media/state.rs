//! Files, downloads, the shared-media gallery, the media library and map thumbnails: the `media` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct MediaState {
    /// The emoji/sticker panel's set contents (`getStickerSet`, lazily).
    pub media_library: MediaLibrary,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: FileMap,
    /// What was asked to download and how each request ended; the status
    /// rows show is `downloads.status(id, &files)`.
    pub downloads: DownloadIntents,
    /// Automatic downloads of full media in the open chat (MED3), whose
    /// progress the history draws: their `updateFile`s redraw at once
    /// (`redraw_need`). Pruned to the requested downloads on each pass.
    pub open_chat_media_downloads: HashSet<i32>,
    /// Avatar file id → number of chats showing it (chat-list photos),
    /// kept where those ids are set.
    pub(crate) avatar_file_refs: HashMap<i32, u32>,
    /// Avatar file ids whose download state may have changed since the
    /// driver last looked (`take_due_chat_list_photos`).
    pub(crate) avatar_downloads_due: BTreeSet<i32>,
    /// Re-check every avatar once (new session, any auth change: requests
    /// were invalidated and files may have been cleared).
    pub(crate) avatar_rescan: bool,
    /// User avatars a view looked up and found missing
    /// (`Session::user_photo_path`), downloaded at the next poll tick
    /// (`take_avatar_claims`): users are many and most are never shown.
    pub(crate) avatar_claims: std::cell::RefCell<BTreeSet<i32>>,
    /// Downloads manager panel open (right side, next to the info panel).
    pub downloads_panel_open: bool,
    /// `@extra` → `file.id` until the download unsticks (survives `file@extra` consuming pending).
    pub(crate) download_extras: DownloadExtras,
    /// Slice media-shared-gallery: per-chat shared-media gallery state
    /// (Media / Files / Music / Links / Voice / GIFs tabs).
    pub shared_media: SharedMediaState,
    /// Static map tiles of location / venue messages.
    pub map_thumbs: MapThumbs,
    /// Message counts per `searchMessagesFilter*` (index into
    /// `MEDIA_COUNT_FILTERS`) for chats whose info panel was opened.
    pub chat_media_counts: HashMap<i64, HashMap<u8, i32>>,
    /// Saved music per user (`getUserProfileAudios`), for the profile's
    /// music button and the Playlist page.
    pub playlists: HashMap<i64, Playlist>,
    /// Songs checked against the current user's saved music
    /// (`isProfileAudio`), by file id: `true` when saved.
    pub profile_audio_ids: HashMap<i32, bool>,
    /// Thumbnails of the Stories and Gifts grids waiting for a download.
    pub(crate) profile_thumbs_due: Vec<FileId>,
}

impl MediaState {
    pub(crate) fn new() -> Self {
        Self {
            media_library: MediaLibrary::default(),
            files: FileMap::default(),
            downloads: DownloadIntents::default(),
            open_chat_media_downloads: HashSet::new(),
            avatar_file_refs: HashMap::new(),
            avatar_downloads_due: BTreeSet::new(),
            avatar_rescan: true,
            avatar_claims: Default::default(),
            downloads_panel_open: false,
            download_extras: DownloadExtras::default(),
            shared_media: SharedMediaState::default(),
            map_thumbs: MapThumbs::default(),
            chat_media_counts: HashMap::new(),
            playlists: HashMap::new(),
            profile_audio_ids: HashMap::new(),
            profile_thumbs_due: Vec::new(),
        }
    }
}

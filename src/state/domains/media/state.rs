//! Files, downloads, the shared-media gallery, the media library and map thumbnails: the `media` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct MediaState {
    /// The emoji/sticker panel's set contents (`getStickerSet`, lazily).
    pub media_library: MediaLibrary,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: HashMap<i32, ParsedFile>,
    /// `downloadFile` in flight (until completed, undownloadable, idle, or error).
    pub downloading: HashSet<i32>,
    /// Subset of `downloading` the user explicitly started (history rows,
    /// viewer) — these surface in the downloads manager. Automatic
    /// thumbs/avatars/sounds are not tracked here.
    pub user_downloads: HashSet<i32>,
    /// Subset of `user_downloads` currently paused via
    /// `toggleDownloadIsPaused`. Pause is list state (`updateFileDownload`)
    /// tracked separately from the in-flight download above.
    pub paused_downloads: HashSet<i32>,
    /// `downloadFile` requests TDLib answered with an error (file id → still
    /// in `downloading` until unstuck; the UI shows "failed — retry").
    /// Cleared when a new download starts or the file completes.
    pub failed_downloads: HashSet<i32>,
    /// Automatic (non-user) downloads TDLib refused or stopped without
    /// completing. The automatic path skips them so each ingest does not
    /// re-send `downloadFile`; an explicit user download or completion
    /// clears the mark (Telegram X `TdlibFilesManager.onFileUpdate` treats
    /// a stopped download as paused until asked again).
    pub stalled_auto_downloads: HashSet<i32>,
    /// Automatic downloads of full media in the open chat (MED3), whose
    /// progress the history draws: their `updateFile`s redraw at once
    /// (`redraw_need`). Pruned to `downloading` on each pass.
    pub open_chat_media_downloads: HashSet<i32>,
    /// Avatar file id → number of chats / users showing it (chat-list
    /// photos and contact `photo_small`), kept where those ids are set.
    pub(crate) avatar_file_refs: HashMap<i32, u32>,
    /// Avatar file ids whose download state may have changed since the
    /// driver last looked (`take_due_chat_list_photos`).
    pub(crate) avatar_downloads_due: BTreeSet<i32>,
    /// Re-check every avatar once (new session, any auth change: requests
    /// were invalidated and files may have been cleared).
    pub(crate) avatar_rescan: bool,
    /// Recently completed downloads (file ids, most recent last, capped) for
    /// the downloads manager's "recent" list. Recorded only when a file was
    /// in `downloading` and its `updateFile` shows completion — pre-existing
    /// local files don't count.
    pub completed_downloads: VecDeque<i32>,
    /// Downloads manager panel open (right side, next to the info panel).
    pub downloads_panel_open: bool,
    /// `@extra` → `file.id` until the download unsticks (survives `file@extra` consuming pending).
    pub(crate) download_extras: HashMap<u64, i32>,
    /// Slice media-shared-gallery: per-chat shared-media gallery state
    /// (Media / Files / Music / Links / Voice / GIFs tabs).
    pub shared_media: SharedMediaState,
    /// Static map tiles of location / venue messages.
    pub map_thumbs: MapThumbs,
    /// Message counts per `searchMessagesFilter*` (index into
    /// `MEDIA_COUNT_FILTERS`) for chats whose info panel was opened.
    pub chat_media_counts: HashMap<i64, HashMap<u8, i32>>,
}

impl MediaState {
    pub(crate) fn new() -> Self {
        Self {
            media_library: MediaLibrary::default(),
            files: HashMap::new(),
            downloading: HashSet::new(),
            user_downloads: HashSet::new(),
            paused_downloads: HashSet::new(),
            failed_downloads: HashSet::new(),
            stalled_auto_downloads: HashSet::new(),
            open_chat_media_downloads: HashSet::new(),
            avatar_file_refs: HashMap::new(),
            avatar_downloads_due: BTreeSet::new(),
            avatar_rescan: true,
            completed_downloads: VecDeque::new(),
            downloads_panel_open: false,
            download_extras: HashMap::new(),
            shared_media: SharedMediaState::default(),
            map_thumbs: MapThumbs::default(),
            chat_media_counts: HashMap::new(),
        }
    }
}

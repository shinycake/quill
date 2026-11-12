//! Files, downloads, the shared-media gallery, the media library and map thumbnails: the `media` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

/// Where a song stands against the current user's saved music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileAudioMark {
    /// `isProfileAudio` went out as `request`; only its answer settles the
    /// song (a local save or remove replaces the mark, so a late answer
    /// finds nothing to change).
    Checking {
        request: RequestId,
    },
    Saved,
    NotSaved,
}

pub struct MediaState {
    /// The emoji/sticker panel's set contents (`getStickerSet`, lazily).
    pub media_library: MediaLibrary,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: FileMap,
    /// How the disk stands: a low or full one holds automatic downloads.
    pub disk: DiskPressure,
    /// Listed downloads the low disk paused, resumed when it is healthy, and
    /// the saved set of their messages (`DiskPauses`).
    pub disk_paused: DiskPauses,
    /// The search for downloads paused by the disk before this TDLib
    /// instance went out (once per session).
    pub disk_search: DiskSearch,
    /// Downloads that search found, waiting for their resume.
    pub disk_resume_due: Vec<i32>,
    /// What was asked to download and how each request ended; the status
    /// rows show is `downloads.status(id, &files)`.
    pub downloads: DownloadIntents,
    /// Avatar file id → number of chats showing it (chat-list photos),
    /// kept where those ids are set.
    pub(crate) avatar_file_refs: HashMap<i32, u32>,
    /// Avatar downloads the driver has not looked at yet
    /// (`take_due_chat_list_photos`).
    pub(crate) avatar_due: AvatarDue,
    /// User avatars a view looked up and found missing
    /// (`Session::user_photo_path`), downloaded at the next poll tick
    /// (`take_avatar_claims`): users are many and most are never shown.
    pub(crate) avatar_claims: std::cell::RefCell<BTreeSet<i32>>,
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
    /// (`isProfileAudio`), by file id.
    pub profile_audio_ids: HashMap<i32, ProfileAudioMark>,
    /// Thumbnails of the Stories and Gifts grids waiting for a download.
    pub(crate) profile_thumbs_due: Vec<FileId>,
}

/// How much room the disk leaves for downloads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiskPressure {
    #[default]
    Healthy,
    /// Low: automatic downloads wait. `room` is what is left above the
    /// close floor, when the free space is known.
    Low { room: Option<u64> },
    /// Full: nothing more fits.
    Full,
}

impl DiskPressure {
    /// A low or full disk holds automatic downloads.
    pub fn holds_auto_downloads(self) -> bool {
        self != Self::Healthy
    }

    /// Room above the close floor while the disk is low; `None` otherwise.
    pub fn room(self) -> Option<u64> {
        match self {
            Self::Healthy => None,
            Self::Low { room } => room,
            Self::Full => Some(0),
        }
    }

    /// The disk is low or full with a known room.
    pub fn is_pressed(self) -> bool {
        self.room().is_some()
    }
}

/// The avatar downloads to look at: every avatar once (a new session or any
/// auth change: requests were invalidated and files may have been
/// cleared), or those whose owner or download state changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AvatarDue {
    Everything,
    Some(BTreeSet<i32>),
}

impl AvatarDue {
    pub(crate) fn insert(&mut self, id: i32) {
        if let Self::Some(set) = self {
            set.insert(id);
        }
    }

    pub(crate) fn extend(&mut self, ids: impl IntoIterator<Item = i32>) {
        if let Self::Some(set) = self {
            set.extend(ids);
        }
    }

    /// What is due, leaving nothing.
    pub(crate) fn take(&mut self) -> Self {
        std::mem::replace(self, Self::Some(BTreeSet::new()))
    }
}

/// The once-per-session search for downloads an earlier TDLib instance's
/// disk pause left. A failed search goes back to `NotAsked`, so the next
/// healthy reading asks again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DiskSearch {
    #[default]
    NotAsked,
    /// Out, as `request`. `keys` are the saved pauses it asks about: one
    /// saved after it went out is not part of its answer.
    Asked {
        request: RequestId,
        keys: BTreeSet<(i64, i64)>,
    },
    Done,
}

impl MediaState {
    pub(crate) fn new() -> Self {
        Self {
            media_library: MediaLibrary::default(),
            files: FileMap::default(),
            disk: DiskPressure::Healthy,
            disk_paused: DiskPauses::default(),
            disk_search: DiskSearch::NotAsked,
            disk_resume_due: Vec::new(),
            downloads: DownloadIntents::default(),
            avatar_file_refs: HashMap::new(),
            avatar_due: AvatarDue::Everything,
            avatar_claims: Default::default(),
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

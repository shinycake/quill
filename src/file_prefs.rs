//! B13: file preferences shared by every account, mirroring tdesktop's
//! app-wide settings: the download folder with "Ask where to save each
//! file" (`Core::Settings::downloadPath` / `askDownloadPath`) and the
//! "File open confirmations" pair (`noWarningExtensions` /
//! `ipRevealWarning`). They live in `file_prefs.json` under the app data
//! root (isolated in screenshot demos, so fixtures never touch the real
//! file).
//!
//! The open-safety classifier is a port of tdesktop's
//! `Core::DetectNameType` / `Core::IsIpRevealingPath` and
//! `LauncherWouldWarn` (`core/mime_type.cpp`,
//! `data/data_document_resolver.cpp`), with the same per-platform
//! executable extension lists.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cap on the saved whitelist (tdesktop: 1024 entries from at most 10240
/// characters of input).
const MAX_EXTENSIONS: usize = 1024;
const MAX_EXTENSION_INPUT: usize = 10_240;

/// App-wide file preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilePrefs {
    /// Folder saved files go to; `None` = the OS downloads folder.
    pub download_dir: Option<PathBuf>,
    /// "Ask where to save each file".
    pub ask_download_path: bool,
    /// Lower-case extensions (no dot) that open without a confirmation.
    pub no_warning_extensions: BTreeSet<String>,
    /// Confirm before opening files that may reveal the IP address.
    pub ip_reveal_warning: bool,
}

impl Default for FilePrefs {
    fn default() -> Self {
        Self {
            download_dir: None,
            ask_download_path: false,
            no_warning_extensions: BTreeSet::new(),
            ip_reveal_warning: true,
        }
    }
}

static PREFS: RwLock<Option<FilePrefs>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(true);

/// Turn disk writes off (tests drive the UI against the in-memory
/// value and must never touch the user's real `file_prefs.json`).
pub fn set_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn prefs_path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("file_prefs.json"))
}

fn load_from_disk() -> FilePrefs {
    prefs_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<FilePrefs>(&bytes).ok())
        .unwrap_or_default()
}

/// The current preferences (loaded from disk on first use).
pub fn current() -> FilePrefs {
    if let Ok(guard) = PREFS.read()
        && let Some(prefs) = guard.as_ref()
    {
        return prefs.clone();
    }
    let loaded = load_from_disk();
    if let Ok(mut guard) = PREFS.write() {
        return guard.get_or_insert(loaded).clone();
    }
    loaded
}

/// Replace the preferences and persist them. A failed write keeps the
/// in-memory value for this run.
pub fn store(prefs: FilePrefs) {
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = prefs_path()
    {
        let _ = crate::settings::write_json_atomic(&path, &prefs);
    }
    if let Ok(mut guard) = PREFS.write() {
        *guard = Some(prefs);
    }
}

/// Edit the preferences in place and persist.
pub fn update(edit: impl FnOnce(&mut FilePrefs)) {
    let mut prefs = current();
    edit(&mut prefs);
    store(prefs);
}

/// The configured download folder when it still exists as a directory.
pub fn configured_download_dir() -> Option<PathBuf> {
    current().download_dir.filter(|dir| dir.is_dir())
}

/// Parse the whitelist editor text: whitespace/comma separated, a leading
/// dot dropped, lower-cased, de-duplicated (tdesktop splits the first
/// 10240 characters on spaces and keeps 1024 entries).
pub fn parse_extension_list(text: &str) -> BTreeSet<String> {
    let clipped: String = text.chars().take(MAX_EXTENSION_INPUT).collect();
    clipped
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(|part| part.trim().trim_start_matches('.').to_lowercase())
        .filter(|part| !part.is_empty())
        .take(MAX_EXTENSIONS)
        .collect()
}

/// The whitelist as editor text (space separated).
pub fn format_extension_list(extensions: &BTreeSet<String>) -> String {
    extensions.iter().cloned().collect::<Vec<_>>().join(" ")
}

/// tdesktop `Core::NameType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameType {
    Executable,
    Image,
    Video,
    Audio,
    Document,
    Archive,
    ThemeFile,
    OtherBenign,
    Unknown,
}

/// Lower-cased extension of a file name (empty when there is none).
pub fn file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn has(list: &str, extension: &str) -> bool {
    list.split_whitespace().any(|item| item == extension)
}

#[cfg(target_os = "windows")]
const EXECUTABLE: &str = "\
ad ade adp ahk app application appref-ms asp aspx asx bas bat bin cab cdxml \
cer cfg cgi chi chm cmd cnt com conf cpl crt csh der diagcab dll drv eml \
exe fon fxp gadget grp hlp hpj hta htt inf ini ins inx isp isu its jar jnlp \
job js jse jsp key ksh lexe library-ms lnk local lua mad maf mag mam \
manifest maq mar mas mat mau mav maw mcf mda mdb mde mdt mdw mdz mht mhtml \
mjs mmc mof msc msg msh msh1 msh2 msh1xml msh2xml mshxml msi msp mst ops \
osd paf pcd phar php php3 php4 php5 php7 phps php-s pht phtml pif pl plg pm \
pod prf prg ps1 ps2 ps1xml ps2xml psc1 psc2 psd1 psm1 pssc pst py py3 pyc \
pyd pyi pyo pyw pyzw pyz rb reg rgs scf scr sct search-ms settingcontent-ms \
sh shb shs slk sys swf t tmp u3p url vb vbe vbp vbs vbscript vdx vsmacros \
vsd vsdm vsdx vss vssm vssx vst vstm vstx vsw vsx vtx website wlua ws wsc \
wsf wsh xbap xll xlsb xlsm xnk xs";
#[cfg(target_os = "macos")]
const EXECUTABLE: &str = "\
applescript action app bin command csh osx workflow terminal url caction \
mpkg pkg scpt scptd xhtm xhtml webarchive";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const EXECUTABLE: &str = "bin csh deb desktop ksh out pet pkg pup rpm run sh shar slp zsh";

const IMAGE: &str = "\
afdesign ai avif bmp dng gif heic icns ico jfif jpeg jpg jpg-large jxl nef \
png png-large psd qoi raw sketch svg tga tif tiff webp";
const VIDEO: &str = "\
3g2 3gp 3gpp aep avi flv h264 m4s m4v mkv mov mp4 mpeg mpg ogv srt tgs tgv \
vob webm wmv";
const AUDIO: &str = "\
aac ac3 aif amr caf cda cue flac m4a m4b mid midi mp3 ogg opus wav wma";
const DOCUMENT: &str = "\
pdf doc docx ppt pptx pps ppsx xls xlsx txt rtf odt ods odp csv text log tl \
tex xspf xml djvu diag ps ost kml pub epub mobi cbr cbz fb2 prc ris pem p7b \
m3u m3u8 wpd wpl htm html xhtml key";
const ARCHIVE: &str = "7z arj bz2 gz rar tar xz z zip zst";
const THEME_FILE: &str = "tdesktop-theme tdesktop-palette tgios-theme attheme";
const OTHER_BENIGN: &str = "\
c cc cpp cxx h m mm swift cs ts class java css ninja cmake patch diff plist \
gyp gitignore strings asoundrc torrent csr json xaml md keylayout sql \
sln xib mk dmg img iso vcd \
pdb eot ics ips ipa core mem pcap ovpn part pcapng dmp pkpass dat zxp crash \
file bak gbr plain dlc fon fnt otf ttc ttf gpx db rss cur tdesktop-endpoints";

/// Port of tdesktop `Core::DetectNameType`.
pub fn detect_name_type(path: &Path) -> NameType {
    let extension = file_extension(path);
    let extension = extension.as_str();
    if has(EXECUTABLE, extension) {
        NameType::Executable
    } else if has(IMAGE, extension) {
        NameType::Image
    } else if has(VIDEO, extension) {
        NameType::Video
    } else if has(AUDIO, extension) {
        NameType::Audio
    } else if has(DOCUMENT, extension) {
        NameType::Document
    } else if has(ARCHIVE, extension) {
        NameType::Archive
    } else if has(THEME_FILE, extension) {
        NameType::ThemeFile
    } else if has(OTHER_BENIGN, extension) {
        NameType::OtherBenign
    } else {
        NameType::Unknown
    }
}

/// Port of tdesktop `Core::IsIpRevealingPath` (extension part): files
/// whose viewer may fetch remote resources.
pub fn is_ip_revealing_path(path: &Path) -> bool {
    let extension = file_extension(path);
    let extension = extension.as_str();
    let mut list = String::from("htm html svg m4v m3u m3u8 xhtml xml kml kmz xspf");
    if cfg!(target_os = "windows") {
        list.push_str(" wpl");
    }
    if cfg!(target_os = "macos") {
        list.push_str(
            " docx dotx docm dotm xlsx xltx xlsm xltm xlsb pptx ppsx potx pptm ppsm potm",
        );
    }
    has(&list, extension)
}

/// Why a confirmation is shown before opening a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenWarning {
    /// The extension runs code (`lng_launch_exe_warning`).
    Executable,
    /// An unrecognized extension (`lng_launch_other_warning`).
    Unknown,
    /// The file may reveal the IP address (`lng_launch_svg_warning`).
    IpReveal,
}

/// What opening `path` should do. `None` = open straight away. Port of
/// `LauncherWouldWarn` plus the `LaunchWithWarning` text choice; files
/// from a verified sender never warn (tdesktop
/// `item->history()->peer->isVerified()`).
pub fn open_warning(path: &Path, prefs: &FilePrefs, verified_sender: bool) -> Option<OpenWarning> {
    if verified_sender {
        return None;
    }
    let name_type = detect_name_type(path);
    let ip_reveal = name_type != NameType::Executable && is_ip_revealing_path(path);
    if ip_reveal && prefs.ip_reveal_warning {
        return Some(OpenWarning::IpReveal);
    }
    let extension = file_extension(path);
    if matches!(name_type, NameType::Executable | NameType::Unknown)
        && !prefs.no_warning_extensions.contains(&extension)
    {
        return Some(if name_type == NameType::Executable {
            OpenWarning::Executable
        } else {
            OpenWarning::Unknown
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warn(name: &str, prefs: &FilePrefs) -> Option<OpenWarning> {
        open_warning(Path::new(name), prefs, false)
    }

    #[test]
    fn executables_and_unknown_types_warn_benign_ones_do_not() {
        let prefs = FilePrefs::default();
        assert_eq!(warn("photo.JPG", &prefs), None);
        assert_eq!(warn("notes.pdf", &prefs), None);
        assert_eq!(warn("archive.zip", &prefs), None);
        assert_eq!(warn("blob.zzzz", &prefs), Some(OpenWarning::Unknown));
        assert_eq!(warn("README", &prefs), Some(OpenWarning::Unknown));
        // `.sh` is executable on macOS only through the shared list's
        // platform entry; `.bin` is executable on every platform.
        assert_eq!(warn("tool.bin", &prefs), Some(OpenWarning::Executable));
    }

    #[test]
    fn whitelist_silences_extensions_but_not_ip_reveal() {
        let mut prefs = FilePrefs::default();
        prefs.no_warning_extensions.insert("zzzz".into());
        prefs.no_warning_extensions.insert("svg".into());
        assert_eq!(warn("blob.zzzz", &prefs), None);
        // The IP-reveal toggle decides for svg, not the whitelist.
        assert_eq!(warn("pic.svg", &prefs), Some(OpenWarning::IpReveal));
        prefs.ip_reveal_warning = false;
        assert_eq!(warn("pic.svg", &prefs), None);
        assert_eq!(warn("page.html", &prefs), None);
    }

    #[test]
    fn verified_senders_never_warn() {
        let prefs = FilePrefs::default();
        assert_eq!(open_warning(Path::new("a.bin"), &prefs, true), None);
        assert_eq!(open_warning(Path::new("a.svg"), &prefs, true), None);
    }

    #[test]
    fn extension_list_round_trips_and_normalizes() {
        let parsed = parse_extension_list(" .PDF, txt  Txt;\n.log ");
        assert_eq!(format_extension_list(&parsed), "log pdf txt");
        assert!(parse_extension_list("   ").is_empty());
        let many: String = (0..2000).map(|n| format!("e{n} ")).collect();
        assert_eq!(
            parse_extension_list(&many).len(),
            1024.min(parsed_len(&many))
        );
    }

    fn parsed_len(text: &str) -> usize {
        text.chars()
            .take(MAX_EXTENSION_INPUT)
            .collect::<String>()
            .split_whitespace()
            .count()
    }

    #[test]
    fn prefs_defaults_ask_about_ip_reveal() {
        let prefs = FilePrefs::default();
        assert!(prefs.ip_reveal_warning);
        assert!(!prefs.ask_download_path);
        assert!(prefs.download_dir.is_none());
        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<FilePrefs>(&json).unwrap(), prefs);
        // Partial files fall back to defaults field by field.
        let partial: FilePrefs = serde_json::from_str(r#"{"ask_download_path":true}"#).unwrap();
        assert!(partial.ask_download_path && partial.ip_reveal_warning);
    }
}

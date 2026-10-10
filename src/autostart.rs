//! Autostart on login (`parity:platform-autostart`).
//!
//! OS-level, no TDLib involved (the schema's `autostart` constructor is
//! for bots, not this). Linux writes an XDG Autostart `.desktop` file;
//! macOS writes a LaunchAgents plist; Windows sets a value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (the same key
//! Telegram Desktop's `platform/win/specific_win.cpp` uses, no elevation).
//!
//! Launch-minimized: tdesktop appends `-autostart` and hides the window only
//! when its "start minimized" setting is on. Quill's equivalent is the
//! persisted General "Start in tray" setting (read on every launch), so the
//! Run value is just the quoted exe path on all platforms — no flag needed.
//!
//! Failure handling: the toggle never leaves a half-written autostart
//! entry. The XDG `.desktop` file and the macOS plist are written to a
//! sibling temp file, synced, then renamed over the target (an interrupted
//! write keeps the previous file, or none). A failed write or registry
//! update surfaces as [`AutostartError::Io`] in the Settings toggle and
//! the entry keeps its old state; there is deliberately no secondary
//! mechanism (Startup-folder shortcut, systemd user unit) because a second
//! entry could not be reliably cleaned up when the user turns the toggle
//! off.
// Windows uses the registry; the file-based helpers below are for XDG/macOS.
#![cfg_attr(windows, allow(dead_code))]
use std::io;
use std::path::{Path, PathBuf};

/// User-facing failure for the autostart toggle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartError {
    /// This OS has no autostart implementation.
    Unsupported,
    Io(String),
}

impl std::fmt::Display for AutostartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutostartError::Unsupported => {
                write!(f, "autostart is not supported on this platform yet")
            }
            AutostartError::Io(detail) => write!(f, "could not update autostart: {detail}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Linux,
    MacOs,
}

/// Registry key (under HKCU) whose values Windows launches at sign-in.
#[cfg(any(windows, test))]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Value name inside [`RUN_KEY`].
#[cfg(any(windows, test))]
const RUN_VALUE_NAME: &str = "Quill";

/// `Run` value data for `exe`: the path quoted so spaces survive.
#[cfg(any(windows, test))]
fn run_value(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

fn platform() -> Option<Platform> {
    if cfg!(target_os = "macos") {
        Some(Platform::MacOs)
    } else if cfg!(target_os = "linux") {
        Some(Platform::Linux)
    } else {
        None
    }
}

/// The autostart file for `platform` under `home` (no HOME lookup — the
/// caller supplies it, which keeps this testable).
fn autostart_path_in(home: &Path, platform: Platform) -> PathBuf {
    match platform {
        Platform::Linux => home.join(".config/autostart/quill.desktop"),
        Platform::MacOs => home.join("Library/LaunchAgents/com.quill.app.plist"),
    }
}

/// `Exec=` value for `exe` per the Desktop Entry spec: a path with spaces
/// or shell-reserved characters is double-quoted with `"`, `` ` ``, `$`
/// and `\` backslash-escaped, then backslashes are doubled for the
/// desktop-file string syntax; a literal `%` is `%%` (field codes).
pub(crate) fn desktop_exec(exe: &Path) -> String {
    const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";
    let raw = exe.display().to_string().replace('%', "%%");
    if !raw.chars().any(|c| RESERVED.contains(c)) {
        return raw;
    }
    let mut quoted = String::with_capacity(raw.len() + 2);
    quoted.push('"');
    for c in raw.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                // One escape level for the Exec quoting, one for the
                // desktop-file string syntax (backslash only).
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            // A raw newline would end the `Exec=` line.
            '\n' | '\r' => quoted.push(' '),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// XML text escaping for the plist `<string>` value.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// File content that enables autostart for `exe` on `platform`.
fn render_file(platform: Platform, exe: &Path) -> String {
    match platform {
        Platform::Linux => format!(
            "[Desktop Entry]\nType=Application\nName=Quill\nExec={}\nTerminal=false\n",
            desktop_exec(exe)
        ),
        Platform::MacOs => format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
                "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\"",
                " \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
                "<plist version=\"1.0\">\n",
                "<dict>\n",
                "    <key>Label</key>\n",
                "    <string>com.quill.app</string>\n",
                "    <key>ProgramArguments</key>\n",
                "    <array>\n",
                "        <string>{}</string>\n",
                "    </array>\n",
                "    <key>RunAtLoad</key>\n",
                "    <true/>\n",
                "</dict>\n",
                "</plist>\n",
            ),
            xml_escape(&exe.display().to_string())
        ),
    }
}

/// Write `content` to `path` atomically: a sibling temp file is written
/// and synced, then renamed over the target, so a crash or full disk can
/// never leave a truncated autostart file that the session manager would
/// half-parse. The temp file is removed when any step fails.
pub(crate) fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = parent.join(format!(".{name}.tmp-{}", std::process::id()));
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

fn write_file(path: &Path, platform: Platform, exe: &Path) -> io::Result<()> {
    write_atomic(path, &render_file(platform, exe))
}

/// Whether this OS has a verified autostart implementation.
pub fn supported() -> bool {
    cfg!(windows) || platform().is_some()
}

/// Whether autostart is currently enabled for this user.
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    return crate::winreg::get_string(RUN_KEY, RUN_VALUE_NAME)
        .ok()
        .flatten()
        .is_some_and(|value| !value.is_empty());
    #[cfg(not(windows))]
    std::env::var("HOME")
        .ok()
        .is_some_and(|home| is_enabled_in(Path::new(&home)))
}

fn is_enabled_in(home: &Path) -> bool {
    platform().is_some_and(|p| autostart_path_in(home, p).exists())
}

/// Enable or disable autostart on login.
pub fn set_enabled(enabled: bool) -> Result<(), AutostartError> {
    #[cfg(windows)]
    return set_enabled_windows(enabled);
    #[cfg(not(windows))]
    set_enabled_home(enabled)
}

#[cfg(windows)]
fn set_enabled_windows(enabled: bool) -> Result<(), AutostartError> {
    let io_err = |e: io::Error| AutostartError::Io(e.to_string());
    if enabled {
        let exe = std::env::current_exe().map_err(io_err)?;
        crate::winreg::set_string(RUN_KEY, RUN_VALUE_NAME, &run_value(&exe)).map_err(io_err)
    } else {
        crate::winreg::delete_value(RUN_KEY, RUN_VALUE_NAME).map_err(io_err)
    }
}

#[cfg(not(windows))]
fn set_enabled_home(enabled: bool) -> Result<(), AutostartError> {
    let home = std::env::var("HOME").map_err(|_| AutostartError::Unsupported)?;
    set_enabled_in(Path::new(&home), enabled)
}

fn set_enabled_in(home: &Path, enabled: bool) -> Result<(), AutostartError> {
    let platform = platform().ok_or(AutostartError::Unsupported)?;
    let path = autostart_path_in(home, platform);
    if enabled {
        let exe = std::env::current_exe().map_err(|e| AutostartError::Io(e.to_string()))?;
        write_file(&path, platform, &exe).map_err(|e| AutostartError::Io(e.to_string()))
    } else if path.exists() {
        std::fs::remove_file(&path).map_err(|e| AutostartError::Io(e.to_string()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_home() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "quill-autostart-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn linux_paths_and_desktop_file() {
        let home = tmp_home();
        let path = autostart_path_in(&home, Platform::Linux);
        assert_eq!(path, home.join(".config/autostart/quill.desktop"));
        let exe = Path::new("/usr/bin/quill");
        write_file(&path, Platform::Linux, exe).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[Desktop Entry]"));
        assert!(content.contains("Exec=/usr/bin/quill"));
        assert!(content.contains("Terminal=false"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn desktop_exec_quotes_paths_with_spaces_and_specials() {
        assert_eq!(desktop_exec(Path::new("/usr/bin/quill")), "/usr/bin/quill");
        assert_eq!(
            desktop_exec(Path::new("/opt/My Apps/quill")),
            "\"/opt/My Apps/quill\""
        );
        // `$` and `"` are escaped for Exec quoting, with the backslash
        // doubled for the desktop-file string syntax.
        assert_eq!(
            desktop_exec(Path::new("/opt/a$b\"c/quill")),
            "\"/opt/a\\\\$b\\\\\"c/quill\""
        );
        // A literal percent must not read as a field code.
        assert_eq!(
            desktop_exec(Path::new("/opt/100%/quill")),
            "/opt/100%%/quill"
        );
    }

    #[test]
    fn linux_file_with_spaced_exe_keeps_exec_on_one_line() {
        let content = render_file(Platform::Linux, Path::new("/opt/My Apps/quill"));
        let exec: Vec<&str> = content.lines().filter(|l| l.starts_with("Exec=")).collect();
        assert_eq!(exec, vec!["Exec=\"/opt/My Apps/quill\""]);
    }

    #[test]
    fn plist_escapes_xml_in_the_exe_path() {
        let content = render_file(Platform::MacOs, Path::new("/Apps/R&D <x>/Quill"));
        assert!(content.contains("<string>/Apps/R&amp;D &lt;x&gt;/Quill</string>"));
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_file() {
        let home = tmp_home();
        let path = home.join(".config/autostart/quill.desktop");
        write_atomic(&path, "old").unwrap();
        write_atomic(&path, "new content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["quill.desktop".to_string()]);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn failed_write_keeps_the_previous_file_and_reports_io() {
        let home = tmp_home();
        let path = home.join(".config/autostart/quill.desktop");
        write_atomic(&path, "previous").unwrap();
        // Make the rename target a non-empty directory: the rename fails
        // after the temp file was written, so cleanup is exercised too.
        let blocked = home.join(".config/autostart/blocked.desktop");
        std::fs::create_dir_all(blocked.join("inner")).unwrap();
        assert!(write_atomic(&blocked, "new").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous");
        let temps = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(temps, 0, "temp file must be cleaned up");
        // An unwritable parent (a file where the directory should be)
        // maps to the user-facing Io error rather than a panic.
        let file_home = tmp_home();
        for dir in [".config", "Library"] {
            std::fs::write(file_home.join(dir), "not a dir").unwrap();
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(matches!(
            set_enabled_in(&file_home, true),
            Err(AutostartError::Io(_)) | Err(AutostartError::Unsupported)
        ));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&file_home);
    }

    #[test]
    fn macos_paths_and_plist() {
        let home = tmp_home();
        let path = autostart_path_in(&home, Platform::MacOs);
        assert_eq!(path, home.join("Library/LaunchAgents/com.quill.app.plist"));
        let exe = Path::new("/Applications/Quill.app/Contents/MacOS/Quill");
        let content = render_file(Platform::MacOs, exe);
        assert!(content.contains("<key>RunAtLoad</key>"));
        assert!(content.contains("<true/>"));
        assert!(content.contains("<string>/Applications/Quill.app/Contents/MacOS/Quill</string>"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn windows_run_value_quotes_the_exe_path() {
        assert_eq!(RUN_KEY, r"Software\Microsoft\Windows\CurrentVersion\Run");
        assert_eq!(RUN_VALUE_NAME, "Quill");
        assert_eq!(
            run_value(Path::new(r"C:\Program Files\Quill\quill.exe")),
            r#""C:\Program Files\Quill\quill.exe""#
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_registry_roundtrip_against_real_run_key() {
        // Real HKCU Run key, restored afterwards: the Windows CI job is the
        // only place this executes.
        let before = crate::winreg::get_string(RUN_KEY, RUN_VALUE_NAME).unwrap();
        set_enabled(true).unwrap();
        assert!(is_enabled());
        let value = crate::winreg::get_string(RUN_KEY, RUN_VALUE_NAME)
            .unwrap()
            .unwrap();
        assert!(value.starts_with('"') && value.ends_with('"'), "{value}");
        set_enabled(false).unwrap();
        assert!(!is_enabled());
        set_enabled(false).unwrap();
        if let Some(previous) = before {
            crate::winreg::set_string(RUN_KEY, RUN_VALUE_NAME, &previous).unwrap();
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn enable_disable_roundtrip() {
        // Exercises the enable/disable logic against a sandboxed home —
        // no HOME mutation (it is process-global).
        let home = tmp_home();
        assert!(!is_enabled_in(&home));
        set_enabled_in(&home, true).unwrap();
        assert!(is_enabled_in(&home));
        set_enabled_in(&home, false).unwrap();
        assert!(!is_enabled_in(&home));
        // Disabling twice is a no-op.
        set_enabled_in(&home, false).unwrap();
        let _ = std::fs::remove_dir_all(&home);
    }
}

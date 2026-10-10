//! Opt-in default handler for `tg:` links (Settings > "Open Telegram links
//! with Quill").
//!
//! Quill never claims the scheme on its own: a user who also runs Telegram
//! Desktop keeps their links. The Settings control calls [`make_default`],
//! and [`state`] tells the UI what the OS currently does.
//!
//! * macOS: `NSWorkspace setDefaultApplicationAtURL:toOpenURLsWithScheme:`
//!   (macOS 12+, the system asks the user to confirm) and
//!   `URLForApplicationToOpenURL` to read the current handler. There is no
//!   API to hand the scheme back to another app, so the UI offers a
//!   "Make default" button instead of a switch.
//! * Linux: `xdg-mime default quill.desktop x-scheme-handler/tg` and
//!   `xdg-mime query default x-scheme-handler/tg`. `xdg-mime` only accepts a
//!   desktop file that exists, so a build that was never installed (an
//!   unpacked tarball, an AppImage) first gets a per-user
//!   `~/.local/share/applications/quill.desktop`, written atomically like the
//!   autostart entry. An installed or packaged entry is left alone. Turning
//!   off restores nothing: xdg has no "previous" handler.
//! * Windows: per-user keys under `HKCU\Software\Classes\tg`, like
//!   tdesktop's `psRegisterCustomScheme`. Turning off removes only a
//!   registration whose command launches a Quill executable.
//!
//! Everything that decides something (parsing, the entry table, the
//! ownership rule) is pure so every platform tests it.

#[cfg(any(test, target_os = "macos"))]
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Who handles `tg:` links right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkHandlerState {
    /// This Quill (or another Quill build on Windows) is the handler.
    Ours,
    /// Another application, by display name.
    Other(String),
    /// Nothing is registered.
    Unset,
    /// This build cannot manage the handler (unbundled dev run, missing
    /// `xdg-mime`, unsupported OS). The text says why.
    Unavailable(String),
}

/// How the setting is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// On/off switch (Linux, Windows).
    Switch,
    /// One-way "Make default" button (macOS).
    Button,
}

/// Which control the current OS gets.
pub const fn control_kind() -> ControlKind {
    if cfg!(target_os = "macos") {
        ControlKind::Button
    } else {
        ControlKind::Switch
    }
}

/// Status line under the control.
pub fn status_text(state: &LinkHandlerState) -> String {
    match state {
        LinkHandlerState::Ours => "Quill is the default app for Telegram links.".into(),
        LinkHandlerState::Other(name) => format!("Telegram links currently open in {name}."),
        LinkHandlerState::Unset => "No app is set for Telegram links.".into(),
        LinkHandlerState::Unavailable(why) => why.clone(),
    }
}

/// What turning the setting off can and cannot do on this OS.
pub const fn turn_off_note() -> &'static str {
    if cfg!(target_os = "macos") {
        "To use another app, make it the default from that app."
    } else if cfg!(windows) {
        "Turning this off removes Quill's registration."
    } else {
        "Turning this off does not pick another app; choose one in your desktop settings."
    }
}

// ---- Linux ---------------------------------------------------------------

/// The desktop file `xdg-mime` is told about.
pub const LINUX_DESKTOP_FILE: &str = "quill.desktop";
/// The MIME type xdg uses for the `tg` scheme.
pub const LINUX_SCHEME_MIME: &str = "x-scheme-handler/tg";

/// Arguments of the command that makes Quill the default.
pub fn linux_set_args() -> [&'static str; 3] {
    ["default", LINUX_DESKTOP_FILE, LINUX_SCHEME_MIME]
}

/// Arguments of the command that prints the current default.
pub fn linux_query_args() -> [&'static str; 3] {
    ["query", "default", LINUX_SCHEME_MIME]
}

/// Interprets `xdg-mime query default x-scheme-handler/tg` output.
pub fn parse_xdg_default(output: &str) -> LinkHandlerState {
    let name = output.lines().next().unwrap_or("").trim();
    if name.is_empty() {
        LinkHandlerState::Unset
    } else if name == LINUX_DESKTOP_FILE {
        LinkHandlerState::Ours
    } else {
        LinkHandlerState::Other(name.strip_suffix(".desktop").unwrap_or(name).to_string())
    }
}

/// Where `xdg-mime` looks for `quill.desktop`: `$XDG_DATA_HOME` (default
/// `~/.local/share`) then each of `$XDG_DATA_DIRS` (default
/// `/usr/local/share:/usr/share`), each with an `applications` folder.
pub fn linux_application_dirs(
    data_home: Option<&str>,
    home: Option<&str>,
    data_dirs: Option<&str>,
) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    fn non_empty(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|v| !v.is_empty())
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    match (non_empty(data_home), non_empty(home)) {
        (Some(dir), _) => roots.push(PathBuf::from(dir)),
        (None, Some(home)) => roots.push(PathBuf::from(home).join(".local/share")),
        (None, None) => {}
    }
    let dirs = non_empty(data_dirs).unwrap_or("/usr/local/share:/usr/share");
    roots.extend(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    roots.into_iter().map(|r| r.join("applications")).collect()
}

/// The executable a desktop entry should launch: the AppImage file when
/// running from one (the mounted binary disappears on exit), else `exe`.
pub fn linux_launch_target(exe: &std::path::Path, appimage: Option<&str>) -> std::path::PathBuf {
    match appimage.map(str::trim).filter(|a| !a.is_empty()) {
        Some(image) => std::path::PathBuf::from(image),
        None => exe.to_path_buf(),
    }
}

/// Content of the per-user `quill.desktop` that makes Quill selectable for
/// `tg:` links. `%u` receives the link; `StartupWMClass` is not set because
/// the window class is not fixed across toolkits.
pub fn linux_desktop_entry(exe: &std::path::Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Quill\n\
         Comment=Unofficial Telegram desktop client\n\
         Exec={} %u\nIcon=quill\nTerminal=false\n\
         Categories=Network;Chat;\nMimeType={LINUX_SCHEME_MIME};\n",
        crate::autostart::desktop_exec(exe)
    )
}

/// Makes sure `xdg-mime` has a `quill.desktop` to point at. Returns the
/// file written, or `None` when one already exists in `app_dirs` (a package
/// or `linux-install.sh` put it there) and is left untouched. The first
/// directory is the per-user one that gets the new file.
pub fn ensure_linux_desktop_entry(
    app_dirs: &[std::path::PathBuf],
    exe: &std::path::Path,
) -> std::io::Result<Option<std::path::PathBuf>> {
    if app_dirs
        .iter()
        .any(|dir| dir.join(LINUX_DESKTOP_FILE).is_file())
    {
        return Ok(None);
    }
    let Some(user_dir) = app_dirs.first() else {
        return Err(std::io::Error::other("no applications folder"));
    };
    let path = user_dir.join(LINUX_DESKTOP_FILE);
    crate::autostart::write_atomic(&path, &linux_desktop_entry(exe))?;
    Ok(Some(path))
}

// ---- Windows -------------------------------------------------------------

/// One registry value: `(subkey under HKCU\Software\Classes, value name,
/// data)`. An empty name is the key's default value.
pub type RegistryEntry = (String, &'static str, String);

/// Subkey (under `HKCU\Software\Classes`) of the `open` command.
pub const WINDOWS_COMMAND_KEY: &str = "tg\\shell\\open\\command";

/// The keys that make Windows launch `exe` for `tg:` links, with the link
/// passed after `--` so it can never be parsed as a flag.
pub fn windows_scheme_entries(exe: &str) -> Vec<RegistryEntry> {
    vec![
        ("tg".into(), "", "URL:Telegram Link".into()),
        ("tg".into(), "URL Protocol", String::new()),
        ("tg\\DefaultIcon".into(), "", format!("\"{exe}\",0")),
        (
            WINDOWS_COMMAND_KEY.into(),
            "",
            format!("\"{exe}\" -- \"%1\""),
        ),
    ]
}

/// File name of the executable named by a registry `open` command (quoted
/// or bare).
fn command_file_name(command: &str) -> String {
    let command = command.trim();
    let exe = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next().unwrap_or("")
    } else {
        command.split_whitespace().next().unwrap_or("")
    };
    exe.replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Whether `command` launches a Quill build (any install path), so the
/// registration is ours to refresh or remove.
pub fn is_quill_command(command: &str) -> bool {
    command_file_name(command).eq_ignore_ascii_case("quill.exe")
}

/// State from the current `open` command.
pub fn windows_state(existing_command: Option<&str>) -> LinkHandlerState {
    match existing_command.map(str::trim) {
        None | Some("") => LinkHandlerState::Unset,
        Some(command) if is_quill_command(command) => LinkHandlerState::Ours,
        Some(command) => {
            let file = command_file_name(command);
            let name = file.strip_suffix(".exe").unwrap_or(&file);
            LinkHandlerState::Other(if name.is_empty() {
                "another app".into()
            } else {
                name.into()
            })
        }
    }
}

// ---- macOS ---------------------------------------------------------------

/// The `.app` bundle that contains `exe` (`Quill.app/Contents/MacOS/quill`),
/// or `None` for an unbundled binary such as `cargo run`.
#[cfg(any(test, target_os = "macos"))]
pub fn app_bundle_of_exe(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

/// State from the handler's bundle path and our own bundle.
#[cfg(any(test, target_os = "macos"))]
pub fn macos_state(handler: Option<&Path>, ours: Option<&Path>) -> LinkHandlerState {
    let Some(ours) = ours else {
        return LinkHandlerState::Unavailable(
            "Only the installed Quill.app can be made the default.".into(),
        );
    };
    match handler {
        None => LinkHandlerState::Unset,
        Some(h) if h == ours => LinkHandlerState::Ours,
        Some(h) => LinkHandlerState::Other(h.file_stem().map_or_else(
            || "another app".into(),
            |s| s.to_string_lossy().into_owned(),
        )),
    }
}

// ---- Platform glue -------------------------------------------------------

#[cfg(all(target_os = "macos", feature = "ui"))]
mod macos_impl {
    use super::{LinkHandlerState, app_bundle_of_exe, macos_state};
    use block2::RcBlock;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSError, NSString, NSURL};
    use std::path::PathBuf;

    fn our_bundle() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        app_bundle_of_exe(&exe.canonicalize().unwrap_or(exe))
    }

    pub fn state() -> LinkHandlerState {
        let ours = our_bundle();
        let handler = NSURL::URLWithString(&NSString::from_str("tg://resolve"))
            .and_then(|url| NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&url))
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .map(|path| path.canonicalize().unwrap_or(path));
        macos_state(handler.as_deref(), ours.as_deref())
    }

    pub fn make_default() -> Result<(), String> {
        let bundle = our_bundle()
            .ok_or_else(|| "Only the installed Quill.app can be made the default.".to_string())?;
        let app_url = NSURL::fileURLWithPath(&NSString::from_str(&bundle.to_string_lossy()));
        // The system shows its own confirmation; the result is read back by
        // `state()` afterwards, so the handler only logs a failure.
        let done = RcBlock::new(|error: *mut NSError| {
            if !error.is_null() {
                eprintln!("quill: could not set the tg: handler");
            }
        });
        NSWorkspace::sharedWorkspace()
            .setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(
                &app_url,
                &NSString::from_str("tg"),
                Some(&done),
            );
        Ok(())
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{LinkHandlerState, WINDOWS_COMMAND_KEY, windows_scheme_entries, windows_state};
    use crate::winreg;
    use std::io;

    const CLASSES: &str = "Software\\Classes\\";

    fn open_command() -> Option<String> {
        winreg::get_string(&format!("{CLASSES}{WINDOWS_COMMAND_KEY}"), "")
            .ok()
            .flatten()
    }

    pub fn state() -> LinkHandlerState {
        windows_state(open_command().as_deref())
    }

    pub fn make_default() -> io::Result<()> {
        let exe = std::env::current_exe()?;
        for (subkey, name, data) in windows_scheme_entries(&exe.to_string_lossy()) {
            winreg::set_string(&format!("{CLASSES}{subkey}"), name, &data)?;
        }
        Ok(())
    }

    /// Removes the registration only when it is a Quill's.
    pub fn release() -> io::Result<()> {
        if state() == LinkHandlerState::Ours {
            winreg::delete_tree(&format!("{CLASSES}tg"))?;
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn run_xdg_mime(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("xdg-mime")
        .args(args)
        .output()
        .map_err(|e| format!("xdg-mime is not available: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn query_state() -> LinkHandlerState {
    macos_impl::state()
}

#[cfg(windows)]
fn query_state() -> LinkHandlerState {
    windows_impl::state()
}

#[cfg(target_os = "linux")]
fn query_state() -> LinkHandlerState {
    match run_xdg_mime(&linux_query_args()) {
        Ok(out) => parse_xdg_default(&out),
        Err(why) => LinkHandlerState::Unavailable(why),
    }
}

#[cfg(not(any(all(target_os = "macos", feature = "ui"), windows, target_os = "linux")))]
fn query_state() -> LinkHandlerState {
    LinkHandlerState::Unavailable("Not available on this platform.".into())
}

const CACHE_TTL: Duration = Duration::from_secs(2);
static CACHE: Mutex<Option<(Instant, LinkHandlerState)>> = Mutex::new(None);

/// The current handler, cached briefly so rendering never spawns a process
/// per frame. Reflects external changes within a couple of seconds.
pub fn state() -> LinkHandlerState {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, state)) = cache.as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return state.clone();
    }
    let fresh = query_state();
    *cache = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Drops the cache so the next [`state`] asks the OS again.
pub fn invalidate() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn platform_make_default() -> Result<(), String> {
    macos_impl::make_default()
}

#[cfg(windows)]
fn platform_make_default() -> Result<(), String> {
    windows_impl::make_default().map_err(|e| format!("could not register Quill: {e}"))
}

#[cfg(target_os = "linux")]
fn platform_make_default() -> Result<(), String> {
    let env = |key| std::env::var(key).ok();
    let dirs = linux_application_dirs(
        env("XDG_DATA_HOME").as_deref(),
        env("HOME").as_deref(),
        env("XDG_DATA_DIRS").as_deref(),
    );
    let exe = std::env::current_exe().map_err(|e| format!("could not find Quill: {e}"))?;
    let target = linux_launch_target(&exe, env("APPIMAGE").as_deref());
    ensure_linux_desktop_entry(&dirs, &target)
        .map_err(|e| format!("could not write the desktop entry: {e}"))?;
    run_xdg_mime(&linux_set_args()).map(|_| ())
}

#[cfg(not(any(all(target_os = "macos", feature = "ui"), windows, target_os = "linux")))]
fn platform_make_default() -> Result<(), String> {
    Err("Not available on this platform.".into())
}

/// Makes Quill the default `tg:` handler (user-initiated only).
pub fn make_default() -> Result<(), String> {
    let result = platform_make_default();
    invalidate();
    result
}

/// Removes Quill's own registration where the OS allows it (Windows); a
/// no-op elsewhere because there is no previous handler to restore.
pub fn release() -> Result<(), String> {
    #[cfg(windows)]
    let result = windows_impl::release().map_err(|e| format!("could not remove registration: {e}"));
    #[cfg(not(windows))]
    let result = Ok(());
    invalidate();
    result
}

#[cfg(test)]
mod tests {
    use super::{
        LinkHandlerState, WINDOWS_COMMAND_KEY, app_bundle_of_exe, ensure_linux_desktop_entry,
        is_quill_command, linux_application_dirs, linux_desktop_entry, linux_launch_target,
        linux_query_args, linux_set_args, macos_state, parse_xdg_default, status_text,
        windows_scheme_entries, windows_state,
    };
    use std::path::Path;

    #[test]
    fn xdg_output_maps_to_state() {
        assert_eq!(parse_xdg_default("quill.desktop\n"), LinkHandlerState::Ours);
        assert_eq!(parse_xdg_default("\n"), LinkHandlerState::Unset);
        assert_eq!(parse_xdg_default(""), LinkHandlerState::Unset);
        assert_eq!(
            parse_xdg_default("org.telegram.desktop.desktop\n"),
            LinkHandlerState::Other("org.telegram.desktop".into())
        );
    }

    #[test]
    fn xdg_commands_target_the_tg_scheme() {
        assert_eq!(
            linux_set_args(),
            ["default", "quill.desktop", "x-scheme-handler/tg"]
        );
        assert_eq!(
            linux_query_args(),
            ["query", "default", "x-scheme-handler/tg"]
        );
    }

    #[test]
    fn entries_launch_the_exe_with_the_link_after_a_separator() {
        let entries = windows_scheme_entries(r"C:\Apps\Quill\quill.exe");
        let command = entries
            .iter()
            .find(|(key, _, _)| key == WINDOWS_COMMAND_KEY)
            .unwrap();
        assert_eq!(command.2, r#""C:\Apps\Quill\quill.exe" -- "%1""#);
        assert!(
            entries
                .iter()
                .any(|(k, n, _)| k == "tg" && *n == "URL Protocol")
        );
        let icon = entries
            .iter()
            .find(|(k, _, _)| k == "tg\\DefaultIcon")
            .unwrap();
        assert_eq!(icon.2, r#""C:\Apps\Quill\quill.exe",0"#);
    }

    #[test]
    fn windows_ownership_is_by_executable_name() {
        assert!(is_quill_command(r#""C:\old\Quill\quill.exe" -- "%1""#));
        assert!(is_quill_command(r#""D:\Apps\QUILL.EXE" -- "%1""#));
        assert!(!is_quill_command(
            r#""C:\Telegram Desktop\Telegram.exe" -- "%1""#
        ));
        assert!(!is_quill_command(r#""C:\Tools\quill-helper.exe" "%1""#));
        assert_eq!(windows_state(None), LinkHandlerState::Unset);
        assert_eq!(windows_state(Some("  ")), LinkHandlerState::Unset);
        assert_eq!(
            windows_state(Some(r#""C:\Apps\Quill\quill.exe" -- "%1""#)),
            LinkHandlerState::Ours
        );
        assert_eq!(
            windows_state(Some(r#""C:\Telegram Desktop\Telegram.exe" -- "%1""#)),
            LinkHandlerState::Other("Telegram".into())
        );
    }

    #[test]
    fn macos_state_compares_bundles() {
        let ours = Path::new("/Applications/Quill.app");
        assert_eq!(
            app_bundle_of_exe(Path::new("/Applications/Quill.app/Contents/MacOS/quill")),
            Some(ours.to_path_buf())
        );
        assert_eq!(
            app_bundle_of_exe(Path::new("/tmp/target/debug/quill")),
            None
        );
        assert_eq!(macos_state(Some(ours), Some(ours)), LinkHandlerState::Ours);
        assert_eq!(
            macos_state(Some(Path::new("/Applications/Telegram.app")), Some(ours)),
            LinkHandlerState::Other("Telegram".into())
        );
        assert_eq!(macos_state(None, Some(ours)), LinkHandlerState::Unset);
        assert!(matches!(
            macos_state(Some(ours), None),
            LinkHandlerState::Unavailable(_)
        ));
    }

    #[test]
    fn status_text_names_the_current_handler() {
        assert!(status_text(&LinkHandlerState::Ours).contains("Quill is the default"));
        assert!(status_text(&LinkHandlerState::Other("Telegram".into())).contains("Telegram"));
    }

    #[test]
    fn application_dirs_follow_the_xdg_defaults() {
        let dirs = linux_application_dirs(None, Some("/home/a"), None);
        assert_eq!(
            dirs,
            [
                "/home/a/.local/share/applications",
                "/usr/local/share/applications",
                "/usr/share/applications"
            ]
            .map(Path::new)
        );
        let dirs =
            linux_application_dirs(Some("/data"), Some("/home/a"), Some("/opt/s::/usr/share"));
        assert_eq!(
            dirs,
            [
                "/data/applications",
                "/opt/s/applications",
                "/usr/share/applications"
            ]
            .map(Path::new)
        );
        // An empty variable counts as unset, per the XDG spec.
        let dirs = linux_application_dirs(Some(""), Some("/home/a"), Some(""));
        assert_eq!(dirs[0], Path::new("/home/a/.local/share/applications"));
    }

    #[test]
    fn desktop_entry_advertises_the_scheme_and_quotes_the_path() {
        let plain = linux_desktop_entry(Path::new("/opt/quill/quill"));
        assert!(plain.contains("Exec=/opt/quill/quill %u\n"));
        assert!(plain.contains("MimeType=x-scheme-handler/tg;\n"));
        assert!(plain.starts_with("[Desktop Entry]\nType=Application\n"));
        let spaced = linux_desktop_entry(Path::new("/home/a b/Quill/quill"));
        assert!(spaced.contains("Exec=\"/home/a b/Quill/quill\" %u\n"));
    }

    #[test]
    fn appimage_path_wins_over_the_mounted_binary() {
        let exe = Path::new("/tmp/.mount_Quill/usr/bin/quill");
        assert_eq!(
            linux_launch_target(exe, Some("/home/a/Quill.AppImage")),
            Path::new("/home/a/Quill.AppImage")
        );
        assert_eq!(linux_launch_target(exe, Some(" ")), exe);
        assert_eq!(linux_launch_target(exe, None), exe);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quill-link-handler-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_desktop_entry_is_written_to_the_user_folder() {
        let root = temp_dir("write");
        let user = root.join("home/applications");
        let system = root.join("sys/applications");
        let exe = Path::new("/opt/quill/quill");
        let written = ensure_linux_desktop_entry(&[user.clone(), system], exe)
            .unwrap()
            .expect("written");
        assert_eq!(written, user.join("quill.desktop"));
        let content = std::fs::read_to_string(&written).unwrap();
        assert_eq!(content, linux_desktop_entry(exe));
        // No temp file is left next to it.
        let names: Vec<_> = std::fs::read_dir(&user)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["quill.desktop"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn installed_desktop_entry_is_left_alone() {
        let root = temp_dir("keep");
        let user = root.join("home/applications");
        let system = root.join("sys/applications");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("quill.desktop"), "packaged").unwrap();
        let out = ensure_linux_desktop_entry(&[user.clone(), system], Path::new("/x/quill"));
        assert_eq!(out.unwrap(), None);
        assert!(!user.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}

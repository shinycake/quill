//! "About Quill" text and the location of the bundled license notices
//! (docs/decisions/codex-legal-compliance.md).

use std::path::{Path, PathBuf};

/// One line for the sign-in screen. Telegram's API terms (section 2.2) ask
/// third-party apps to tell users in the in-app intro that the app uses the
/// Telegram API.
pub const INTRO_NOTICE: &str =
    "Quill is an unofficial app that uses the Telegram API. It is not affiliated with Telegram.";

/// The short disclaimer shown in Settings next to the version.
pub const DISCLAIMER: &str = "Quill is an independent, unofficial Telegram client. It is not \
affiliated with, endorsed by, or sponsored by Telegram. \"Telegram\" is a trademark of its \
owner and is used here only to describe compatibility. Quill is early, experimental \
software provided under the MIT License, without warranty of any kind. Keep an official \
Telegram app installed and back up anything important.";

/// The notices index shipped next to the executable by the package scripts
/// (scripts/stage-licenses.sh, scripts/windows-package.ps1).
pub const NOTICES_FILE: &str = "THIRD_PARTY.md";

/// Where to read the notices when the bundled copy is missing (a `cargo run`
/// build, or a package assembled before the notices were staged).
pub const NOTICES_URL: &str = "https://github.com/shinycake/quill/blob/main/THIRD_PARTY.md";

/// Places a package keeps `THIRD_PARTY.md` for an executable at `exe`:
/// beside it (Linux and Windows packages) and in `Contents/Resources`
/// (macOS app bundle, executable in `Contents/MacOS`).
pub fn notices_candidates(exe: &Path) -> Vec<PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    vec![
        dir.join(NOTICES_FILE),
        dir.join("..").join("Resources").join(NOTICES_FILE),
    ]
}

/// The bundled `THIRD_PARTY.md` for the running executable, if present.
pub fn bundled_notices() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    notices_candidates(&exe).into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_cover_flat_packages_and_mac_bundles() {
        let flat = notices_candidates(Path::new("/opt/quill/quill"));
        assert_eq!(flat[0], Path::new("/opt/quill/THIRD_PARTY.md"));
        let mac = notices_candidates(Path::new("/Applications/Quill.app/Contents/MacOS/quill"));
        assert_eq!(
            mac[1],
            Path::new("/Applications/Quill.app/Contents/MacOS/../Resources/THIRD_PARTY.md")
        );
    }

    #[test]
    fn bundled_notices_found_beside_executable() {
        let dir = std::env::temp_dir().join(format!("quill-about-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Contents/MacOS")).unwrap();
        std::fs::create_dir_all(dir.join("Contents/Resources")).unwrap();
        std::fs::write(dir.join("Contents/Resources/THIRD_PARTY.md"), "notices").unwrap();
        let found = notices_candidates(&dir.join("Contents/MacOS/quill"))
            .into_iter()
            .find(|p| p.is_file());
        assert!(found.is_some_and(|p| p.ends_with("Resources/THIRD_PARTY.md")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

//! `quill-cli`: runs the Quill executable next to it with the same
//! arguments, waits for it, copies its output and exits with its code.
//!
//! On Windows, release builds of quill.exe use the GUI subsystem (no console
//! window when Quill opens), so cmd and PowerShell don't wait for it: its
//! output lands after the next prompt and %ERRORLEVEL% never gets its exit
//! code. quill-cli.exe is a console program, so the terminal waits for it,
//! and `quill-cli agent status` behaves like any command-line tool. The
//! output comes through pipes, which a GUI program writes to normally.
//!
//! It uses only std, so it stays small and doesn't link the app.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// The exit code when Quill's executable can't be run (`quill agent`'s
/// "local file" code).
const CANNOT_RUN: u8 = 8;

/// The Quill executable beside `shim`.
fn sibling(shim: &Path) -> PathBuf {
    shim.with_file_name(if cfg!(windows) { "quill.exe" } else { "quill" })
}

/// Copy `from` to `to` until it closes; a closed `to` just stops the copy.
fn pump(mut from: impl std::io::Read, mut to: impl Write) {
    let _ = std::io::copy(&mut from, &mut to);
    let _ = to.flush();
}

fn main() -> ExitCode {
    let quill = match std::env::current_exe() {
        Ok(shim) => sibling(&shim),
        Err(e) => return fail(&format!("could not find its own path ({e})")),
    };
    let child = Command::new(&quill)
        .args(std::env::args_os().skip(1))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return fail(&format!("could not run {} ({e})", quill.display())),
    };
    let out = child
        .stdout
        .take()
        .map(|from| std::thread::spawn(move || pump(from, std::io::stdout())));
    let err = child
        .stderr
        .take()
        .map(|from| std::thread::spawn(move || pump(from, std::io::stderr())));
    let status = child.wait();
    for pump in [out, err].into_iter().flatten() {
        let _ = pump.join();
    }
    match status {
        Ok(status) => ExitCode::from(
            status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1),
        ),
        Err(e) => fail(&format!("lost {} ({e})", quill.display())),
    }
}

fn fail(what: &str) -> ExitCode {
    let _ = writeln!(
        std::io::stderr().lock(),
        "quill-cli {what}. It runs the Quill executable in its own folder; reinstall Quill if that is missing."
    );
    ExitCode::from(CANNOT_RUN)
}

#[cfg(test)]
mod tests {
    use super::sibling;
    use std::path::Path;

    #[test]
    fn runs_the_quill_beside_it() {
        let quill = sibling(Path::new("/opt/Quill/quill-cli"));
        let want = if cfg!(windows) { "quill.exe" } else { "quill" };
        assert_eq!(quill, Path::new("/opt/Quill").join(want));
    }
}

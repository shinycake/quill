//! Windows spellcheck backend: the system `ISpellChecker` (Windows 8+
//! Spell Checking API), as Telegram Desktop's
//! `lib_spellcheck/spellcheck/platform/win/spellcheck_win.cpp`.
//!
//! ISpellChecker is not thread-safe, so every COM object lives on one
//! dedicated apartment thread and the backend posts jobs to it (tdesktop's
//! `ComThread`). Languages are the spelling languages Windows has enabled
//! for the user (`%APPDATA%\Microsoft\Spelling\<tag>` folders, else the
//! user locale), as tdesktop's `SystemLanguages()`, filtered by
//! `ISpellCheckerFactory::IsSupported`.
//!
//! Learned words stay in Quill's own list (the caller persists them):
//! `ISpellChecker` can't report which words were added, so adding to the
//! shared Windows dictionary would leave "Remove from Dictionary"
//! unable to know what to undo.

use std::sync::{Mutex, mpsc};

use windows::Win32::Globalization::{
    CORRECTIVE_ACTION_GET_SUGGESTIONS, CORRECTIVE_ACTION_REPLACE, ISpellChecker,
    ISpellCheckerFactory, SpellCheckerFactory,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::core::{HSTRING, PCWSTR};

use crate::spellcheck::{Script, SpellBackend, locale_script, word_script};

/// tdesktop skips Persian: "ISpellChecker API has bugs for Persian".
fn is_persian(tag: &str) -> bool {
    tag.starts_with("fa")
}

type Job = Box<dyn FnOnce(&mut Checkers) + Send>;

struct Checkers {
    list: Vec<(String, ISpellChecker)>,
}

/// Languages to try, most preferred first: the Windows spelling-language
/// folders, else the user locale.
fn candidate_tags() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA")
        && let Ok(entries) =
            std::fs::read_dir(std::path::Path::new(&appdata).join(r"Microsoft\Spelling"))
    {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir())
                && let Some(name) = entry.file_name().to_str()
            {
                out.push(name.to_string());
            }
        }
    }
    out
}

fn init(
    wanted_locales: &[String],
    chosen: &[String],
) -> Option<(
    ISpellCheckerFactory,
    Vec<(String, ISpellChecker)>,
    Vec<String>,
)> {
    // SAFETY: standard COM activation on this thread's own apartment.
    let factory: ISpellCheckerFactory = unsafe {
        CoCreateInstance(
            &SpellCheckerFactory,
            None,
            CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER,
        )
    }
    .ok()?;
    let supported = |tag: &str| {
        let h = HSTRING::from(tag);
        // SAFETY: `h` outlives the call.
        unsafe { factory.IsSupported(&h) }.is_ok_and(|b| b.as_bool())
    };
    let mut tags: Vec<String> = Vec::new();
    for tag in wanted_locales
        .iter()
        .cloned()
        .chain(candidate_tags())
        .filter(|t| !is_persian(t) && supported(t))
    {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    let available = tags.clone();
    let picked: Vec<String> = if chosen.is_empty() {
        tags
    } else {
        let p: Vec<String> = chosen
            .iter()
            .filter(|c| tags.contains(c))
            .cloned()
            .collect();
        if p.is_empty() { tags } else { p }
    };
    let mut list = Vec::new();
    for tag in picked {
        let h = HSTRING::from(tag.as_str());
        // SAFETY: `h` outlives the call.
        if let Ok(checker) = unsafe { factory.CreateSpellChecker(&h) } {
            list.push((tag, checker));
        }
    }
    Some((factory, list, available))
}

pub struct WindowsSpellBackend {
    tx: Mutex<mpsc::Sender<Job>>,
    languages: Vec<(String, Script)>,
    available: Vec<String>,
}

impl WindowsSpellBackend {
    /// Starts the COM thread. `locales` are the user locale tags
    /// (`en-US`); `chosen` the user's explicit language picks (empty:
    /// automatic). `None` when ISpellChecker is unavailable or has no
    /// supported language.
    pub fn new(locales: &[String], chosen: &[String]) -> Option<Self> {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Option<(Vec<String>, Vec<String>)>>();
        let locales = locales.to_vec();
        let chosen = chosen.to_vec();
        std::thread::Builder::new()
            .name("quill-spell-com".into())
            .spawn(move || {
                // SAFETY: balanced with `CoUninitialize` below.
                let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                let initialized = hr.is_ok();
                match init(&locales, &chosen) {
                    Some((_factory, list, available)) if !list.is_empty() => {
                        let tags: Vec<String> = list.iter().map(|(t, _)| t.clone()).collect();
                        let _ = ready_tx.send(Some((tags, available)));
                        let mut checkers = Checkers { list };
                        while let Ok(job) = job_rx.recv() {
                            job(&mut checkers);
                        }
                    }
                    _ => {
                        let _ = ready_tx.send(None);
                    }
                }
                if initialized {
                    // SAFETY: matches the successful init above; COM
                    // objects were dropped with `checkers`/`_factory`.
                    unsafe { CoUninitialize() };
                }
            })
            .ok()?;
        let (tags, available) = ready_rx.recv().ok()??;
        let languages = tags
            .into_iter()
            .filter_map(|tag| {
                let script = locale_script(&tag).filter(|s| s.is_checkable())?;
                Some((tag, script))
            })
            .collect::<Vec<_>>();
        (!languages.is_empty()).then_some(Self {
            tx: Mutex::new(job_tx),
            languages,
            available,
        })
    }

    /// Every spelling language Windows offers this user.
    pub fn available_languages(&self) -> Vec<String> {
        self.available.clone()
    }

    /// Languages in use.
    pub fn active_languages(&self) -> Vec<String> {
        self.languages.iter().map(|(t, _)| t.clone()).collect()
    }

    fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Checkers) -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = mpsc::channel();
        let job: Job = Box::new(move |c| {
            let _ = tx.send(f(c));
        });
        self.tx.lock().ok()?.send(job).ok()?;
        rx.recv().ok()
    }

    /// Which checkers apply to this word (by script).
    fn tags_for(&self, word: &str) -> Vec<String> {
        let script = word_script(word);
        self.languages
            .iter()
            .filter(|(_, s)| Some(*s) == script)
            .map(|(t, _)| t.clone())
            .collect()
    }
}

/// The user locale tags for [`WindowsSpellBackend::new`], via the same
/// BCP-47 environment Windows exposes to Rust (`LANG`-less): the system
/// default locale name.
pub fn user_locale_tags() -> Vec<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is LOCALE_NAME_MAX_LENGTH wide.
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if len <= 1 {
        return Vec::new();
    }
    vec![String::from_utf16_lossy(&buf[..len as usize - 1])]
}

impl SpellBackend for WindowsSpellBackend {
    fn handles(&self, script: Script, _word: &str) -> bool {
        self.languages.iter().any(|(_, s)| *s == script)
    }

    fn is_correct(&self, word: &str) -> bool {
        let tags = self.tags_for(word);
        let word = word.to_string();
        self.run(move |c| {
            let mut judged = false;
            for (tag, checker) in &c.list {
                if !tags.contains(tag) {
                    continue;
                }
                judged = true;
                let h = HSTRING::from(word.as_str());
                // SAFETY: `h` outlives the call; COM on its own thread.
                let Ok(errors) = (unsafe { checker.Check(&h) }) else {
                    return true;
                };
                // No error (Next fails on the empty enumeration) or an
                // error that is no misspelling (e.g. a repeated word):
                // correct in this language.
                let mut found = None;
                let hr = unsafe { errors.Next(&mut found) };
                let Some(error) = found.filter(|_| hr.is_ok()) else {
                    return true;
                };
                let action = unsafe { error.CorrectiveAction() };
                if !matches!(
                    action,
                    Ok(a) if a == CORRECTIVE_ACTION_GET_SUGGESTIONS || a == CORRECTIVE_ACTION_REPLACE
                ) {
                    return true;
                }
            }
            !judged
        })
        .unwrap_or(true)
    }

    fn suggestions(&self, word: &str, limit: usize) -> Vec<String> {
        let tags = self.tags_for(word);
        let word = word.to_string();
        self.run(move |c| {
            let mut out: Vec<String> = Vec::new();
            for (tag, checker) in &c.list {
                if !tags.contains(tag) {
                    continue;
                }
                let h = HSTRING::from(word.as_str());
                // SAFETY: as in `is_correct`.
                let Ok(list) = (unsafe { checker.Suggest(&h) }) else {
                    continue;
                };
                loop {
                    let mut item = [windows::core::PWSTR::null()];
                    let hr = unsafe { list.Next(&mut item, None) };
                    if hr != windows::Win32::Foundation::S_OK || item[0].is_null() {
                        break;
                    }
                    // SAFETY: COM allocated a NUL-terminated string.
                    let text = unsafe { PCWSTR(item[0].0).to_string() }.unwrap_or_default();
                    unsafe { CoTaskMemFree(Some(item[0].0 as *const _)) };
                    if !text.is_empty() && !out.contains(&text) {
                        out.push(text);
                        if out.len() >= limit {
                            return out;
                        }
                    }
                }
            }
            out
        })
        .unwrap_or_default()
    }
}

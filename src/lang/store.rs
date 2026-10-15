//! The strings of the chosen language pack, as one immutable snapshot that
//! render code reads lock-free and the language flow swaps whole.

use std::sync::Arc;

use arc_swap::ArcSwapOption;

use super::intern::intern;
use super::key_table::{self, KEY_COUNT};
use super::parse::{Form, split_plural};
use super::plural::PluralRules;

/// A string value as a language pack carries it (TDLib
/// `LanguagePackStringValue`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PackValue {
    Ordinary(String),
    /// The six CLDR forms, indexed by [`Form`]; empty means absent.
    Pluralized([String; 6]),
    /// `languagePackStringValueDeleted`: falls back to the next layer.
    Deleted,
}

/// One interface string of a snapshot.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Text(&'static str),
    Plural(Box<[Option<&'static str>; 6]>),
}

/// The strings Quill renders with: the chosen pack layered over its base
/// pack and Quill's community overlay, indexed by key. A key with no value
/// falls back to the built-in English.
#[derive(Debug)]
pub struct Snapshot {
    pack_id: String,
    is_rtl: bool,
    plural: PluralRules,
    values: Vec<Option<Value>>,
}

impl Snapshot {
    /// An empty snapshot for `pack_id` (every key English until layers are
    /// added). `plural_code` is the pack's `plural_code` (CLDR locale).
    pub fn new(pack_id: &str, plural_code: &str, is_rtl: bool) -> Self {
        Self {
            pack_id: pack_id.to_string(),
            is_rtl,
            plural: PluralRules::for_code(plural_code),
            values: vec![None; KEY_COUNT],
        }
    }

    pub fn pack_id(&self) -> &str {
        &self.pack_id
    }

    pub fn is_rtl(&self) -> bool {
        self.is_rtl
    }

    /// Writes `strings` over the current values: later layers win, a
    /// deleted string clears the key back to the lower layers' value only
    /// when applied as a patch (see [`Snapshot::patched`]); in a layer it is
    /// skipped. Keys Quill does not use are ignored.
    pub fn layer<'a>(&mut self, strings: impl IntoIterator<Item = (&'a str, &'a PackValue)>) {
        for (key, value) in strings {
            self.set(key, value, false);
        }
    }

    /// A copy with `updateLanguagePackStrings` applied: changed values
    /// replace the old ones, deleted ones fall back to English.
    pub fn patched<'a>(&self, strings: impl IntoIterator<Item = (&'a str, &'a PackValue)>) -> Self {
        let mut next = Self {
            pack_id: self.pack_id.clone(),
            is_rtl: self.is_rtl,
            plural: self.plural.clone(),
            values: self.values.clone(),
        };
        for (key, value) in strings {
            next.set(key, value, true);
        }
        next
    }

    /// Adds the entries of a `.strings` file (Quill's community overlay):
    /// `key#form` entries fill one plural form each.
    pub fn layer_entries(&mut self, entries: &[super::parse::Entry]) {
        for entry in entries {
            let (base, form) = split_plural(&entry.key);
            let Some(index) = key_table::index_of(base) else {
                continue;
            };
            let Some(slot) = self.values.get_mut(index) else {
                continue;
            };
            let text = intern(&entry.value);
            match form {
                None => *slot = Some(Value::Text(text)),
                Some(form) => {
                    let mut forms = match slot.take() {
                        Some(Value::Plural(forms)) => forms,
                        _ => Box::default(),
                    };
                    if let Some(f) = forms.get_mut(form.index()) {
                        *f = Some(text);
                    }
                    *slot = Some(Value::Plural(forms));
                }
            }
        }
    }

    fn set(&mut self, key: &str, value: &PackValue, deleting: bool) {
        let Some(index) = key_table::index_of(key) else {
            return;
        };
        let Some(slot) = self.values.get_mut(index) else {
            return;
        };
        match value {
            PackValue::Ordinary(text) => *slot = Some(Value::Text(intern(text))),
            PackValue::Pluralized(forms) => {
                let mut out: [Option<&'static str>; 6] = Default::default();
                for (dst, src) in out.iter_mut().zip(forms) {
                    if !src.is_empty() {
                        *dst = Some(intern(src));
                    }
                }
                *slot = Some(Value::Plural(Box::new(out)));
            }
            PackValue::Deleted if deleting => *slot = None,
            PackValue::Deleted => {}
        }
    }

    pub(crate) fn value(&self, index: u16) -> Option<&Value> {
        self.values.get(usize::from(index))?.as_ref()
    }

    pub(crate) fn plural_form(&self, count: i64) -> Form {
        self.plural.form_for(count)
    }

    /// The snapshot's strings by key name (`key` for ordinary values; a
    /// plural key once with all its forms), for the startup cache.
    pub fn entries(&self) -> Vec<(String, PackValue)> {
        let mut out = Vec::new();
        for (name, value) in key_table::KEY_NAMES.iter().zip(&self.values) {
            let value = match value {
                Some(Value::Text(text)) => PackValue::Ordinary(text.to_string()),
                Some(Value::Plural(forms)) => PackValue::Pluralized(
                    (**forms).map(|form| form.map(str::to_string).unwrap_or_default()),
                ),
                None => continue,
            };
            out.push(((*name).to_string(), value));
        }
        out
    }

    /// How many of the keys this snapshot translates (for the "% translated"
    /// line of Quill-only strings).
    pub fn translated_count(&self, prefix: &str) -> (usize, usize) {
        let mut done = 0;
        let mut total = 0;
        for (name, value) in key_table::KEY_NAMES.iter().zip(&self.values) {
            if name.starts_with(prefix) {
                total += 1;
                if value.is_some() {
                    done += 1;
                }
            }
        }
        (done, total)
    }
}

/// The snapshot in use; `None` is the built-in English.
static CURRENT: ArcSwapOption<Snapshot> = ArcSwapOption::const_empty();

thread_local! {
    /// The strings of a thread that called [`isolate_thread`]: tests that
    /// switch languages keep the switch to themselves.
    static LOCAL: std::cell::RefCell<Option<Option<Arc<Snapshot>>>> =
        const { std::cell::RefCell::new(None) };
}

/// From now on, [`install`] and every lookup on this thread use strings of
/// its own, starting with English. For tests that switch languages while
/// other tests run in parallel.
pub fn isolate_thread() {
    LOCAL.with(|local| {
        if let Ok(mut local) = local.try_borrow_mut() {
            *local = Some(None);
        }
    });
}

/// Installs into this thread's own strings when it has them.
fn install_local(snapshot: &mut Option<Option<Arc<Snapshot>>>) -> bool {
    LOCAL.with(|local| match local.try_borrow_mut() {
        Ok(mut local) if local.is_some() => {
            *local = snapshot.take();
            true
        }
        _ => false,
    })
}

/// Makes `snapshot` the strings every later lookup uses (`None`: English).
/// The caller refreshes the windows.
pub fn install(snapshot: Option<Snapshot>) {
    let mut local = Some(snapshot.map(Arc::new));
    if install_local(&mut local) {
        return;
    }
    if let Some(snapshot) = local {
        CURRENT.store(snapshot);
    }
}

/// The snapshot in use, if a pack other than the built-in English is.
pub fn current() -> Option<Arc<Snapshot>> {
    if let Some(local) = local_snapshot() {
        return local;
    }
    CURRENT.load_full()
}

/// This thread's own snapshot after [`isolate_thread`].
fn local_snapshot() -> Option<Option<Arc<Snapshot>>> {
    LOCAL.with(|local| local.try_borrow().ok().and_then(|local| local.clone()))
}

/// Runs `f` with the current snapshot without cloning the `Arc`.
pub(crate) fn with_current<R>(f: impl FnOnce(Option<&Snapshot>) -> R) -> R {
    if let Some(local) = local_snapshot() {
        return f(local.as_deref());
    }
    let guard = CURRENT.load();
    f(guard.as_deref())
}

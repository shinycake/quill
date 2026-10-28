//! Build script:
//! - turns `lang/en/*.strings` into the interface string keys
//!   (`OUT_DIR/lang_keys.rs`, included by `src/lang/keys.rs`);
//! - on Windows, embeds `assets/icons/Quill.ico` as the application icon of
//!   `quill.exe`, and gives its main thread an 8 MB stack;
//! - compiles the vendored ThorVG (`third_party/thorvg`, the animated sticker
//!   renderer) into a static library.

#[path = "src/lang/parse.rs"]
#[allow(dead_code)]
mod parse;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lang/parse.rs");
    let manifest = PathBuf::from(env("CARGO_MANIFEST_DIR")?);
    let out = PathBuf::from(env("OUT_DIR")?);
    lang_keys(&manifest, &out)?;
    thorvg(&manifest, &out)?;
    #[cfg(windows)]
    windows_icon(&manifest, &out)?;
    windows_main_stack();
    Ok(())
}

/// Windows gives the main thread a 1 MB stack, macOS and Linux 8 MB. GPUI
/// runs the whole UI on the main thread, so give `quill.exe` the same 8 MB.
fn windows_main_stack() {
    const MAIN_STACK: u32 = 8 * 1024 * 1024;
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:{MAIN_STACK}"),
            Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{MAIN_STACK}"),
            _ => {}
        }
    }
}

/// ThorVG with only what stickers need: the CPU rasterizer, the Lottie loader
/// (no expressions, no file IO) and the C API. Built the same in every
/// profile, so debug builds decode at release speed and the golden-frame test
/// sees the same pixels everywhere. See docs/decisions/thorvg-sticker-renderer.md.
fn thorvg(manifest: &Path, out: &Path) -> Result<(), String> {
    let root = manifest.join("third_party").join("thorvg");
    println!("cargo:rerun-if-changed={}", root.display());
    let version_file = root.join("VERSION");
    let version_text = std::fs::read_to_string(&version_file)
        .map_err(|err| format!("{}: {err}", version_file.display()))?;
    let version = version_text
        .lines()
        .find_map(|line| line.strip_prefix("version "))
        .ok_or_else(|| format!("{}: no version line", version_file.display()))?;
    let config_dir = out.join("thorvg");
    std::fs::create_dir_all(&config_dir)
        .map_err(|err| format!("{}: {err}", config_dir.display()))?;
    // No THORVG_THREAD_SUPPORT: Quill decodes on its own worker threads, one
    // canvas per decode, and ThorVG's scheduler then runs every task inline on
    // the calling thread. Its process-wide state stays behind `StrictKey`,
    // which is a real mutex only when the compiler defines
    // __STDCPP_THREADS__, so the build stops if one does not.
    // No THORVG_PARTIAL_RENDER_SUPPORT: each frame is a full redraw.
    let config = format!(
        "#define THORVG_VERSION_STRING \"{version}\"\n\
         #define THORVG_CPU_ENGINE_SUPPORT 1\n\
         #define THORVG_LOTTIE_LOADER_SUPPORT 1\n\
         #define THORVG_CAPI_BINDING_SUPPORT 1\n\
         #if !defined(__STDCPP_THREADS__)\n\
         #error \"ThorVG's StrictKey needs __STDCPP_THREADS__ to lock\"\n\
         #endif\n"
    );
    let config_path = config_dir.join("config.h");
    std::fs::write(&config_path, config)
        .map_err(|err| format!("{}: {err}", config_path.display()))?;

    let source_dirs = [
        "src/common",
        "src/renderer",
        "src/renderer/cpu_engine",
        "src/loaders/raw",
        "src/loaders/lottie",
        "src/bindings/capi",
    ];
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++14")
        .opt_level(3)
        .debug(false)
        .warnings(false)
        .cargo_warnings(false)
        .include(&config_dir)
        .include(root.join("inc"))
        .define("TVG_STATIC", None)
        .define("NDEBUG", None);
    for dir in source_dirs {
        let dir = root.join(dir);
        build.include(&dir);
        let mut sources: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|err| format!("{}: {err}", dir.display()))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
            .collect();
        sources.sort();
        build.files(sources);
    }
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        build
            .define("NOMINMAX", None)
            .define("WIN32_LEAN_AND_MEAN", None);
    }
    if build.get_compiler().is_like_msvc() {
        // ThorVG has no try/catch and no dynamic_cast; _HAS_EXCEPTIONS=0
        // keeps the MSVC STL headers consistent with /EHs-c-.
        build
            .flag("/EHs-c-")
            .flag("/GR-")
            .define("_HAS_EXCEPTIONS", "0");
    } else {
        // Same as ThorVG's meson build for GCC and Clang, except that the
        // stack protector and unwind tables stay on: the input is untrusted,
        // and crash reports need to walk through these frames.
        build
            .flag("-fno-exceptions")
            .flag("-fno-rtti")
            .flag("-fno-math-errno");
    }
    build.try_compile("thorvg").map_err(|err| err.to_string())
}

fn env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|err| format!("{name}: {err}"))
}

/// A key's English text: one value, or the plural forms by `parse::Form`.
enum English {
    Text(String),
    Plural([Option<String>; 6]),
}

fn lang_keys(manifest: &Path, out: &Path) -> Result<(), String> {
    let dir = manifest.join("lang").join("en");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|err| format!("{}: {err}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "strings"))
        .collect();
    files.sort();
    let mut keys: BTreeMap<String, (English, String)> = BTreeMap::new();
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let text =
            std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
        let name = path.display().to_string();
        let entries = parse::parse_strings(&text).map_err(|err| format!("{name}: {err}"))?;
        // Keys of this file, so a plural key may not be split across files
        // and a key may repeat across files only with the same text.
        let mut local: BTreeMap<String, English> = BTreeMap::new();
        for entry in entries {
            let at = format!("{name}:{}", entry.line);
            if !parse::braces_balanced(&entry.value) {
                return Err(format!("{at}: unbalanced braces in {:?}", entry.key));
            }
            let (base, form) = parse::split_plural(&entry.key);
            if !valid_key(base) {
                return Err(format!(
                    "{at}: key {base:?} must be lng_* or qll_* in [a-z0-9_]"
                ));
            }
            match (local.get_mut(base), form) {
                (None, None) => {
                    local.insert(base.to_string(), English::Text(entry.value));
                }
                (None, Some(form)) => {
                    let mut forms: [Option<String>; 6] = Default::default();
                    if let Some(slot) = forms.get_mut(form.index()) {
                        *slot = Some(entry.value);
                    }
                    local.insert(base.to_string(), English::Plural(forms));
                }
                (Some(English::Plural(forms)), Some(form)) => {
                    let slot = forms
                        .get_mut(form.index())
                        .ok_or_else(|| format!("{at}: bad plural form"))?;
                    if slot.is_some() {
                        return Err(format!("{at}: duplicate key {:?}", entry.key));
                    }
                    *slot = Some(entry.value);
                }
                _ => return Err(format!("{at}: duplicate key {:?}", entry.key)),
            }
        }
        for (key, english) in local {
            if let English::Plural(forms) = &english
                && forms
                    .get(parse::Form::Other.index())
                    .is_none_or(Option::is_none)
            {
                return Err(format!("{name}: plural key {key:?} has no #other form"));
            }
            match keys.get(&key) {
                Some((existing, first)) => {
                    if !same(existing, &english) {
                        return Err(format!(
                            "{name}: key {key:?} differs from its definition in {first}"
                        ));
                    }
                }
                None => {
                    keys.insert(key, (english, name.clone()));
                }
            }
        }
    }
    if keys.len() > usize::from(u16::MAX) {
        return Err("too many interface strings".into());
    }
    let mut names = BTreeMap::new();
    let mut code = String::new();
    code.push_str("// Generated by build.rs from lang/en/*.strings. Do not edit.\n");
    code.push_str(&format!(
        "/// Number of interface string keys.\npub(crate) const KEY_COUNT: usize = {};\n",
        keys.len()
    ));
    code.push_str("/// Key names, sorted, so a pack's strings map to keys by binary search.\n");
    code.push_str("pub(crate) static KEY_NAMES: [&str; KEY_COUNT] = [\n");
    for key in keys.keys() {
        code.push_str(&format!("    {key:?},\n"));
    }
    code.push_str("];\n/// The English text of each key, in key order.\n");
    code.push_str("pub(crate) static ENGLISH: [English; KEY_COUNT] = [\n");
    for (english, _) in keys.values() {
        match english {
            English::Text(text) => code.push_str(&format!("    English::Text({text:?}),\n")),
            English::Plural(forms) => {
                code.push_str("    English::Plural([");
                for form in forms {
                    match form {
                        Some(text) => code.push_str(&format!("Some({text:?}), ")),
                        None => code.push_str("None, "),
                    }
                }
                code.push_str("]),\n");
            }
        }
    }
    code.push_str("];\n\n/// One constant per interface string (`lng_saved_messages` → `SAVED_MESSAGES`,\n/// `qll_*` → `Q_*`).\npub mod keys {\n    use super::{Key, PluralKey};\n");
    for (index, (key, (english, _))) in keys.iter().enumerate() {
        let constant = constant_name(key);
        if let Some(other) = names.insert(constant.clone(), key.clone()) {
            return Err(format!(
                "keys {other:?} and {key:?} both map to the constant {constant}"
            ));
        }
        let (ty, doc) = match english {
            English::Text(text) => ("Key", text.clone()),
            English::Plural(forms) => (
                "PluralKey",
                forms
                    .get(parse::Form::Other.index())
                    .cloned()
                    .flatten()
                    .unwrap_or_default(),
            ),
        };
        code.push_str(&format!(
            "    #[doc = {:?}]\n    pub const {constant}: {ty} = {ty}({index});\n",
            format!("`{key}`: {doc}")
        ));
    }
    code.push_str("}\n");
    code.push_str(&overlays(&manifest.join("lang"))?);
    let path = out.join("lang_keys.rs");
    std::fs::write(&path, code).map_err(|err| format!("{}: {err}", path.display()))
}

/// Community translations of Quill's strings: every `lang/<code>/*.strings`
/// except English, embedded as `(code, file text)`.
fn overlays(lang: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    let dirs = std::fs::read_dir(lang).map_err(|err| format!("{}: {err}", lang.display()))?;
    for dir in dirs.filter_map(Result::ok).map(|e| e.path()) {
        let Some(code) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        if code == "en" || !dir.is_dir() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", dir.display());
        let entries = std::fs::read_dir(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
        for path in entries.filter_map(Result::ok).map(|e| e.path()) {
            if path.extension().is_some_and(|ext| ext == "strings") {
                println!("cargo:rerun-if-changed={}", path.display());
                let text = std::fs::read_to_string(&path)
                    .map_err(|err| format!("{}: {err}", path.display()))?;
                parse::parse_strings(&text).map_err(|err| format!("{}: {err}", path.display()))?;
                files.push((code.to_ascii_lowercase(), path));
            }
        }
    }
    files.sort();
    let mut code = String::from(
        "/// Community translations of Quill's strings (`lang/<code>/*.strings`).\npub(crate) static OVERLAYS: &[(&str, &str)] = &[\n",
    );
    for (lang, path) in files {
        code.push_str(&format!(
            "    ({lang:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    code.push_str("];\n");
    Ok(code)
}

fn same(a: &English, b: &English) -> bool {
    match (a, b) {
        (English::Text(a), English::Text(b)) => a == b,
        (English::Plural(a), English::Plural(b)) => a == b,
        _ => false,
    }
}

fn valid_key(key: &str) -> bool {
    (key.starts_with("lng_") || key.starts_with("qll_"))
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `lng_saved_messages` → `SAVED_MESSAGES`, `qll_hdr_badge` → `Q_HDR_BADGE`.
fn constant_name(key: &str) -> String {
    let (prefix, rest) = match key.strip_prefix("lng_") {
        Some(rest) => ("", rest),
        None => ("Q_", key.strip_prefix("qll_").unwrap_or(key)),
    };
    let mut name = format!("{prefix}{}", rest.to_ascii_uppercase());
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, 'N');
    }
    name
}

#[cfg(windows)]
fn windows_icon(manifest: &Path, out: &Path) -> Result<(), String> {
    println!("cargo:rerun-if-changed=assets/icons/Quill.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let icon = manifest.join("assets/icons/Quill.ico");
    let rc = out.join("quill.rc");
    let icon = icon.to_string_lossy().replace('\\', "/");
    std::fs::write(&rc, format!("1 ICON \"{icon}\"\n"))
        .map_err(|err| format!("write rc: {err}"))?;
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_required()
        .map_err(|err| format!("embed application icon: {err:?}"))
}

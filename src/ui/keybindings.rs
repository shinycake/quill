use super::actions::{
    AttachFile, CancelSearch, ChatSearchNewer, ChatSearchOlder, CloseWindow, ComposerEditLink,
    ComposerPastePlain, DeleteSelection, FirstChat, FocusComposer, FocusSidebar, FormatBlockQuote,
    FormatBold, FormatClear, FormatItalic, FormatMonospace, FormatSpoiler, FormatStrikethrough,
    FormatUnderline, HistoryPageDown, HistoryPageUp, HistoryToBottom, HistoryToTop, LastChat,
    LoadOlder, LockApp, MarkChatRead, MinimizeWindow, NextChat, NextFolder, OpenArchive,
    OpenChatSearch, OpenContacts, OpenHelp, OpenPinnedChat, OpenSavedMessages, OpenSearch,
    OpenSettings, OpenShortcuts, PrevChat, PrevFolder, QuitApp, ReplyToNext, ReplyToPrevious,
    SelectionExtendNewer, SelectionExtendOlder, SelectionFocusNewer, SelectionFocusOlder,
    ShowChatMenu, ShowChatPreview, StoryTogglePause, ToggleFullscreen, ToggleMessageSelection,
    ToggleTheme, ViewerCopy, ViewerFlipHorizontal, ViewerFlipVertical, ViewerNext, ViewerPrev,
    ViewerSave, ViewerZoomIn, ViewerZoomOut, ViewerZoomReset, ZoomWindow,
};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::settings::CustomKeybinding;

/// The platform's primary modifier plus a key, as a keystroke literal:
/// `cmd-` on macOS, `ctrl-` elsewhere (GPUI's `secondary`). The composer
/// formatting shortcuts use only this, as Telegram Desktop does (Qt maps
/// `ctrl` to Cmd on macOS), so Linux and Windows get Ctrl and macOS keeps
/// Ctrl+B and friends for text-field editing.
macro_rules! primary {
    ($key:literal) => {
        if cfg!(target_os = "macos") {
            concat!("cmd-", $key)
        } else {
            concat!("ctrl-", $key)
        }
    };
}

/// Quote chord. On a US layout macOS GPUI reports Shift+. as the key ">"
/// with Shift already consumed (gpui-pre-macos events.rs
/// `parse_keystroke`), but the exact event depends on the keyboard layout
/// (non-ASCII layouts fall back to the Cmd layout), and a live test of the
/// `cmd->` form alone did nothing; so every platform binds both spellings
/// (`.` + Shift and `>`).
#[cfg(target_os = "macos")]
const QUOTE_CHORDS: &[&str] = &["cmd->", "cmd-shift-."];
#[cfg(not(target_os = "macos"))]
const QUOTE_CHORDS: &[&str] = &["ctrl-shift-.", "ctrl->"];

/// Key context of the composer's Textarea wrapper (see
/// [`composer_bindings`]).
pub(super) const COMPOSER_CONTEXT: &str = "QuillComposer";

/// Where the link chord applies: the Textarea (kit context `Input`) inside
/// the composer wrapper. It matches at the same depth as the unscoped
/// quick-switch binding, so it must be added after it (a later binding wins
/// a depth tie), which `apply_custom_bindings` and `shortcut_rows` do.
const COMPOSER_INPUT_CONTEXT: &str = "QuillComposer > Input";

/// Dismissing Appearance must also release ownership of composer keystrokes.
pub(super) fn close_appearance_capture(
    appearance_open: &mut bool,
    capture: &mut Option<String>,
    error: &mut Option<(String, String)>,
) {
    *appearance_open = false;
    *capture = None;
    *error = None;
}

/// Also discard stale capture state if a dialog was dismissed elsewhere.
pub(super) fn capture_active(
    appearance_open: bool,
    capture: &mut Option<String>,
    error: &mut Option<(String, String)>,
) -> bool {
    if !appearance_open {
        *capture = None;
        *error = None;
    }
    appearance_open && capture.is_some()
}

/// A new prefs root needs its own keymap, even when it has no overrides.
pub(super) fn invalidate_account_keybindings(
    applied: &mut bool,
    capture: &mut Option<String>,
    error: &mut Option<(String, String)>,
) {
    *applied = false;
    *capture = None;
    *error = None;
}

/// Parity slice (platform-custom-keybindings): a user-rebindable action.
pub struct RebindableAction {
    /// Stable id used in persisted prefs (e.g. "focus-composer").
    pub id: &'static str,
    /// Display label in the shortcuts settings section.
    pub label: &'static str,
    /// Default keystrokes (all platforms).
    pub defaults: &'static [&'static str],
}

/// Parity slice (platform-custom-keybindings): the actions users may rebind.
/// Window-chrome and app-lifecycle bindings (quit, close, minimize, …) stay
/// fixed — rebinding those risks stranding the user.
pub const REBINDABLE_ACTIONS: &[RebindableAction] = &[
    RebindableAction {
        id: "focus-sidebar",
        label: "Focus chat list",
        // Cmd/Ctrl+1..8 open the pinned chats (tdesktop), so focusing the
        // list moved to the Alt variant.
        defaults: &["cmd-alt-1", "ctrl-alt-1"],
    },
    RebindableAction {
        id: "focus-composer",
        label: "Focus composer",
        defaults: &["cmd-l", "ctrl-l"],
    },
    RebindableAction {
        id: "next-chat",
        label: "Next chat",
        defaults: &["alt-down", "ctrl-tab", primary!("pagedown")],
    },
    RebindableAction {
        id: "prev-chat",
        label: "Previous chat",
        defaults: &["alt-up", "ctrl-shift-tab", primary!("pageup")],
    },
    RebindableAction {
        id: "load-older",
        label: "Load older messages",
        // Cmd/Ctrl+Up replies to the previous message (tdesktop).
        defaults: &["alt-pageup"],
    },
    RebindableAction {
        id: "open-search",
        label: "Search",
        defaults: &["cmd-k", "ctrl-k"],
    },
    RebindableAction {
        id: "open-chat-search",
        label: "Search in chat",
        defaults: &["cmd-f", "ctrl-f"],
    },
    RebindableAction {
        id: "chat-search-newer",
        label: "Next search result",
        defaults: &["cmd-g", "ctrl-g"],
    },
    RebindableAction {
        id: "chat-search-older",
        label: "Previous search result",
        defaults: &["cmd-shift-g", "ctrl-shift-g"],
    },
    RebindableAction {
        id: "cancel-search",
        label: "Cancel search",
        defaults: &["escape"],
    },
    RebindableAction {
        id: "format-bold",
        label: "Bold",
        defaults: &[primary!("b")],
    },
    RebindableAction {
        id: "format-italic",
        label: "Italic",
        defaults: &[primary!("i")],
    },
    RebindableAction {
        id: "format-underline",
        label: "Underline",
        defaults: &[primary!("u")],
    },
    RebindableAction {
        id: "format-strikethrough",
        label: "Strikethrough",
        defaults: &[primary!("shift-x")],
    },
    RebindableAction {
        id: "format-monospace",
        label: "Monospace",
        defaults: &[primary!("shift-m")],
    },
    RebindableAction {
        id: "format-blockquote",
        label: "Quote",
        defaults: QUOTE_CHORDS,
    },
    RebindableAction {
        id: "format-spoiler",
        label: "Spoiler",
        defaults: &[primary!("shift-p")],
    },
    RebindableAction {
        id: "format-clear",
        label: "Clear formatting",
        defaults: &[primary!("shift-n")],
    },
    RebindableAction {
        id: "viewer-prev",
        label: "Previous media",
        defaults: &["left"],
    },
    RebindableAction {
        id: "viewer-next",
        label: "Next media",
        defaults: &["right"],
    },
    RebindableAction {
        id: "viewer-zoom-reset",
        label: "Reset zoom",
        defaults: &["0"],
    },
    RebindableAction {
        id: "viewer-zoom-in",
        label: "Zoom in",
        defaults: &["=", primary!("=")],
    },
    RebindableAction {
        id: "viewer-zoom-out",
        label: "Zoom out",
        defaults: &["-", primary!("-")],
    },
    RebindableAction {
        id: "viewer-flip-h",
        label: "Flip photo horizontally",
        defaults: &["h"],
    },
    RebindableAction {
        id: "viewer-flip-v",
        label: "Flip photo vertically",
        defaults: &["v"],
    },
    RebindableAction {
        id: "viewer-copy",
        label: "Copy photo",
        defaults: &[primary!("c")],
    },
    RebindableAction {
        id: "story-pause",
        label: "Pause or resume story",
        defaults: &["space"],
    },
    RebindableAction {
        id: "viewer-save",
        label: "Save media",
        defaults: &[primary!("s")],
    },
];

/// Parity slice (platform-custom-keybindings): build the key binding for a
/// rebindable action id and keystroke string. Returns `None` for unknown ids
/// or unparsable keystrokes (a bad saved pref never breaks startup).
pub fn keybinding_for(id: &str, keystroke: &str) -> Option<KeyBinding> {
    // `KeyBinding::new` unwraps the keystroke parse — validate first so a bad
    // saved pref can never panic startup. An empty string parses as a blank
    // key, which is not a usable chord.
    let Ok(parsed) = Keystroke::parse(keystroke) else {
        return None;
    };
    if parsed.key.is_empty() {
        return None;
    }
    match id {
        "focus-sidebar" => Some(KeyBinding::new(keystroke, FocusSidebar, None)),
        "focus-composer" => Some(KeyBinding::new(keystroke, FocusComposer, None)),
        "next-chat" => Some(KeyBinding::new(keystroke, NextChat, None)),
        "prev-chat" => Some(KeyBinding::new(keystroke, PrevChat, None)),
        "load-older" => Some(KeyBinding::new(keystroke, LoadOlder, None)),
        "open-search" => Some(KeyBinding::new(keystroke, OpenSearch, None)),
        "open-chat-search" => Some(KeyBinding::new(keystroke, OpenChatSearch, None)),
        "chat-search-newer" => Some(KeyBinding::new(keystroke, ChatSearchNewer, None)),
        "chat-search-older" => Some(KeyBinding::new(keystroke, ChatSearchOlder, None)),
        "cancel-search" => Some(KeyBinding::new(keystroke, CancelSearch, None)),
        "format-bold" => Some(KeyBinding::new(keystroke, FormatBold, None)),
        "format-italic" => Some(KeyBinding::new(keystroke, FormatItalic, None)),
        "format-underline" => Some(KeyBinding::new(keystroke, FormatUnderline, None)),
        "format-strikethrough" => Some(KeyBinding::new(keystroke, FormatStrikethrough, None)),
        "format-monospace" => Some(KeyBinding::new(keystroke, FormatMonospace, None)),
        "format-blockquote" => Some(KeyBinding::new(keystroke, FormatBlockQuote, None)),
        "format-spoiler" => Some(KeyBinding::new(keystroke, FormatSpoiler, None)),
        "format-clear" => Some(KeyBinding::new(keystroke, FormatClear, None)),
        "viewer-prev" => Some(KeyBinding::new(keystroke, ViewerPrev, None)),
        "viewer-next" => Some(KeyBinding::new(keystroke, ViewerNext, None)),
        "viewer-zoom-reset" => Some(KeyBinding::new(keystroke, ViewerZoomReset, None)),
        "viewer-zoom-in" => Some(KeyBinding::new(keystroke, ViewerZoomIn, None)),
        "viewer-zoom-out" => Some(KeyBinding::new(keystroke, ViewerZoomOut, None)),
        "viewer-flip-h" => Some(KeyBinding::new(keystroke, ViewerFlipHorizontal, None)),
        "viewer-flip-v" => Some(KeyBinding::new(keystroke, ViewerFlipVertical, None)),
        "viewer-copy" => Some(KeyBinding::new(keystroke, ViewerCopy, None)),
        "viewer-save" => Some(KeyBinding::new(keystroke, ViewerSave, None)),
        "story-pause" => Some(KeyBinding::new(keystroke, StoryTogglePause, None)),
        _ => None,
    }
}

/// The composer link chord: Cmd+K on macOS, Ctrl+K elsewhere
/// (`kEditLinkSequence`).
pub(super) fn link_chord() -> &'static str {
    primary!("k")
}

/// Plain paste chord: Cmd+Shift+V / Ctrl+Shift+V.
fn paste_plain_chord() -> &'static str {
    primary!("shift-v")
}

/// Composer-only bindings that are not user-rebindable: the link chord
/// (scoped to the composer's key context so it overrides quick switch only
/// there) and paste-as-plain-text.
fn composer_bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new(link_chord(), ComposerEditLink, Some(COMPOSER_INPUT_CONTEXT)),
        KeyBinding::new(paste_plain_chord(), ComposerPastePlain, None),
    ];
    bindings.extend(composer_scoped_bindings());
    bindings
}

/// Fixed bindings: window chrome and app lifecycle — not rebindable.
fn fixed_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-q", QuitApp, None),
        KeyBinding::new("ctrl-q", QuitApp, None),
        // kit Phase 7: window-chrome shortcuts (HIG: Cmd+W close, Cmd+M
        // minimize; F11 / Cmd+Ctrl+F fullscreen).
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("ctrl-w", CloseWindow, None),
        // Local passcode. tdesktop uses Ctrl/Cmd+L, which Quill already
        // gives to "Focus composer", so lock takes the Shift variant.
        KeyBinding::new("cmd-shift-l", LockApp, None),
        KeyBinding::new("ctrl-shift-l", LockApp, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
        KeyBinding::new("ctrl-m", MinimizeWindow, None),
        KeyBinding::new("f11", ToggleFullscreen, None),
        KeyBinding::new("cmd-ctrl-f", ToggleFullscreen, None),
    ]
}

fn default_bindings() -> Vec<KeyBinding> {
    let mut bindings: Vec<KeyBinding> =
        shortcut_rows().into_iter().map(|row| row.binding).collect();
    bindings.extend(composer_scoped_bindings());
    bindings
}

/// The shortcut-pack bindings that are not user-rebindable. They are
/// installed with the fixed chrome and claim their chords first, so no
/// rebindable action can land on one.
fn pack_bindings() -> Vec<KeyBinding> {
    chat_nav_rows()
        .into_iter()
        .chain(pinned_chat_rows())
        .chain(message_rows())
        .map(|row| row.binding)
        .collect()
}

/// The keystrokes reserved by the fixed (window-chrome/app-lifecycle)
/// bindings — a custom binding may never take one of these.
fn fixed_keystrokes() -> Vec<Keystroke> {
    fixed_bindings()
        .iter()
        .chain(pack_bindings().iter())
        .flat_map(|b| b.keystrokes().iter().map(|k| k.inner().clone()))
        .collect()
}

/// Why a chord cannot become the live shortcut for an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeybindingConflict {
    /// The string does not parse as a keystroke.
    Invalid,
    /// Reserved by quit, close, minimize, or fullscreen.
    FixedChrome,
    /// Already the live chord of another rebindable action.
    Rebindable { other_label: &'static str },
}

/// One rebindable action after saved overrides are filtered.
///
/// `live` is what the keymap will actually install. `rejected` is a saved
/// override that was not installed — the UI must not present that chord as
/// the active shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeybinding {
    pub id: &'static str,
    pub label: &'static str,
    pub live: Vec<String>,
    pub rejected: Option<KeybindingConflict>,
}

/// User-facing explanation. The chord named here is the one that was
/// refused, not the chord that remains active.
pub fn conflict_message(chord: &str, conflict: &KeybindingConflict) -> String {
    match conflict {
        KeybindingConflict::Invalid => {
            format!("{chord} isn't a valid shortcut and was not applied.")
        }
        KeybindingConflict::FixedChrome => {
            format!("{chord} is reserved for a built-in shortcut and was not applied.")
        }
        KeybindingConflict::Rebindable { other_label } => {
            format!("{chord} is already used by {other_label} and was not applied.")
        }
    }
}

/// Whether `chord` may be saved as the shortcut for `action_id`.
///
/// Compares against fixed chrome and against the chords
/// [`resolve_keybindings`] would actually install for every other action.
/// The action's own current chord is not a conflict — rebinding replaces it.
pub fn keybinding_conflict(
    action_id: &str,
    chord: &str,
    customs: &[CustomKeybinding],
) -> Option<KeybindingConflict> {
    let Some(parsed) = parse_chord(chord) else {
        return Some(KeybindingConflict::Invalid);
    };
    if chord_taken(&fixed_keystrokes(), &parsed) {
        return Some(KeybindingConflict::FixedChrome);
    }
    for row in resolve_keybindings(customs) {
        if row.id == action_id {
            continue;
        }
        if row
            .live
            .iter()
            .any(|live| parse_chord(live).is_some_and(|other| same_chord(&other, &parsed)))
        {
            return Some(KeybindingConflict::Rebindable {
                other_label: row.label,
            });
        }
    }
    None
}

/// Resolve saved overrides into the chords that should be bound.
///
/// Fixed chrome is claimed first. Each rebindable action then takes its
/// saved chord only when that chord is still free; otherwise the save is
/// rejected and the action keeps whichever of its defaults are still free.
/// A chord is never installed twice, so two actions cannot both go live on
/// the same keystroke.
pub fn resolve_keybindings(customs: &[CustomKeybinding]) -> Vec<ResolvedKeybinding> {
    let fixed = fixed_keystrokes();
    let mut claimed = fixed.clone();
    let mut resolved = Vec::with_capacity(REBINDABLE_ACTIONS.len());
    for ra in REBINDABLE_ACTIONS {
        let saved = customs
            .iter()
            .find(|custom| custom.id == ra.id && !custom.keystroke.is_empty());
        if let Some(saved) = saved {
            if let Some(ks) = parse_chord(&saved.keystroke) {
                if !chord_taken(&claimed, &ks) {
                    let label = canonical_chord(&ks);
                    claimed.push(ks);
                    resolved.push(ResolvedKeybinding {
                        id: ra.id,
                        label: ra.label,
                        live: vec![label],
                        rejected: None,
                    });
                    continue;
                }
                let rejected = if chord_taken(&fixed, &ks) {
                    KeybindingConflict::FixedChrome
                } else {
                    KeybindingConflict::Rebindable {
                        other_label: owner_label(&resolved, &ks).unwrap_or("another shortcut"),
                    }
                };
                resolved.push(ResolvedKeybinding {
                    id: ra.id,
                    label: ra.label,
                    live: claim_defaults(ra, &mut claimed),
                    rejected: Some(rejected),
                });
                continue;
            }
            resolved.push(ResolvedKeybinding {
                id: ra.id,
                label: ra.label,
                live: claim_defaults(ra, &mut claimed),
                rejected: Some(KeybindingConflict::Invalid),
            });
            continue;
        }
        resolved.push(ResolvedKeybinding {
            id: ra.id,
            label: ra.label,
            live: claim_defaults(ra, &mut claimed),
            rejected: None,
        });
    }
    resolved
}

/// Modifier keys are the start of a chord, not a finished shortcut.
pub fn is_modifier_key(key: &str) -> bool {
    matches!(
        key,
        "control"
            | "ctrl"
            | "alt"
            | "option"
            | "shift"
            | "platform"
            | "cmd"
            | "super"
            | "win"
            | "meta"
            | "function"
            | "fn"
    )
}

/// Parseable chord for a captured key event (`"ctrl-k"`, `"cmd-shift-g"`).
///
/// `Keystroke`'s `Display` is the on-screen form (`ctrl-K`, or symbols on
/// macOS) and does not round-trip through `Keystroke::parse`. Returns
/// `None` for bare modifiers so holding Ctrl does not end capture.
pub fn canonical_event_chord(keystroke: &Keystroke) -> Option<String> {
    if keystroke.key.is_empty() || is_modifier_key(&keystroke.key) {
        return None;
    }
    let normalized = Keystroke {
        modifiers: keystroke.modifiers,
        key: keystroke.key.to_ascii_lowercase(),
        key_char: None,
    };
    let chord = canonical_chord(&normalized);
    parse_chord(&chord).map(|_| chord)
}

fn canonical_chord(ks: &Keystroke) -> String {
    let mut result = String::new();
    if ks.modifiers.function {
        result.push_str("fn-");
    }
    if ks.modifiers.control {
        result.push_str("ctrl-");
    }
    if ks.modifiers.alt {
        result.push_str("alt-");
    }
    if ks.modifiers.platform {
        result.push_str("cmd-");
    }
    if ks.modifiers.shift {
        result.push_str("shift-");
    }
    result.push_str(&ks.key);
    result
}

fn parse_chord(source: &str) -> Option<Keystroke> {
    let ks = Keystroke::parse(source).ok()?;
    if ks.key.is_empty() {
        return None;
    }
    Some(Keystroke {
        modifiers: ks.modifiers,
        key: ks.key,
        key_char: None,
    })
}

fn same_chord(a: &Keystroke, b: &Keystroke) -> bool {
    a.modifiers == b.modifiers && a.key.eq_ignore_ascii_case(&b.key)
}

fn chord_taken(claimed: &[Keystroke], ks: &Keystroke) -> bool {
    claimed.iter().any(|claimed| same_chord(claimed, ks))
}

fn owner_label(resolved: &[ResolvedKeybinding], ks: &Keystroke) -> Option<&'static str> {
    resolved.iter().find_map(|row| {
        row.live.iter().find_map(|live| {
            parse_chord(live)
                .filter(|other| same_chord(other, ks))
                .map(|_| row.label)
        })
    })
}

fn claim_defaults(ra: &RebindableAction, claimed: &mut Vec<Keystroke>) -> Vec<String> {
    let mut live = Vec::new();
    for default in ra.defaults {
        let Some(ks) = parse_chord(default) else {
            continue;
        };
        if chord_taken(claimed, &ks) {
            continue;
        }
        claimed.push(ks);
        live.push((*default).to_string());
    }
    live
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(default_bindings());
    trace_keys(cx);
}

/// `QUILL_TRACE_KEYS=1`: log every keystroke the window receives (what GPUI
/// made of the OS event) and the action it resolved to, to stderr. For
/// finding the real spelling of a chord on a given keyboard layout.
fn trace_keys(cx: &mut App) {
    if std::env::var_os("QUILL_TRACE_KEYS").is_none() {
        return;
    }
    cx.observe_keystrokes(|event, _, _| {
        let ks = &event.keystroke;
        eprintln!(
            "key: {} key={:?} key_char={:?} mods={:?} action={}",
            ks.unparse(),
            ks.key,
            ks.key_char,
            ks.modifiers,
            event.action.as_ref().map_or("none", |action| action.name())
        );
    })
    .detach();
}

/// Parity slice (platform-custom-keybindings): rebuild the keymap from
/// defaults with the user's overrides applied. An override replaces all
/// default keystrokes for its action. Overrides that are unparsable, that
/// collide with a fixed (window-chrome/app-lifecycle) keystroke, or that
/// collide with another rebindable action's live chord are not installed —
/// that action keeps the defaults that are still free. A chord is bound to
/// at most one action.
pub fn apply_custom_bindings(cx: &mut App, customs: &[CustomKeybinding]) {
    // Review fix (#242 defect 1): `clear_key_bindings` wipes the whole app
    // keymap, including kit-internal bindings (List / command-palette
    // navigation) registered by `gpui_kit::init`. Keep everything that isn't
    // Quill-managed — Quill's own set is rebuilt below. Every Quill action
    // lives in the `quill_ui` actions! namespace (src/ui/actions.rs).
    let mut bindings: Vec<KeyBinding> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|kb| !kb.action().name().starts_with("quill_ui::"))
        .cloned()
        .collect();

    // Fixed bindings are installed and claimed first. GPUI resolves
    // same-keystroke ties later-added-wins, so a custom chord on
    // cmd-q/cmd-w/cmd-m/f11 — or two rebindable actions on one chord —
    // must never be added. `resolve_keybindings` drops those.
    bindings.extend(fixed_bindings());
    bindings.extend(pack_bindings());
    for row in resolve_keybindings(customs) {
        for chord in row.live {
            if let Some(kb) = keybinding_for(row.id, &chord) {
                bindings.push(kb);
            }
        }
    }
    // After the rebindable rows: the composer link chord ties with quick
    // switch on depth and the later binding wins.
    bindings.extend(composer_bindings());
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
}

/// One row of the app's keyboard-shortcut reference: the single source of
/// truth for both `bind_keys` (what the app listens for) and the shortcuts
/// reference dialog (what the user sees). `keystroke` is gpui format
/// (e.g. `"cmd-q"`); the dialog renders it through kit's `Kbd`.
pub struct ShortcutRow {
    pub label: &'static str,
    pub section: &'static str,
    pub binding: KeyBinding,
}

fn row<A: Action>(
    keystroke: &'static str,
    label: &'static str,
    section: &'static str,
    action: A,
) -> ShortcutRow {
    ShortcutRow {
        label,
        section,
        binding: KeyBinding::new(keystroke, action, None),
    }
}

/// Every keybinding the app registers, in reference-dialog order. Rows that
/// share a label (e.g. `cmd-q` / `ctrl-q`) render as one entry with both
/// chips.
pub fn shortcut_rows() -> Vec<ShortcutRow> {
    let mut rows = vec![
        // General.
        row("cmd-q", "Quit Quill", "General", QuitApp),
        row("ctrl-q", "Quit Quill", "General", QuitApp),
        // kit Phase 7: window-chrome shortcuts (HIG: Cmd+W close, Cmd+M
        // minimize; F11 / Cmd+Ctrl+F fullscreen).
        row("cmd-shift-l", "Lock Quill", "General", LockApp),
        row("ctrl-shift-l", "Lock Quill", "General", LockApp),
        row("cmd-w", "Close window", "General", CloseWindow),
        row("ctrl-w", "Close window", "General", CloseWindow),
        row("cmd-m", "Minimize window", "General", MinimizeWindow),
        row("ctrl-m", "Minimize window", "General", MinimizeWindow),
        row("f11", "Toggle fullscreen", "General", ToggleFullscreen),
        row(
            "cmd-ctrl-f",
            "Toggle fullscreen",
            "General",
            ToggleFullscreen,
        ),
        // Navigation.
        row("cmd-alt-1", "Focus chat list", "Navigation", FocusSidebar),
        row("ctrl-alt-1", "Focus chat list", "Navigation", FocusSidebar),
        row(
            "cmd-l",
            "Focus message composer",
            "Navigation",
            FocusComposer,
        ),
        row(
            "ctrl-l",
            "Focus message composer",
            "Navigation",
            FocusComposer,
        ),
        row("alt-down", "Next chat", "Navigation", NextChat),
        row("ctrl-tab", "Next chat", "Navigation", NextChat),
        row(primary!("pagedown"), "Next chat", "Navigation", NextChat),
        row("alt-up", "Previous chat", "Navigation", PrevChat),
        row("ctrl-shift-tab", "Previous chat", "Navigation", PrevChat),
        row(primary!("pageup"), "Previous chat", "Navigation", PrevChat),
        row("alt-pageup", "Load older messages", "Navigation", LoadOlder),
        // Search.
        row("cmd-k", "Quick switch chats", "Search", OpenSearch),
        row("ctrl-k", "Quick switch chats", "Search", OpenSearch),
        row("cmd-f", "Find in chat", "Search", OpenChatSearch),
        row("ctrl-f", "Find in chat", "Search", OpenChatSearch),
        row("cmd-g", "Next search result", "Search", ChatSearchNewer),
        row("ctrl-g", "Next search result", "Search", ChatSearchNewer),
        row(
            "cmd-shift-g",
            "Previous search result",
            "Search",
            ChatSearchOlder,
        ),
        row(
            "ctrl-shift-g",
            "Previous search result",
            "Search",
            ChatSearchOlder,
        ),
        row("escape", "Close search / cancel", "Search", CancelSearch),
        // Parity slice 5: the handlers no-op (and let the keystroke reach
        // text inputs) unless the media viewer is open.
        row(
            "space",
            "Pause or resume story",
            "Media viewer",
            StoryTogglePause,
        ),
        row("left", "Previous item", "Media viewer", ViewerPrev),
        row("right", "Next item", "Media viewer", ViewerNext),
        row("0", "Reset zoom", "Media viewer", ViewerZoomReset),
        row("=", "Zoom in", "Media viewer", ViewerZoomIn),
        row(primary!("="), "Zoom in", "Media viewer", ViewerZoomIn),
        row("-", "Zoom out", "Media viewer", ViewerZoomOut),
        row(primary!("-"), "Zoom out", "Media viewer", ViewerZoomOut),
        row(
            "h",
            "Flip horizontally",
            "Media viewer",
            ViewerFlipHorizontal,
        ),
        row("v", "Flip vertically", "Media viewer", ViewerFlipVertical),
        row(primary!("c"), "Copy photo", "Media viewer", ViewerCopy),
        row(primary!("s"), "Save", "Media viewer", ViewerSave),
        // M1: composer formatting shortcuts; the handlers no-op unless
        // the composer textarea has focus.
        row(primary!("b"), "Bold", "Composer", FormatBold),
        row(primary!("i"), "Italic", "Composer", FormatItalic),
        row(primary!("u"), "Underline", "Composer", FormatUnderline),
        row(
            primary!("shift-x"),
            "Strikethrough",
            "Composer",
            FormatStrikethrough,
        ),
        row(
            primary!("shift-m"),
            "Monospace",
            "Composer",
            FormatMonospace,
        ),
        // Quote rows are appended below (one per chord spelling).
        row(primary!("shift-p"), "Spoiler", "Composer", FormatSpoiler),
        row(
            primary!("shift-n"),
            "Clear formatting",
            "Composer",
            FormatClear,
        ),
        // Scoped to the composer's key context; with nothing selected it
        // falls back to quick switch (see `composer_bindings`).
        ShortcutRow {
            label: "Link (selected text)",
            section: "Composer",
            binding: KeyBinding::new(link_chord(), ComposerEditLink, Some(COMPOSER_INPUT_CONTEXT)),
        },
        row(
            paste_plain_chord(),
            "Paste as plain text",
            "Composer",
            ComposerPastePlain,
        ),
    ];
    rows.extend(
        QUOTE_CHORDS
            .iter()
            .map(|chord| row(chord, "Quote", "Composer", FormatBlockQuote)),
    );
    // The reference dialog prints a heading whenever the section changes,
    // so each group goes in next to its own section.
    let at = rows
        .iter()
        .position(|r| r.section == "Search")
        .unwrap_or(rows.len());
    rows.splice(
        at..at,
        chat_nav_rows().into_iter().chain(pinned_chat_rows()),
    );
    let at = rows
        .iter()
        .position(|r| r.section == "Media viewer")
        .unwrap_or(rows.len());
    rows.splice(at..at, message_rows());
    rows
}

/// Chat-list navigation chords from tdesktop's `fillDefaults`: first/last
/// chat, folders, Saved Messages, Archive, Contacts, read, menu, preview.
/// Not user-rebindable (they are claimed like window chrome).
fn chat_nav_rows() -> Vec<ShortcutRow> {
    vec![
        row(primary!("alt-home"), "First chat", "Navigation", FirstChat),
        row(primary!("alt-end"), "Last chat", "Navigation", LastChat),
        row("ctrl-shift-up", "Previous folder", "Navigation", PrevFolder),
        row("ctrl-shift-down", "Next folder", "Navigation", NextFolder),
        row(
            primary!("0"),
            "Open Saved Messages",
            "Navigation",
            OpenSavedMessages,
        ),
        row(primary!("9"), "Open Archive", "Navigation", OpenArchive),
        row(primary!("j"), "Open Contacts", "Navigation", OpenContacts),
        row(
            primary!("r"),
            "Mark chat as read",
            "Navigation",
            MarkChatRead,
        ),
        row(primary!("\\"), "Chat menu", "Navigation", ShowChatMenu),
        row(primary!("]"), "Chat preview", "Navigation", ShowChatPreview),
    ]
}

/// Cmd/Ctrl+1..8: the Nth pinned chat of the list on screen
/// (`Command::ChatPinned1..8`).
fn pinned_chat_rows() -> Vec<ShortcutRow> {
    let chords = [
        primary!("1"),
        primary!("2"),
        primary!("3"),
        primary!("4"),
        primary!("5"),
        primary!("6"),
        primary!("7"),
        primary!("8"),
    ];
    let labels = [
        "Open pinned chat 1",
        "Open pinned chat 2",
        "Open pinned chat 3",
        "Open pinned chat 4",
        "Open pinned chat 5",
        "Open pinned chat 6",
        "Open pinned chat 7",
        "Open pinned chat 8",
    ];
    chords
        .into_iter()
        .zip(labels)
        .enumerate()
        .map(|(index, (chord, label))| row(chord, label, "Navigation", OpenPinnedChat { index }))
        .collect()
}

/// The message-history shortcuts: reply navigation, attach, scrolling and
/// deleting a selection (tdesktop `HistoryWidget::keyPressEvent`,
/// `HistoryInner::keyPressEvent`).
fn message_rows() -> Vec<ShortcutRow> {
    vec![
        row(
            primary!("up"),
            "Reply to previous message",
            "Messages",
            ReplyToPrevious,
        ),
        row(
            primary!("down"),
            "Reply to next message",
            "Messages",
            ReplyToNext,
        ),
        row(primary!("o"), "Attach file", "Messages", AttachFile),
        row("pageup", "Scroll up a page", "Messages", HistoryPageUp),
        row(
            "pagedown",
            "Scroll down a page",
            "Messages",
            HistoryPageDown,
        ),
        row(
            "home",
            "Scroll to the first message",
            "Messages",
            HistoryToTop,
        ),
        row(
            "end",
            "Scroll to the latest message",
            "Messages",
            HistoryToBottom,
        ),
        row(
            "delete",
            "Delete selected messages",
            "Messages",
            DeleteSelection,
        ),
        row(
            "backspace",
            "Delete selected messages",
            "Messages",
            DeleteSelection,
        ),
        row(
            primary!("space"),
            "Select the focused message",
            "Messages",
            ToggleMessageSelection,
        ),
        row(
            "up",
            "Focus the older message while selecting",
            "Messages",
            SelectionFocusOlder,
        ),
        row(
            "down",
            "Focus the newer message while selecting",
            "Messages",
            SelectionFocusNewer,
        ),
        row(
            "shift-up",
            "Extend the selection to the older message",
            "Messages",
            SelectionExtendOlder,
        ),
        row(
            "shift-down",
            "Extend the selection to the newer message",
            "Messages",
            SelectionExtendNewer,
        ),
    ]
}

/// Chords the composer's own `Input` context binds (page keys, the Cmd/Ctrl
/// up/down caret jumps on macOS, bracket indent). A binding without a
/// context loses to those, so these twins sit in the composer's context
/// and added after the kit's; each handler propagates when the composer
/// should keep the key, which lets the kit binding run next.
fn composer_scoped_bindings() -> Vec<KeyBinding> {
    let ctx = Some(COMPOSER_INPUT_CONTEXT);
    vec![
        KeyBinding::new(primary!("up"), ReplyToPrevious, ctx),
        KeyBinding::new(primary!("down"), ReplyToNext, ctx),
        KeyBinding::new("pageup", HistoryPageUp, ctx),
        KeyBinding::new("pagedown", HistoryPageDown, ctx),
        KeyBinding::new(primary!("]"), ShowChatPreview, ctx),
    ]
}

/// kit Phase 7: the application menus — File / Edit / View / Window / Help,
/// every item wired to a working action. `setup_app_menus` installs them
/// twice from this one definition: `cx.set_menus` drives the native menu bar
/// on macOS, and `GlobalState::set_app_menus` feeds kit's `AppMenuBar`,
/// rendered in-window on Linux/Windows.
fn app_menus() -> Vec<Menu> {
    // Local aliases: `Copy` would shadow the derive macro's `Copy` at
    // module scope.
    use gpui_kit::component::input::{
        Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction,
        SelectAll as SelectAllAction, Undo as UndoAction,
    };
    // HIG: on macOS Quit lives in the app menu, not File.
    #[cfg(target_os = "macos")]
    let file_items = vec![MenuItem::action("Close Window", CloseWindow)];
    #[cfg(not(target_os = "macos"))]
    let file_items = vec![
        MenuItem::action("Close Window", CloseWindow),
        MenuItem::separator(),
        MenuItem::action("Quit Quill", QuitApp),
    ];
    let mut menus = Vec::new();
    // HIG: on macOS Quit lives in the app menu, not File.
    #[cfg(target_os = "macos")]
    menus.push(Menu::new("Quill").items([
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::action("Lock Quill", LockApp),
        MenuItem::separator(),
        MenuItem::action("Quit Quill", QuitApp),
    ]));
    menus.extend([
        Menu::new("File").items(file_items),
        Menu::new("Edit").items([
            MenuItem::action("Undo", UndoAction),
            MenuItem::action("Redo", RedoAction),
            MenuItem::separator(),
            MenuItem::os_action("Cut", CutAction, OsAction::Cut),
            MenuItem::os_action("Copy", CopyAction, OsAction::Copy),
            MenuItem::os_action("Paste", PasteAction, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", SelectAllAction, OsAction::SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action("Quick Switch", OpenSearch),
            MenuItem::action("Find in Chat", OpenChatSearch),
            MenuItem::separator(),
            MenuItem::action("Enter Full Screen", ToggleFullscreen),
            // kit Phase 8: light/dark switch for the whole app.
            MenuItem::action("Toggle Theme", ToggleTheme),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", MinimizeWindow),
            MenuItem::action("Zoom", ZoomWindow),
        ]),
        Menu::new("Help").items([
            MenuItem::action("Keyboard Shortcuts", OpenShortcuts),
            MenuItem::action("Quill on GitHub", OpenHelp),
        ]),
    ]);
    menus
}

/// kit Phase 7: install the app menus — native on macOS, kit `AppMenuBar`
/// data on Linux/Windows. Call once after `gpui_kit::init` + `bind_keys`.
pub fn setup_app_menus(cx: &mut App) {
    let owned: Vec<OwnedMenu> = app_menus().into_iter().map(Menu::owned).collect();
    cx.set_menus(app_menus());
    GlobalState::global_mut(cx).set_app_menus(owned);
}

// Context menus own typing and chat shortcuts; native focus traversal and
// activation still work, as do macOS window commands.
pub(super) fn context_menu_captures_key(key: &Keystroke) -> bool {
    if key.modifiers.platform && matches!(key.key.as_str(), "q" | "w" | "m") {
        return false;
    }
    if !key.modifiers.control
        && !key.modifiers.platform
        && !key.modifiers.alt
        && matches!(key.key.as_str(), "tab" | "enter" | "space")
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    // NOTE: explicit imports, not `use super::*` — `gpui_kit::*` re-exports
    // gpui's `#[test]` proc macro, which would shadow the builtin test
    // attribute and fail macro expansion ("recursion limit reached").
    use super::{
        Action, KeybindingConflict, Keystroke, Modifiers, OpenPinnedChat, QuitApp,
        REBINDABLE_ACTIONS, canonical_event_chord, capture_active, close_appearance_capture,
        composer_bindings, composer_scoped_bindings, conflict_message, context_menu_captures_key,
        default_bindings, fixed_keystrokes, invalidate_account_keybindings, keybinding_conflict,
        keybinding_for, resolve_keybindings, same_chord,
    };
    use quill::settings::CustomKeybinding;

    fn custom(id: &str, keystroke: &str) -> CustomKeybinding {
        CustomKeybinding {
            id: id.to_string(),
            keystroke: keystroke.to_string(),
        }
    }

    #[test]
    fn reference_table_matches_resolved_defaults() {
        let defaults = default_bindings();
        assert_eq!(defaults.len(), 90);
        for row in resolve_keybindings(&[]) {
            for chord in row.live {
                let binding = keybinding_for(row.id, &chord).unwrap();
                assert!(defaults.iter().any(|default| default.action().name()
                    == binding.action().name()
                    && same_chord(
                        default.keystrokes()[0].inner(),
                        binding.keystrokes()[0].inner()
                    )));
            }
        }
    }

    #[test]
    fn appearance_dismiss_clears_capture_and_error() {
        let mut open = true;
        let mut capture = Some("focus-composer".to_string());
        let mut error = Some(("focus-composer".to_string(), "Conflict".to_string()));
        close_appearance_capture(&mut open, &mut capture, &mut error);
        assert!(!open);
        assert_eq!(capture, None);
        assert_eq!(error, None);
        assert!(!capture_active(open, &mut capture, &mut error));
    }

    #[test]
    fn capture_only_consumes_keys_while_appearance_is_open_and_armed() {
        let mut capture = None;
        let mut error = None;
        assert!(!capture_active(true, &mut capture, &mut error));
        capture = Some("focus-composer".to_string());
        assert!(capture_active(true, &mut capture, &mut error));
        error = Some(("focus-composer".to_string(), "Conflict".to_string()));
        assert!(!capture_active(false, &mut capture, &mut error));
        assert_eq!(capture, None);
        assert_eq!(error, None);
    }

    #[test]
    fn account_change_invalidates_applied_bindings_and_capture() {
        let mut applied = true;
        let mut capture = Some("focus-composer".to_string());
        let mut error = Some(("focus-composer".to_string(), "Conflict".to_string()));
        invalidate_account_keybindings(&mut applied, &mut capture, &mut error);
        assert!(!applied);
        assert_eq!(capture, None);
        assert_eq!(error, None);
    }

    #[test]
    fn account_overrides_and_empty_prefs_resolve_independently() {
        let account_a = vec![custom("focus-composer", "ctrl-alt-9")];
        let account_b = vec![];
        let account_c = vec![custom("focus-composer", "ctrl-alt-8")];
        // Each apply rebuilds from this resolver, never from the prior keymap.
        for prefs in [&account_a, &account_b, &account_c, &account_b, &account_a] {
            let resolved = resolve_keybindings(prefs);
            if prefs.is_empty() {
                for (row, action) in resolved.iter().zip(REBINDABLE_ACTIONS) {
                    assert_eq!(row.id, action.id);
                    assert_eq!(row.live, action.defaults);
                    assert_eq!(row.rejected, None);
                }
                assert!(!resolved.iter().any(|row| {
                    row.live
                        .iter()
                        .any(|chord| chord == "ctrl-alt-9" || chord == "ctrl-alt-8")
                }));
            } else {
                let focus = resolved
                    .iter()
                    .find(|row| row.id == "focus-composer")
                    .unwrap();
                assert_eq!(focus.live, vec![prefs[0].keystroke.clone()]);
                assert_eq!(focus.rejected, None);
            }
        }
    }

    #[test]
    fn keybinding_for_valid_id_and_keystroke() {
        assert!(keybinding_for("focus-composer", "ctrl-l").is_some());
        assert!(keybinding_for("cancel-search", "escape").is_some());
    }

    #[test]
    fn keybinding_for_unknown_id_is_none() {
        assert!(keybinding_for("nope", "ctrl-l").is_none());
    }

    #[test]
    fn keybinding_for_invalid_keystroke_is_none() {
        // Must not panic (KeyBinding::new unwraps the parse).
        assert!(keybinding_for("focus-composer", "not-a-keystroke-%%%").is_none());
        assert!(keybinding_for("focus-composer", "").is_none());
    }

    #[test]
    fn rebindable_ids_are_unique() {
        let mut ids: Vec<&str> = REBINDABLE_ACTIONS.iter().map(|ra| ra.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), REBINDABLE_ACTIONS.len());
    }

    #[test]
    fn quill_action_names_match_preserve_filter() {
        // apply_custom_bindings keeps bindings whose action name does NOT
        // start with this prefix; if the actions! namespace ever changes,
        // the filter must change with it.
        assert!(QuitApp.name().starts_with("quill_ui::"));
    }

    #[test]
    fn custom_keystroke_on_fixed_keystroke_is_rejected() {
        // Mirrors apply_custom_bindings' collision rule: a custom "cmd-q"
        // for Search must not shadow the fixed Quit binding.
        let fixed = fixed_keystrokes();
        let kb = keybinding_for("open-search", "cmd-q").unwrap();
        assert!(kb.keystrokes().iter().any(|k| fixed.contains(k.inner())));
        let ok = keybinding_for("open-search", "ctrl-k").unwrap();
        assert!(!ok.keystrokes().iter().any(|k| fixed.contains(k.inner())));
        for reserved in [
            "cmd-q",
            "ctrl-q",
            "cmd-w",
            "ctrl-w",
            "cmd-m",
            "ctrl-m",
            "f11",
            "cmd-ctrl-f",
        ] {
            assert_eq!(
                keybinding_conflict("open-search", reserved, &[]),
                Some(KeybindingConflict::FixedChrome),
                "{reserved}"
            );
        }
    }

    #[test]
    fn rebindable_conflict_blocks_a_shared_live_chord() {
        // ctrl-l is still Focus composer's live default.
        assert_eq!(
            keybinding_conflict("open-search", "ctrl-l", &[]),
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        // Moving Focus composer off ctrl-l frees it. The new chord is taken.
        let moved = vec![custom("focus-composer", "ctrl-alt-9")];
        assert_eq!(keybinding_conflict("open-search", "ctrl-l", &moved), None);
        assert_eq!(
            keybinding_conflict("open-search", "ctrl-alt-9", &moved),
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        // An action may keep its own default.
        assert_eq!(keybinding_conflict("focus-composer", "ctrl-l", &[]), None);
        assert_eq!(
            keybinding_conflict("open-search", "not-a-keystroke-%%%", &[]),
            Some(KeybindingConflict::Invalid)
        );
    }

    #[test]
    fn rejected_chords_are_not_reported_as_live() {
        let customs = vec![
            custom("focus-composer", "ctrl-k"),
            custom("open-search", "cmd-q"),
            custom("format-bold", "ctrl-k"),
        ];
        let resolved = resolve_keybindings(&customs);
        let focus = resolved
            .iter()
            .find(|row| row.id == "focus-composer")
            .unwrap();
        assert_eq!(focus.live, vec!["ctrl-k".to_string()]);
        assert_eq!(focus.rejected, None);

        let search = resolved.iter().find(|row| row.id == "open-search").unwrap();
        assert_eq!(search.rejected, Some(KeybindingConflict::FixedChrome));
        assert_eq!(search.live, vec!["cmd-k".to_string()]);
        assert!(!search.live.iter().any(|chord| chord == "cmd-q"));

        let bold = resolved.iter().find(|row| row.id == "format-bold").unwrap();
        assert_eq!(
            bold.rejected,
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        assert_eq!(bold.live, vec![primary("b")]);
        assert!(!bold.live.iter().any(|chord| chord == "ctrl-k"));

        let message = conflict_message("cmd-q", &KeybindingConflict::FixedChrome);
        assert!(message.contains("cmd-q"));
        assert!(message.contains("not applied"));
    }

    #[test]
    fn resolved_live_chords_do_not_overlap() {
        let customs = vec![
            custom("open-search", "cmd-q"),
            custom("focus-composer", "ctrl-k"),
            custom("format-bold", "ctrl-k"),
            custom("format-italic", &primary("b")),
        ];
        let resolved = resolve_keybindings(&customs);
        let mut seen: Vec<Keystroke> = fixed_keystrokes();
        for row in &resolved {
            for chord in &row.live {
                let ks = Keystroke::parse(chord).unwrap();
                assert!(
                    !seen.iter().any(|claimed| {
                        claimed.modifiers == ks.modifiers
                            && claimed.key.eq_ignore_ascii_case(&ks.key)
                    }),
                    "{} reuses {chord}",
                    row.id
                );
                seen.push(ks);
            }
        }
        let italic = resolved
            .iter()
            .find(|row| row.id == "format-italic")
            .unwrap();
        assert!(matches!(
            italic.rejected,
            Some(KeybindingConflict::Rebindable { .. })
        ));
        assert_eq!(italic.live, vec![primary("i")]);
    }

    #[test]
    fn captured_event_chord_round_trips_and_matches_fixed_chrome() {
        let ks = Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Modifiers::none()
            },
            key: "q".into(),
            key_char: Some("q".into()),
        };
        assert_eq!(canonical_event_chord(&ks).as_deref(), Some("ctrl-q"));
        assert!(
            canonical_event_chord(&Keystroke {
                modifiers: Modifiers::none(),
                key: "control".into(),
                key_char: None,
            })
            .is_none()
        );
    }
    #[test]
    fn context_menu_owns_chat_keys_but_preserves_focus_and_window_commands() {
        for (key, platform, captured) in [
            ("a", false, true),
            ("enter", true, true),
            ("k", true, true),
            ("tab", false, false),
            ("enter", false, false),
            ("q", true, false),
            ("w", true, false),
        ] {
            assert_eq!(
                context_menu_captures_key(&Keystroke {
                    key: key.into(),
                    key_char: None,
                    modifiers: Modifiers {
                        platform,
                        ..Modifiers::none()
                    },
                }),
                captured
            );
        }
    }

    fn keymap() -> gpui_kit::Keymap {
        gpui_kit::Keymap::new(default_bindings())
    }

    /// The action name a chord resolves to with `contexts` focused.
    fn resolve(chord: &str, contexts: &[&str]) -> Option<String> {
        let stack: Vec<gpui_kit::KeyContext> = contexts
            .iter()
            .map(|c| gpui_kit::KeyContext::parse(c).unwrap())
            .collect();
        let input = [Keystroke::parse(chord).unwrap()];
        let (bindings, _) = keymap().bindings_for_input(&input, &stack);
        bindings.first().map(|b| b.action().name().to_string())
    }

    fn primary(rest: &str) -> String {
        format!(
            "{}-{rest}",
            if cfg!(target_os = "macos") {
                "cmd"
            } else {
                "ctrl"
            }
        )
    }

    #[test]
    fn composer_shortcuts_resolve_to_their_format_actions() {
        use quill::composer::COMPOSER_SHORTCUTS;
        let expected = [
            ("b", false, "quill_ui::FormatBold"),
            ("i", false, "quill_ui::FormatItalic"),
            ("u", false, "quill_ui::FormatUnderline"),
            ("x", true, "quill_ui::FormatStrikethrough"),
            ("m", true, "quill_ui::FormatMonospace"),
            ("p", true, "quill_ui::FormatSpoiler"),
            ("n", true, "quill_ui::FormatClear"),
            ("k", false, "quill_ui::ComposerEditLink"),
        ];
        assert_eq!(expected.len() + 1, COMPOSER_SHORTCUTS.len());
        for (key, shift, action) in expected {
            let chord = primary(&format!("{}{key}", if shift { "shift-" } else { "" }));
            assert_eq!(
                resolve(&chord, &["Workspace", "QuillComposer", "Input"]).as_deref(),
                Some(action),
                "{chord}"
            );
        }
    }

    /// GPUI on macOS reports Cmd+Shift+. as key ">" with Shift consumed
    /// (events.rs `parse_keystroke`); Linux/Windows may report either form.
    #[test]
    fn quote_chord_matches_the_keystroke_each_platform_reports() {
        let stack: Vec<gpui_kit::KeyContext> = ["QuillComposer", "Input"]
            .iter()
            .map(|c| gpui_kit::KeyContext::parse(c).unwrap())
            .collect();
        let reported = |key: &str, shift: bool, platform: bool| Keystroke {
            modifiers: Modifiers {
                shift,
                platform,
                control: !platform,
                ..Modifiers::none()
            },
            key: key.into(),
            key_char: None,
        };
        let platform = cfg!(target_os = "macos");
        let forms = [
            reported(">", false, platform),
            reported(".", true, platform),
        ];
        for form in forms {
            let (found, _) = keymap().bindings_for_input(std::slice::from_ref(&form), &stack);
            assert_eq!(
                found.first().map(|b| b.action().name()),
                Some("quill_ui::FormatBlockQuote"),
                "{form:?}"
            );
        }
    }

    #[test]
    fn plain_paste_chord_is_bound() {
        assert_eq!(
            resolve(&primary("shift-v"), &["Input"]).as_deref(),
            Some("quill_ui::ComposerPastePlain")
        );
    }

    #[test]
    fn link_chord_beats_quick_switch_only_inside_the_composer() {
        let chord = primary("k");
        assert_eq!(
            resolve(&chord, &["Workspace", "QuillComposer", "Input"]).as_deref(),
            Some("quill_ui::ComposerEditLink")
        );
        assert_eq!(
            resolve(&chord, &["Workspace"]).as_deref(),
            Some("quill_ui::OpenSearch")
        );
        assert_eq!(
            resolve(&chord, &[]).as_deref(),
            Some("quill_ui::OpenSearch")
        );
    }

    #[test]
    fn shortcut_pack_chords_resolve_to_their_actions() {
        let cases = [
            (primary("up"), "ReplyToPrevious"),
            (primary("down"), "ReplyToNext"),
            (primary("o"), "AttachFile"),
            ("pageup".to_string(), "HistoryPageUp"),
            ("pagedown".to_string(), "HistoryPageDown"),
            ("home".to_string(), "HistoryToTop"),
            ("end".to_string(), "HistoryToBottom"),
            ("delete".to_string(), "DeleteSelection"),
            ("backspace".to_string(), "DeleteSelection"),
            (primary("space"), "ToggleMessageSelection"),
            ("up".to_string(), "SelectionFocusOlder"),
            ("down".to_string(), "SelectionFocusNewer"),
            ("shift-up".to_string(), "SelectionExtendOlder"),
            ("shift-down".to_string(), "SelectionExtendNewer"),
            (primary("0"), "OpenSavedMessages"),
            (primary("9"), "OpenArchive"),
            (primary("j"), "OpenContacts"),
            (primary("r"), "MarkChatRead"),
            (primary("\\"), "ShowChatMenu"),
            (primary("]"), "ShowChatPreview"),
            (primary("alt-home"), "FirstChat"),
            (primary("alt-end"), "LastChat"),
            ("ctrl-shift-up".to_string(), "PrevFolder"),
            ("ctrl-shift-down".to_string(), "NextFolder"),
            (primary("pageup"), "PrevChat"),
            (primary("pagedown"), "NextChat"),
        ];
        for (chord, action) in cases {
            assert_eq!(
                resolve(&chord, &[]).as_deref(),
                Some(format!("quill_ui::{action}").as_str()),
                "{chord}"
            );
        }
    }

    #[test]
    fn pinned_chord_n_opens_pinned_chat_n() {
        let keymap = keymap();
        for n in 1..=8usize {
            let input = [Keystroke::parse(&primary(&n.to_string())).unwrap()];
            let (found, _) = keymap.bindings_for_input(&input, &[]);
            let action = found.first().unwrap().action();
            assert!(
                action.partial_eq(&OpenPinnedChat { index: n - 1 }),
                "{n} opens the wrong pinned chat"
            );
        }
        // Focus chat list moved off Cmd/Ctrl+1 to make room.
        assert_eq!(
            resolve("ctrl-alt-1", &[]).as_deref(),
            Some("quill_ui::FocusSidebar")
        );
    }

    #[test]
    fn composer_keeps_keys_only_its_handlers_release() {
        // The composer's own `Input` context binds these; the twins in the
        // composer context are what let the history handlers see them first.
        let mut bindings = default_bindings();
        bindings.extend(composer_bindings());
        let keymap = gpui_kit::Keymap::new(bindings);
        let stack: Vec<gpui_kit::KeyContext> = ["QuillComposer", "Input"]
            .iter()
            .map(|c| gpui_kit::KeyContext::parse(c).unwrap())
            .collect();
        for (chord, action) in [
            (primary("up"), "ReplyToPrevious"),
            (primary("down"), "ReplyToNext"),
            ("pageup".to_string(), "HistoryPageUp"),
            ("pagedown".to_string(), "HistoryPageDown"),
            (primary("]"), "ShowChatPreview"),
        ] {
            let input = [Keystroke::parse(&chord).unwrap()];
            let (found, _) = keymap.bindings_for_input(&input, &stack);
            assert_eq!(
                found[0].action().name(),
                format!("quill_ui::{action}"),
                "{chord}"
            );
        }
        // Home and End stay with the text field: no composer twin.
        assert!(
            composer_scoped_bindings()
                .iter()
                .all(|b| !matches!(b.keystrokes()[0].inner().key.as_str(), "home" | "end"))
        );
    }

    #[test]
    fn pack_chords_cannot_be_rebound_onto() {
        for chord in [
            primary("j"),
            primary("up"),
            "delete".to_string(),
            primary("9"),
        ] {
            assert_eq!(
                keybinding_conflict("open-search", &chord, &[]),
                Some(KeybindingConflict::FixedChrome),
                "{chord}"
            );
        }
    }

    #[test]
    fn rebound_quick_switch_keeps_working_inside_the_composer() {
        let live = resolve_keybindings(&[custom("open-search", "cmd-p")]);
        let row = live.iter().find(|row| row.id == "open-search").unwrap();
        assert_eq!(row.live, vec!["cmd-p".to_string()]);
        let mut bindings = vec![keybinding_for("open-search", "cmd-p").unwrap()];
        bindings.extend(composer_bindings());
        let keymap = gpui_kit::Keymap::new(bindings);
        let stack: Vec<gpui_kit::KeyContext> = ["QuillComposer", "Input"]
            .iter()
            .map(|c| gpui_kit::KeyContext::parse(c).unwrap())
            .collect();
        let (found, _) = keymap.bindings_for_input(&[Keystroke::parse("cmd-p").unwrap()], &stack);
        assert_eq!(found[0].action().name(), "quill_ui::OpenSearch");
    }

    #[test]
    fn format_defaults_use_only_the_platform_modifier() {
        for id in [
            "format-bold",
            "format-italic",
            "format-underline",
            "format-strikethrough",
            "format-monospace",
            "format-blockquote",
            "format-spoiler",
            "format-clear",
        ] {
            let action = REBINDABLE_ACTIONS.iter().find(|a| a.id == id).unwrap();
            assert_eq!(
                action.defaults.len(),
                if id == "format-blockquote" { 2 } else { 1 },
                "{id}"
            );
            let wanted = if cfg!(target_os = "macos") {
                "cmd-"
            } else {
                "ctrl-"
            };
            assert!(action.defaults[0].starts_with(wanted), "{id}");
        }
    }
}

/// Headless key delivery through the real GPUI dispatcher (needs the
/// `demo-capture` feature for gpui-kit's test support).
#[cfg(all(test, feature = "demo-capture"))]
mod dispatch_tests {
    use super::{COMPOSER_CONTEXT, default_bindings};
    use crate::ui::actions::{FormatBlockQuote, FormatStrikethrough};
    use gpui_kit::component::Root;
    use gpui_kit::component::input::{Textarea, TextareaState};
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, div, px, size,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Host {
        input: Entity<TextareaState>,
        seen: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .on_action(cx.listener(|this, _: &FormatStrikethrough, _, _| {
                    this.seen.borrow_mut().push("strike")
                }))
                .on_action(cx.listener(|this, _: &FormatBlockQuote, _, _| {
                    this.seen.borrow_mut().push("quote")
                }))
                .child(
                    div()
                        .id("composer")
                        .test_support()
                        .w(px(400.))
                        .key_context(COMPOSER_CONTEXT)
                        .child(Textarea::new(&self.input)),
                )
        }
    }

    #[gpui_kit::test]
    fn real_keystrokes_reach_the_format_actions(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| cx.bind_keys(default_bindings()));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let handle = cx.open_window(size(px(640.), px(240.)), |window, cx| {
            let input = cx.new(|cx| TextareaState::new(window, cx));
            let seen = seen.clone();
            let view = cx.new(|_| Host { input, seen });
            Root::new(view, window, cx)
        });
        let (strike, quote) = if cfg!(target_os = "macos") {
            ("cmd-shift-x", "cmd->")
        } else {
            ("ctrl-shift-x", "ctrl->")
        };
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.within("composer").press(strike, cx);
            window.within("composer").press(quote, cx);
        })
        .unwrap();
        assert_eq!(*seen.borrow(), vec!["strike", "quote"]);
    }
}

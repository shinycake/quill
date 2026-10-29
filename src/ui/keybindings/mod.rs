#![deny(clippy::indexing_slicing)]
use super::actions::{
    AiComposeApply, ArchiveOpenChat, AttachFile, CancelSearch, ChatSearchNewer, ChatSearchOlder,
    CloseWindow, ComposerEditLink, ComposerPastePlain, DeleteSelection, FirstChat, FocusComposer,
    FocusSidebar, FormatBlockQuote, FormatBold, FormatClear, FormatItalic, FormatMonospace,
    FormatSpoiler, FormatStrikethrough, FormatUnderline, HistoryPageDown, HistoryPageUp,
    HistoryToBottom, HistoryToTop, JustSend, LastChat, LoadOlder, LockApp, MarkChatRead,
    MinimizeWindow, NextChat, NextFolder, OpenArchive, OpenChatSearch, OpenContacts, OpenHelp,
    OpenPinnedChat, OpenSavedMessages, OpenSearch, OpenSettings, OpenShortcuts, PrevChat,
    PrevFolder, QuitApp, RecordRoundMessage, ReopenClosedWindow, ReplyToNext, ReplyToPrevious,
    ScheduleSend, SelectionExtendNewer, SelectionExtendOlder, SelectionFocusNewer,
    SelectionFocusOlder, SendSilently, ShowAdminLog, ShowChatMenu, ShowChatPreview,
    ShowScheduledMessages, StoryTogglePause, ToggleFullscreen, ToggleLinkPreview,
    ToggleMessageSelection, ToggleSidebar, ToggleTheme, ViewerCopy, ViewerFlipHorizontal,
    ViewerFlipVertical, ViewerNext, ViewerPrev, ViewerSave, ViewerZoomIn, ViewerZoomOut,
    ViewerZoomReset,
};
use super::actions::{HideApp, HideOtherApps};
use super::host_os::HOST_OS;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::settings::CustomKeybinding;

/// The platform's primary modifier plus a key, as a keystroke literal:
/// `cmd-` on macOS, `ctrl-` elsewhere (GPUI's `secondary`). Every app
/// shortcut uses only this, as Telegram Desktop does (Qt maps `ctrl` to Cmd
/// on macOS): macOS keeps Ctrl+K, Ctrl+F and friends for text editing, and
/// Linux and Windows leave the Super key to the desktop.
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

/// Hide / show the chat list column: macOS's own Show Sidebar chord
/// (Cmd+Ctrl+S), Ctrl+Shift+S elsewhere (Ctrl+S alone would read as Save).
#[cfg(target_os = "macos")]
pub(in crate::ui) const SIDEBAR_CHORD: &str = "cmd-ctrl-s";
#[cfg(not(target_os = "macos"))]
pub(in crate::ui) const SIDEBAR_CHORD: &str = "ctrl-shift-s";

/// Key context of the composer's Textarea wrapper (see
/// [`composer_bindings`]).
pub(super) const COMPOSER_CONTEXT: &str = "QuillComposer";

/// Where the link chord applies: the Textarea (kit context `Input`) inside
/// the composer wrapper. It matches at the same depth as the unscoped
/// quick-switch binding, so it must be added after it (a later binding wins
/// a depth tie), which `apply_custom_bindings` and `shortcut_rows` do.
const COMPOSER_INPUT_CONTEXT: &str = "QuillComposer > Input";

/// Also discard stale capture state once the Shortcuts page is gone.
pub(super) fn capture_active(
    shown: bool,
    capture: &mut Option<String>,
    error: &mut Option<(String, String)>,
) -> bool {
    if !shown {
        *capture = None;
        *error = None;
    }
    shown && capture.is_some()
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
    pub label: quill::lang::Key,
    /// Default keystrokes (all platforms).
    pub defaults: &'static [&'static str],
}

/// Parity slice (platform-custom-keybindings): the actions users may rebind.
/// Window-chrome and app-lifecycle bindings (quit, close, minimize, …) stay
/// fixed — rebinding those risks stranding the user.
pub const REBINDABLE_ACTIONS: &[RebindableAction] = &[
    RebindableAction {
        id: "focus-sidebar",
        label: keys::Q_PREFS_FOCUS_CHAT_LIST,
        // Cmd/Ctrl+1..8 open the pinned chats (tdesktop), so focusing the
        // list moved to the Alt variant.
        defaults: &[primary!("alt-1")],
    },
    RebindableAction {
        id: "focus-composer",
        label: keys::Q_PREFS_FOCUS_COMPOSER,
        defaults: &[primary!("shift-l")],
    },
    RebindableAction {
        id: "toggle-sidebar",
        label: keys::Q_PREFS_TOGGLE_SIDEBAR,
        defaults: &[SIDEBAR_CHORD],
    },
    RebindableAction {
        id: "next-chat",
        label: keys::Q_PREFS_NEXT_CHAT,
        defaults: &["alt-down", "ctrl-tab", primary!("pagedown")],
    },
    RebindableAction {
        id: "prev-chat",
        label: keys::Q_PREFS_PREVIOUS_CHAT,
        defaults: &["alt-up", "ctrl-shift-tab", primary!("pageup")],
    },
    RebindableAction {
        id: "load-older",
        label: keys::Q_PREFS_LOAD_OLDER_MESSAGES,
        // Cmd/Ctrl+Up replies to the previous message (tdesktop).
        defaults: &["alt-pageup"],
    },
    RebindableAction {
        id: "open-search",
        label: keys::Q_PREFS_SEARCH,
        defaults: &[primary!("k")],
    },
    RebindableAction {
        id: "open-chat-search",
        label: keys::Q_PREFS_SEARCH_IN_CHAT,
        defaults: &[primary!("f")],
    },
    RebindableAction {
        id: "chat-search-newer",
        label: keys::Q_PREFS_NEXT_SEARCH_RESULT,
        defaults: &[primary!("g")],
    },
    RebindableAction {
        id: "chat-search-older",
        label: keys::Q_PREFS_PREVIOUS_SEARCH_RESULT,
        defaults: &[primary!("shift-g")],
    },
    RebindableAction {
        id: "cancel-search",
        label: keys::Q_PREFS_CANCEL_SEARCH,
        defaults: &["escape"],
    },
    RebindableAction {
        id: "format-bold",
        label: keys::Q_PREFS_BOLD,
        defaults: &[primary!("b")],
    },
    RebindableAction {
        id: "format-italic",
        label: keys::Q_PREFS_ITALIC,
        defaults: &[primary!("i")],
    },
    RebindableAction {
        id: "format-underline",
        label: keys::Q_PREFS_UNDERLINE,
        defaults: &[primary!("u")],
    },
    RebindableAction {
        id: "format-strikethrough",
        label: keys::Q_PREFS_STRIKETHROUGH,
        defaults: &[primary!("shift-x")],
    },
    RebindableAction {
        id: "format-monospace",
        label: keys::Q_PREFS_MONOSPACE,
        defaults: &[primary!("shift-m")],
    },
    RebindableAction {
        id: "format-blockquote",
        label: keys::Q_PREFS_QUOTE,
        defaults: QUOTE_CHORDS,
    },
    RebindableAction {
        id: "format-spoiler",
        label: keys::Q_PREFS_SPOILER,
        defaults: &[primary!("shift-p")],
    },
    RebindableAction {
        id: "format-clear",
        label: keys::Q_PREFS_CLEAR_FORMATTING,
        defaults: &[primary!("shift-n")],
    },
    RebindableAction {
        id: "viewer-prev",
        label: keys::Q_PREFS_PREVIOUS_MEDIA,
        defaults: &["left"],
    },
    RebindableAction {
        id: "viewer-next",
        label: keys::Q_PREFS_NEXT_MEDIA,
        defaults: &["right"],
    },
    RebindableAction {
        id: "viewer-zoom-reset",
        label: keys::Q_PREFS_RESET_ZOOM,
        defaults: &["0"],
    },
    RebindableAction {
        id: "viewer-zoom-in",
        label: keys::Q_PREFS_ZOOM_IN,
        defaults: &["=", primary!("=")],
    },
    RebindableAction {
        id: "viewer-zoom-out",
        label: keys::Q_PREFS_ZOOM_OUT,
        defaults: &["-", primary!("-")],
    },
    RebindableAction {
        id: "viewer-flip-h",
        label: keys::Q_PREFS_FLIP_PHOTO_HORIZONTALLY,
        defaults: &["h"],
    },
    RebindableAction {
        id: "viewer-flip-v",
        label: keys::Q_PREFS_FLIP_PHOTO_VERTICALLY,
        defaults: &["v"],
    },
    RebindableAction {
        id: "viewer-copy",
        label: keys::Q_PREFS_COPY_PHOTO,
        defaults: &[primary!("c")],
    },
    RebindableAction {
        id: "story-pause",
        label: keys::Q_PREFS_PAUSE_OR_RESUME_STORY,
        defaults: &["space"],
    },
    RebindableAction {
        id: "viewer-save",
        label: keys::Q_PREFS_SAVE_MEDIA,
        defaults: &[primary!("s")],
    },
    // Commands of tdesktop's shortcut list that ship without a chord
    // (`core/shortcuts.cpp` `fillDefaults`), except reopening a window.
    RebindableAction {
        id: "reopen-closed-window",
        label: keys::Q_PREFS_REOPEN_CLOSED_WINDOW,
        defaults: &[primary!("shift-t")],
    },
    RebindableAction {
        id: "archive-chat",
        label: keys::Q_PREFS_ARCHIVE_CHAT,
        defaults: &[],
    },
    RebindableAction {
        id: "show-scheduled",
        label: keys::Q_PREFS_SCHEDULED_MESSAGES,
        defaults: &[],
    },
    RebindableAction {
        id: "just-send",
        label: keys::Q_PREFS_JUST_SEND,
        defaults: &[],
    },
    RebindableAction {
        id: "send-silently",
        label: keys::Q_PREFS_SILENT_SEND,
        defaults: &[],
    },
    RebindableAction {
        id: "schedule-send",
        label: keys::Q_PREFS_SCHEDULE,
        defaults: &[],
    },
    RebindableAction {
        id: "toggle-link-preview",
        label: keys::Q_PREFS_TOGGLE_LINK_PREVIEW,
        defaults: &[],
    },
    RebindableAction {
        id: "record-round",
        label: keys::Q_PREFS_RECORD_ROUND_MESSAGE,
        defaults: &[],
    },
    RebindableAction {
        id: "show-admin-log",
        label: keys::SHORTCUTS_ADMIN_LOG,
        defaults: &[],
    },
    RebindableAction {
        id: "ai-compose-apply",
        label: keys::SHORTCUTS_AI_COMPOSE_APPLY,
        defaults: &[],
    },
];

/// Parity slice (platform-custom-keybindings): build the key binding for a
/// rebindable action id and keystroke string. Returns `None` for unknown ids
/// or unparsable keystrokes (a bad saved pref never breaks startup).
pub fn keybinding_for(id: &str, keystroke: &str, cx: &App) -> Option<KeyBinding> {
    // An empty string parses as a blank key, which is not a usable chord.
    let Ok(parsed) = Keystroke::parse_with_os(keystroke, HOST_OS) else {
        return None;
    };
    if parsed.key.is_empty() {
        return None;
    }
    let action: Box<dyn Action> = match id {
        "focus-sidebar" => Box::new(FocusSidebar),
        "focus-composer" => Box::new(FocusComposer),
        "toggle-sidebar" => Box::new(ToggleSidebar),
        "next-chat" => Box::new(NextChat),
        "prev-chat" => Box::new(PrevChat),
        "load-older" => Box::new(LoadOlder),
        "open-search" => Box::new(OpenSearch),
        "open-chat-search" => Box::new(OpenChatSearch),
        "chat-search-newer" => Box::new(ChatSearchNewer),
        "chat-search-older" => Box::new(ChatSearchOlder),
        "cancel-search" => Box::new(CancelSearch),
        "format-bold" => Box::new(FormatBold),
        "format-italic" => Box::new(FormatItalic),
        "format-underline" => Box::new(FormatUnderline),
        "format-strikethrough" => Box::new(FormatStrikethrough),
        "format-monospace" => Box::new(FormatMonospace),
        "format-blockquote" => Box::new(FormatBlockQuote),
        "format-spoiler" => Box::new(FormatSpoiler),
        "format-clear" => Box::new(FormatClear),
        "viewer-prev" => Box::new(ViewerPrev),
        "viewer-next" => Box::new(ViewerNext),
        "viewer-zoom-reset" => Box::new(ViewerZoomReset),
        "viewer-zoom-in" => Box::new(ViewerZoomIn),
        "viewer-zoom-out" => Box::new(ViewerZoomOut),
        "viewer-flip-h" => Box::new(ViewerFlipHorizontal),
        "viewer-flip-v" => Box::new(ViewerFlipVertical),
        "viewer-copy" => Box::new(ViewerCopy),
        "viewer-save" => Box::new(ViewerSave),
        "story-pause" => Box::new(StoryTogglePause),
        "reopen-closed-window" => Box::new(ReopenClosedWindow),
        "archive-chat" => Box::new(ArchiveOpenChat),
        "show-scheduled" => Box::new(ShowScheduledMessages),
        "just-send" => Box::new(JustSend),
        "send-silently" => Box::new(SendSilently),
        "schedule-send" => Box::new(ScheduleSend),
        "toggle-link-preview" => Box::new(ToggleLinkPreview),
        "record-round" => Box::new(RecordRoundMessage),
        "ai-compose-apply" => Box::new(AiComposeApply),
        "show-admin-log" => Box::new(ShowAdminLog),
        _ => return None,
    };
    KeyBinding::load(keystroke, action, None, false, None, cx).ok()
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
    bindings.extend(composer_extra_rows().into_iter().map(|row| row.binding));
    bindings
}

/// Fixed bindings: window chrome and app lifecycle — not rebindable.
fn fixed_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new(primary!("q"), QuitApp, None),
        // kit Phase 7: window-chrome shortcuts (HIG: Cmd+W close, Cmd+M
        // minimize; Cmd+Ctrl+F / F11 fullscreen).
        KeyBinding::new(primary!(","), OpenSettings, None),
        KeyBinding::new(primary!("w"), CloseWindow, None),
        // Local passcode: Ctrl/Cmd+L as in tdesktop (Focus composer moved to
        // the Shift variant).
        KeyBinding::new(primary!("l"), LockApp, None),
        KeyBinding::new(primary!("m"), MinimizeWindow, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-ctrl-f", ToggleFullscreen, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("f11", ToggleFullscreen, None),
    ]
    .into_iter()
    .chain(app_menu_bindings())
    .collect()
}

/// The macOS app menu's Hide Quill (Cmd+H) and Hide Others (Cmd+Opt+H).
/// GPUI gives AppKit's standard commands no default chord, so without these
/// the keys did nothing.
fn app_menu_bindings() -> Vec<KeyBinding> {
    if cfg!(target_os = "macos") {
        vec![
            KeyBinding::new("cmd-h", HideApp, None),
            KeyBinding::new("cmd-alt-h", HideOtherApps, None),
        ]
    } else {
        Vec::new()
    }
}

fn default_bindings() -> Vec<KeyBinding> {
    let mut bindings: Vec<KeyBinding> =
        shortcut_rows().into_iter().map(|row| row.binding).collect();
    bindings.extend(app_menu_bindings());
    bindings.extend(composer_scoped_bindings());
    bindings.extend(super::chat_list_keys::chat_list_bindings());
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
    /// Reserved by quit, close, minimize, hide, or fullscreen.
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
    pub label: quill::lang::Key,
    pub live: Vec<String>,
    pub rejected: Option<KeybindingConflict>,
}

/// User-facing explanation. The chord named here is the one that was
/// refused, not the chord that remains active.
pub fn conflict_message(chord: &str, conflict: &KeybindingConflict) -> String {
    match conflict {
        KeybindingConflict::Invalid => {
            tr_with(keys::Q_PREFS_ISNT_A_VALID_SHORTCUT_AND, &[("chord", chord)])
        }
        KeybindingConflict::FixedChrome => {
            tr_with(keys::Q_PREFS_IS_RESERVED_FOR_A_BUILT, &[("chord", chord)])
        }
        KeybindingConflict::Rebindable { other_label } => tr_with(
            keys::Q_PREFS_IS_ALREADY_USED_BY_AND,
            &[("chord", chord), ("other_label", other_label)],
        ),
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
                other_label: tr(row.label),
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
                        other_label: owner_label(&resolved, &ks)
                            .unwrap_or(tr(keys::Q_PREFS_ANOTHER_SHORTCUT)),
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
    let ks = Keystroke::parse_with_os(source, HOST_OS).ok()?;
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
                .map(|_| tr(row.label))
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
            ks.unparse(HOST_OS),
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
            if let Some(kb) = keybinding_for(row.id, &chord, cx) {
                bindings.push(kb);
            }
        }
    }
    // After the rebindable rows: the composer link chord ties with quick
    // switch on depth and the later binding wins.
    bindings.extend(composer_bindings());
    bindings.extend(super::chat_list_keys::chat_list_bindings());
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
}

mod shortcut_rows;
pub use shortcut_rows::*;

mod pinned_chat_rows;
#[allow(unused_imports)]
pub use pinned_chat_rows::*;

#[cfg(test)]
mod tests {
    // NOTE: explicit imports, not `use super::*` — `gpui_kit::*` re-exports
    // gpui's `#[test]` proc macro, which would shadow the builtin test
    // attribute and fail macro expansion ("recursion limit reached").
    use super::{
        Action, HOST_OS, KeybindingConflict, Keystroke, Modifiers, OpenPinnedChat, QuitApp,
        REBINDABLE_ACTIONS, canonical_event_chord, capture_active, composer_bindings,
        composer_scoped_bindings, conflict_message, context_menu_captures_key, default_bindings,
        fixed_keystrokes, invalidate_account_keybindings, keybinding_conflict, resolve_keybindings,
    };
    use quill::settings::CustomKeybinding;

    /// `keybinding_for` in a test app.
    #[cfg(feature = "demo-capture")]
    fn binding_for(id: &str, keystroke: &str) -> Option<gpui_kit::KeyBinding> {
        gpui_kit::TestAppContext::single().update(|cx| super::keybinding_for(id, keystroke, cx))
    }

    fn custom(id: &str, keystroke: &str) -> CustomKeybinding {
        CustomKeybinding {
            id: id.to_string(),
            keystroke: keystroke.to_string(),
        }
    }

    #[test]
    #[cfg(feature = "demo-capture")]
    fn reference_table_matches_resolved_defaults() {
        let defaults = default_bindings();
        // macOS adds Hide and Hide Others.
        let expected = if cfg!(target_os = "macos") { 98 } else { 96 };
        assert_eq!(defaults.len(), expected);
        for row in resolve_keybindings(&[]) {
            for chord in row.live {
                let binding = binding_for(row.id, &chord).unwrap();
                assert!(defaults.iter().any(|default| default.action().name()
                    == binding.action().name()
                    && super::same_chord(
                        default.keystrokes()[0].inner(),
                        binding.keystrokes()[0].inner()
                    )));
            }
        }
    }

    #[test]
    #[cfg(feature = "demo-capture")]
    fn ctrl_l_locks_and_shift_variant_focuses_the_composer() {
        let lock = super::LockApp.name();
        let chords: Vec<String> = default_bindings()
            .iter()
            .filter(|binding| binding.action().name() == lock)
            .map(|binding| binding.keystrokes()[0].inner().unparse(HOST_OS))
            .collect();
        let expected = Keystroke::parse_with_os(&primary("l"), HOST_OS)
            .unwrap()
            .unparse(HOST_OS);
        assert_eq!(chords, vec![expected]);
        assert!(binding_for("focus-composer", &primary("shift-l")).is_some());
    }

    #[test]
    fn capture_only_consumes_keys_while_shortcuts_are_shown_and_armed() {
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
    #[cfg(feature = "demo-capture")]
    fn keybinding_for_valid_id_and_keystroke() {
        assert!(binding_for("focus-composer", "ctrl-shift-l").is_some());
        assert!(binding_for("cancel-search", "escape").is_some());
    }

    #[test]
    #[cfg(feature = "demo-capture")]
    fn keybinding_for_unknown_id_is_none() {
        assert!(binding_for("nope", "ctrl-shift-l").is_none());
    }

    #[test]
    #[cfg(feature = "demo-capture")]
    fn keybinding_for_invalid_keystroke_is_none() {
        // Must not panic on a bad saved chord.
        assert!(binding_for("focus-composer", "not-a-keystroke-%%%").is_none());
        assert!(binding_for("focus-composer", "").is_none());
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
    #[cfg(feature = "demo-capture")]
    fn custom_keystroke_on_fixed_keystroke_is_rejected() {
        // Mirrors apply_custom_bindings' collision rule: a custom Cmd/Ctrl+Q
        // for Search must not shadow the fixed Quit binding.
        let fixed = fixed_keystrokes();
        let kb = binding_for("open-search", &primary("q")).unwrap();
        assert!(kb.keystrokes().iter().any(|k| fixed.contains(k.inner())));
        let ok = binding_for("open-search", "ctrl-alt-k").unwrap();
        assert!(!ok.keystrokes().iter().any(|k| fixed.contains(k.inner())));
        let platform: &[&str] = if cfg!(target_os = "macos") {
            &["cmd-ctrl-f", "cmd-h", "cmd-alt-h"]
        } else {
            &["f11"]
        };
        let reserved = ["q", "w", "m", "l", ","].map(primary);
        for reserved in reserved
            .iter()
            .map(String::as_str)
            .chain(platform.iter().copied())
        {
            assert_eq!(
                keybinding_conflict("open-search", reserved, &[]),
                Some(KeybindingConflict::FixedChrome),
                "{reserved}"
            );
        }
    }

    /// Each platform's shortcuts use its own primary modifier only: macOS
    /// keeps Ctrl+letter for Cocoa text editing (Ctrl+A/E/F/B/K/…), and
    /// Linux and Windows leave Super chords to the desktop.
    #[test]
    fn shortcuts_leave_the_other_platforms_modifier_alone() {
        let mut bindings = default_bindings();
        bindings.extend(super::fixed_bindings());
        bindings.extend(composer_bindings());
        let defaults = REBINDABLE_ACTIONS
            .iter()
            .flat_map(|ra| ra.defaults)
            .map(|chord| Keystroke::parse_with_os(chord, HOST_OS).unwrap());
        let chords: Vec<Keystroke> = bindings
            .iter()
            .flat_map(|b| b.keystrokes().iter().map(|k| k.inner().clone()))
            .chain(defaults)
            .collect();
        for ks in chords {
            let chord = ks.unparse(HOST_OS);
            if cfg!(target_os = "macos") {
                let ctrl_letter = ks.modifiers.control
                    && !ks.modifiers.platform
                    && ks.key.len() == 1
                    && ks.key.chars().all(|c| c.is_ascii_alphabetic());
                assert!(!ctrl_letter, "{chord} takes a Cocoa text key");
            } else {
                assert!(!ks.modifiers.platform, "{chord} takes a Super chord");
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn cmd_h_hides_and_cmd_alt_h_hides_others() {
        assert_eq!(resolve("cmd-h", &[]).as_deref(), Some("quill_ui::HideApp"));
        assert_eq!(
            resolve("cmd-alt-h", &["Workspace", "QuillComposer", "Input"]).as_deref(),
            Some("quill_ui::HideOtherApps")
        );
        assert_eq!(
            resolve("cmd-h", &["Workspace", "QuillComposer", "Input"]).as_deref(),
            Some("quill_ui::HideApp")
        );
    }

    #[test]
    fn rebindable_conflict_blocks_a_shared_live_chord() {
        let focus = primary("shift-l");
        // Cmd/Ctrl+Shift+L is still Focus composer's live default.
        assert_eq!(
            keybinding_conflict("open-search", &focus, &[]),
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        // Moving Focus composer off it frees it. The new chord is taken.
        let moved = vec![custom("focus-composer", "ctrl-alt-9")];
        assert_eq!(keybinding_conflict("open-search", &focus, &moved), None);
        assert_eq!(
            keybinding_conflict("open-search", "ctrl-alt-9", &moved),
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        // An action may keep its own default.
        assert_eq!(keybinding_conflict("focus-composer", &focus, &[]), None);
        assert_eq!(
            keybinding_conflict("open-search", "not-a-keystroke-%%%", &[]),
            Some(KeybindingConflict::Invalid)
        );
    }

    #[test]
    fn rejected_chords_are_not_reported_as_live() {
        let quit = primary("q");
        let customs = vec![
            custom("focus-composer", "ctrl-alt-k"),
            custom("open-search", &quit),
            custom("format-bold", "ctrl-alt-k"),
        ];
        let resolved = resolve_keybindings(&customs);
        let focus = resolved
            .iter()
            .find(|row| row.id == "focus-composer")
            .unwrap();
        assert_eq!(focus.live, vec!["ctrl-alt-k".to_string()]);
        assert_eq!(focus.rejected, None);

        let search = resolved.iter().find(|row| row.id == "open-search").unwrap();
        assert_eq!(search.rejected, Some(KeybindingConflict::FixedChrome));
        assert_eq!(search.live, vec![primary("k")]);
        assert!(!search.live.contains(&quit));

        let bold = resolved.iter().find(|row| row.id == "format-bold").unwrap();
        assert_eq!(
            bold.rejected,
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        assert_eq!(bold.live, vec![primary("b")]);
        assert!(!bold.live.iter().any(|chord| chord == "ctrl-alt-k"));

        let message = conflict_message(&quit, &KeybindingConflict::FixedChrome);
        assert!(message.contains(&quit));
        assert!(message.contains("not applied"));
    }

    #[test]
    fn resolved_live_chords_do_not_overlap() {
        let customs = vec![
            custom("open-search", &primary("q")),
            custom("focus-composer", "ctrl-alt-k"),
            custom("format-bold", "ctrl-alt-k"),
            custom("format-italic", &primary("b")),
        ];
        let resolved = resolve_keybindings(&customs);
        let mut seen: Vec<Keystroke> = fixed_keystrokes();
        for row in &resolved {
            for chord in &row.live {
                let ks = Keystroke::parse_with_os(chord, HOST_OS).unwrap();
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
            ("h", true, false),
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
        let input = [Keystroke::parse_with_os(chord, HOST_OS).unwrap()];
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
            ("d", true, "quill_ui::ComposerEditDate"),
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
            (primary("shift-a"), "ToggleMessageSelection"),
            ("ctrl-space".to_string(), "ToggleMessageSelection"),
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
            let input = [Keystroke::parse_with_os(&primary(&n.to_string()), HOST_OS).unwrap()];
            let (found, _) = keymap.bindings_for_input(&input, &[]);
            let action = found.first().unwrap().action();
            assert!(
                action.partial_eq(&OpenPinnedChat { index: n - 1 }),
                "{n} opens the wrong pinned chat"
            );
        }
        // Focus chat list moved off Cmd/Ctrl+1 to make room.
        assert_eq!(
            resolve(&primary("alt-1"), &[]).as_deref(),
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
            let input = [Keystroke::parse_with_os(&chord, HOST_OS).unwrap()];
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
    #[cfg(feature = "demo-capture")]
    fn rebound_quick_switch_keeps_working_inside_the_composer() {
        let live = resolve_keybindings(&[custom("open-search", "cmd-p")]);
        let row = live.iter().find(|row| row.id == "open-search").unwrap();
        assert_eq!(row.live, vec!["cmd-p".to_string()]);
        let mut bindings = vec![binding_for("open-search", "cmd-p").unwrap()];
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
#[cfg(test)]
#[cfg(feature = "demo-capture")]
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

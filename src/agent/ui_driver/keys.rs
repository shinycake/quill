//! Key strings from agents (`"cmd-k"`, `"ctrl+shift+a enter"`) as plain
//! chords, and the chords the driver never presses.

use crate::composer::SendKeyMode;

/// One key with its modifiers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyChord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on macOS, Win elsewhere.
    pub cmd: bool,
    pub function: bool,
    /// GPUI's key name: a lowercase character, or `enter`, `escape`,
    /// `backspace`, `delete`, `tab`, `space`, `up`, `down`, `left`, `right`,
    /// `home`, `end`, `pageup`, `pagedown`, `f1`..`f35`.
    pub key: String,
}

impl KeyChord {
    /// The platform's primary modifier (Cmd on macOS, Ctrl elsewhere).
    pub fn primary(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.cmd
        } else {
            self.ctrl
        }
    }

    fn plain(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Self::default()
        }
    }

    /// `cmd-shift-k` style, GPUI's spelling.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.cmd {
            parts.push("cmd");
        }
        if self.function {
            parts.push("fn");
        }
        parts.push(self.key.as_str());
        parts.join("-")
    }
}

/// Aliases agents use, to GPUI's names.
fn canonical_key(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let key = match lower.as_str() {
        "return" | "enter" => "enter",
        "esc" | "escape" => "escape",
        "del" | "delete" => "delete",
        "backspace" | "bksp" => "backspace",
        "tab" => "tab",
        "space" | "spacebar" => "space",
        "up" | "arrowup" => "up",
        "down" | "arrowdown" => "down",
        "left" | "arrowleft" => "left",
        "right" | "arrowright" => "right",
        "home" => "home",
        "end" => "end",
        "pageup" | "pgup" => "pageup",
        "pagedown" | "pgdn" => "pagedown",
        "insert" => "insert",
        "plus" => "+",
        "minus" => "-",
        other => {
            if other.chars().count() == 1 {
                return Some(other.to_string());
            }
            if let Some(number) = other.strip_prefix('f')
                && number.parse::<u8>().is_ok_and(|n| (1..=35).contains(&n))
            {
                return Some(other.to_string());
            }
            return None;
        }
    };
    Some(key.to_string())
}

/// Parse one chord: modifiers and a key joined by `-` or `+`.
pub fn parse_chord(text: &str) -> Result<KeyChord, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(
            "Empty key. Pass something like \"cmd-k\", \"escape\" or \"ctrl-shift-a\".".into(),
        );
    }
    // A trailing separator is the key itself ("-", "cmd--", "shift-+").
    let (body, trailing) = match text.chars().last() {
        Some(last @ ('-' | '+')) => {
            let body = text.get(..text.len().saturating_sub(1)).unwrap_or("");
            let body = body.strip_suffix(['-', '+']).unwrap_or(body);
            (body, Some(last.to_string()))
        }
        _ => (text, None),
    };
    let mut chord = KeyChord::default();
    let mut key = None;
    let parts: Vec<&str> = body
        .split(['-', '+'])
        .filter(|p| trailing.is_none() || !p.is_empty())
        .collect();
    for part in parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => chord.ctrl = true,
            "alt" | "option" | "opt" => chord.alt = true,
            "shift" => chord.shift = true,
            "cmd" | "command" | "super" | "win" | "meta" => chord.cmd = true,
            "fn" | "function" => chord.function = true,
            "" => {
                return Err(format!(
                    "Could not read key \"{text}\". Join modifiers and the key with -, like \"cmd-k\"."
                ));
            }
            other => {
                if key.is_some() {
                    return Err(format!(
                        "\"{text}\" names two keys. Separate chords with spaces: \"cmd-k enter\"."
                    ));
                }
                // "K" is shift-k, as in GPUI.
                if part.len() == 1 && part.bytes().all(|b| b.is_ascii_uppercase()) {
                    chord.shift = true;
                }
                let name = canonical_key(other).ok_or_else(|| {
                    format!(
                        "Unknown key \"{other}\" in \"{text}\". Use a character, or enter, escape, tab, space, backspace, delete, up, down, left, right, home, end, pageup, pagedown, f1-f35."
                    )
                })?;
                key = Some(name);
            }
        }
    }
    if let Some(sep) = trailing {
        if key.is_some() {
            return Err(format!("\"{text}\" names two keys."));
        }
        key = Some(sep);
    }
    chord.key = key.ok_or_else(|| {
        format!("\"{text}\" has modifiers but no key. Add the key, like \"cmd-k\".")
    })?;
    Ok(chord)
}

/// Parse a space-separated sequence of chords.
pub fn parse_sequence(text: &str) -> Result<Vec<KeyChord>, String> {
    let chords: Result<Vec<KeyChord>, String> = text.split_whitespace().map(parse_chord).collect();
    let chords = chords?;
    if chords.is_empty() {
        return Err(
            "keys is empty. Pass one or more chords separated by spaces, like \"cmd-k\" or \"escape\"."
                .into(),
        );
    }
    if chords.len() > 32 {
        return Err("At most 32 chords per call.".into());
    }
    Ok(chords)
}

/// Chords that leave Quill, close its window or hide it, or paste the
/// user's clipboard (whatever is on it, into whatever has the focus):
/// never pressed for an agent. Returns what the chord would do.
pub fn forbidden(chord: &KeyChord) -> Option<&'static str> {
    let primary = chord.cmd || chord.ctrl;
    match chord.key.as_str() {
        "q" if primary => Some("quit Quill"),
        "w" if primary => Some("close the window"),
        "h" if chord.cmd && !chord.ctrl => Some("hide Quill"),
        "m" if chord.cmd && !chord.ctrl && !chord.shift => Some("minimise the window"),
        "f4" if chord.alt => Some("close the window"),
        "v" if primary => Some(PASTES),
        "insert" if chord.shift => Some(PASTES),
        _ => None,
    }
}

/// What a paste chord would do; the clipboard is the user's, not the
/// agent's (`type_text` types the agent's own text).
const PASTES: &str = "paste the user's clipboard into Quill";

/// Whether the chord sends the message in the composer under `mode`
/// (Telegram Desktop: Enter sends, or Ctrl/Cmd+Enter sends).
pub fn sends_message(chord: &KeyChord, mode: SendKeyMode) -> bool {
    if chord.key != "enter" {
        return false;
    }
    match mode {
        SendKeyMode::Enter => !chord.shift && !chord.alt,
        SendKeyMode::CtrlEnter => chord.ctrl || chord.cmd,
    }
}

/// The chord that inserts a line break in the composer under `mode`.
pub fn newline_chord(mode: SendKeyMode) -> KeyChord {
    match mode {
        SendKeyMode::Enter => KeyChord {
            shift: true,
            ..KeyChord::plain("enter")
        },
        SendKeyMode::CtrlEnter => KeyChord::plain("enter"),
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyChord, forbidden, newline_chord, parse_chord, parse_sequence, sends_message};
    use crate::composer::SendKeyMode;

    #[test]
    fn chords_parse_with_either_separator_and_aliases() {
        let chord = parse_chord("cmd-k").unwrap();
        assert!(chord.cmd && !chord.shift);
        assert_eq!(chord.key, "k");
        let chord = parse_chord("Ctrl+Shift+A").unwrap();
        assert!(chord.ctrl && chord.shift);
        assert_eq!(chord.key, "a");
        assert_eq!(parse_chord("Return").unwrap().key, "enter");
        assert_eq!(parse_chord("Esc").unwrap().key, "escape");
        assert_eq!(parse_chord("ArrowDown").unwrap().key, "down");
        assert_eq!(parse_chord("f12").unwrap().key, "f12");
        assert_eq!(parse_chord("option-space").unwrap().describe(), "alt-space");
        let upper = parse_chord("K").unwrap();
        assert!(upper.shift);
        assert_eq!(upper.key, "k");
        assert_eq!(parse_chord("cmd--").unwrap().key, "-");
        assert_eq!(parse_chord("shift-+").unwrap().key, "+");
        assert_eq!(parse_chord("-").unwrap().key, "-");
    }

    #[test]
    fn bad_chords_say_what_to_fix() {
        assert!(parse_chord("").unwrap_err().contains("cmd-k"));
        assert!(parse_chord("cmd").unwrap_err().contains("no key"));
        assert!(
            parse_chord("cmd-blorp")
                .unwrap_err()
                .contains("Unknown key")
        );
        assert!(parse_chord("a-b").unwrap_err().contains("two keys"));
        assert!(parse_chord("f99").unwrap_err().contains("Unknown key"));
    }

    #[test]
    fn sequences_split_on_spaces() {
        let chords = parse_sequence("cmd-k  escape\tenter").unwrap();
        assert_eq!(chords.len(), 3);
        assert_eq!(chords[1].key, "escape");
        assert!(parse_sequence("   ").is_err());
        let many = vec!["a"; 33].join(" ");
        assert!(parse_sequence(&many).unwrap_err().contains("32"));
    }

    #[test]
    fn quit_close_and_hide_are_forbidden() {
        assert_eq!(
            forbidden(&parse_chord("cmd-q").unwrap()),
            Some("quit Quill")
        );
        assert_eq!(
            forbidden(&parse_chord("ctrl-q").unwrap()),
            Some("quit Quill")
        );
        assert!(forbidden(&parse_chord("cmd-w").unwrap()).is_some());
        assert!(forbidden(&parse_chord("alt-f4").unwrap()).is_some());
        assert!(forbidden(&parse_chord("cmd-h").unwrap()).is_some());
        assert_eq!(forbidden(&parse_chord("cmd-k").unwrap()), None);
        assert_eq!(forbidden(&parse_chord("q").unwrap()), None);
    }

    #[test]
    fn paste_chords_are_forbidden() {
        for chord in [
            "cmd-v",
            "ctrl-v",
            "cmd-shift-v",
            "ctrl-shift-v",
            "shift-insert",
        ] {
            assert!(
                forbidden(&parse_chord(chord).unwrap()).is_some(),
                "{chord} pastes the user's clipboard"
            );
        }
        assert_eq!(forbidden(&parse_chord("v").unwrap()), None);
        assert_eq!(forbidden(&parse_chord("insert").unwrap()), None);
        assert_eq!(forbidden(&parse_chord("cmd-c").unwrap()), None);
    }

    #[test]
    fn send_chords_follow_the_send_key_setting() {
        let enter = parse_chord("enter").unwrap();
        let shift_enter = parse_chord("shift-enter").unwrap();
        let cmd_enter = parse_chord("cmd-enter").unwrap();
        assert!(sends_message(&enter, SendKeyMode::Enter));
        assert!(!sends_message(&shift_enter, SendKeyMode::Enter));
        assert!(sends_message(&cmd_enter, SendKeyMode::Enter));
        assert!(!sends_message(&enter, SendKeyMode::CtrlEnter));
        assert!(sends_message(&cmd_enter, SendKeyMode::CtrlEnter));
        assert!(!sends_message(
            &parse_chord("a").unwrap(),
            SendKeyMode::Enter
        ));
        assert_eq!(newline_chord(SendKeyMode::Enter), shift_enter);
        assert_eq!(
            newline_chord(SendKeyMode::CtrlEnter),
            KeyChord::plain("enter")
        );
    }
}

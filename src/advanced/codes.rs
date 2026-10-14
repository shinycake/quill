//! Secret codes: typing a word on the sign-in screen or in Settings runs a
//! hidden action, as in tdesktop's `CodesFeedString`
//! (`settings/settings_codes.cpp`). Keystrokes pile up in a buffer; a
//! complete code runs and clears it, text that cannot start any code is
//! dropped from the front.

/// The hidden actions Quill has a counterpart for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// `debugmode`: ask to turn the debug logs on or off.
    DebugMode,
    /// `viewlogs`: show the log folder.
    ViewLogs,
    /// `testmode`: switch to the test environment or back (sign-in only).
    TestMode,
    /// `numberbuttons`: toggle the fast buttons experimental option.
    NumberButtons,
}

const CODES: &[(&str, Code)] = &[
    ("debugmode", Code::DebugMode),
    ("viewlogs", Code::ViewLogs),
    ("testmode", Code::TestMode),
    ("numberbuttons", Code::NumberButtons),
];

/// The typed-text buffer (`secret` in tdesktop).
#[derive(Debug, Default)]
pub struct CodeFeeder {
    secret: Vec<char>,
}

impl CodeFeeder {
    /// Add typed text. Returns the code it completed, if any.
    pub fn feed(&mut self, text: &str) -> Option<Code> {
        for c in text.chars().flat_map(char::to_lowercase) {
            self.secret.push(c);
        }
        let size = self.secret.len();
        let mut from = 0;
        let mut found = None;
        while size > from {
            let piece: String = self.secret.get(from..).unwrap_or_default().iter().collect();
            if let Some((_, code)) = CODES.iter().find(|(key, _)| *key == piece) {
                found = Some(*code);
                from = size;
                break;
            }
            if CODES.iter().any(|(key, _)| key.starts_with(&piece)) {
                break;
            }
            from += 1;
        }
        self.secret = self.secret.get(from..).unwrap_or_default().to_vec();
        found
    }
}

#[cfg(test)]
mod tests {
    use super::{Code, CodeFeeder};

    fn type_all(feeder: &mut CodeFeeder, text: &str) -> Option<Code> {
        let mut last = None;
        for c in text.chars() {
            last = feeder.feed(&c.to_string()).or(last);
        }
        last
    }

    #[test]
    fn a_typed_code_runs_once_and_clears_the_buffer() {
        let mut feeder = CodeFeeder::default();
        assert_eq!(type_all(&mut feeder, "debugmode"), Some(Code::DebugMode));
        assert_eq!(feeder.feed("x"), None);
        assert_eq!(type_all(&mut feeder, "testmode"), Some(Code::TestMode));
    }

    #[test]
    fn noise_before_a_code_is_dropped() {
        let mut feeder = CodeFeeder::default();
        assert_eq!(
            type_all(&mut feeder, "qdebdebugmode"),
            Some(Code::DebugMode)
        );
        assert_eq!(type_all(&mut feeder, "viewlogs"), Some(Code::ViewLogs));
    }

    #[test]
    fn codes_ignore_case_and_accept_pasted_text() {
        let mut feeder = CodeFeeder::default();
        assert_eq!(feeder.feed("NumberButtons"), Some(Code::NumberButtons));
    }

    #[test]
    fn a_partial_code_waits_for_the_rest() {
        let mut feeder = CodeFeeder::default();
        assert_eq!(feeder.feed("test"), None);
        assert_eq!(feeder.feed("mode"), Some(Code::TestMode));
        assert_eq!(feeder.feed("testmod"), None);
        assert_eq!(feeder.feed("x"), None);
        assert_eq!(feeder.feed("e"), None);
    }
}

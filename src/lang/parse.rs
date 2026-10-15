//! Parser for `.strings` files (`"key" = "value";`, the Apple strings format
//! tdesktop's `lang.strings` and Telegram's translation platform exports
//! use). Shared by `build.rs` (which turns `lang/en/*.strings` into the key
//! constants) and the runtime (community overlays in `lang/<code>/`), so it
//! depends on nothing but `std`.

/// One `"key" = "value";` entry. A plural form keeps its `#one`/`#other`
/// suffix in `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    /// 1-based line of the key, for error messages.
    pub line: usize,
}

/// Parses a whole file. Comments (`//` to the end of the line and
/// `/* … */`) and whitespace between entries are skipped. Escapes in
/// values: `\"`, `\\`, `\n`, `\t`.
pub fn parse_strings(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut chars = text.chars().peekable();
    let mut line = 1usize;
    loop {
        skip_blank(&mut chars, &mut line)?;
        let Some(&c) = chars.peek() else {
            return Ok(entries);
        };
        if c != '"' {
            return Err(format!("line {line}: expected '\"', found {c:?}"));
        }
        let key_line = line;
        let key = quoted(&mut chars, &mut line)?;
        skip_blank(&mut chars, &mut line)?;
        expect(&mut chars, '=', line)?;
        skip_blank(&mut chars, &mut line)?;
        let value = quoted(&mut chars, &mut line)?;
        skip_blank(&mut chars, &mut line)?;
        expect(&mut chars, ';', line)?;
        entries.push(Entry {
            key,
            value,
            line: key_line,
        });
    }
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

fn expect(chars: &mut Chars<'_>, want: char, line: usize) -> Result<(), String> {
    match chars.next() {
        Some(c) if c == want => Ok(()),
        other => Err(format!("line {line}: expected {want:?}, found {other:?}")),
    }
}

fn skip_blank(chars: &mut Chars<'_>, line: &mut usize) -> Result<(), String> {
    while let Some(&c) = chars.peek() {
        if c == '\n' {
            *line += 1;
            chars.next();
        } else if c.is_whitespace() {
            chars.next();
        } else if c == '/' {
            chars.next();
            match chars.next() {
                Some('/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            *line += 1;
                            break;
                        }
                    }
                }
                Some('*') => {
                    let mut prev = ' ';
                    let mut closed = false;
                    for c in chars.by_ref() {
                        if c == '\n' {
                            *line += 1;
                        }
                        if prev == '*' && c == '/' {
                            closed = true;
                            break;
                        }
                        prev = c;
                    }
                    if !closed {
                        return Err(format!("line {line}: unterminated comment"));
                    }
                }
                other => return Err(format!("line {line}: unexpected '/' then {other:?}")),
            }
        } else {
            break;
        }
    }
    Ok(())
}

fn quoted(chars: &mut Chars<'_>, line: &mut usize) -> Result<String, String> {
    expect(chars, '"', *line)?;
    let mut out = String::new();
    loop {
        match chars.next() {
            None => return Err(format!("line {line}: unterminated string")),
            Some('"') => return Ok(out),
            Some('\\') => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(c @ ('"' | '\\')) => out.push(c),
                other => return Err(format!("line {line}: unknown escape \\{other:?}")),
            },
            Some(c) => {
                if c == '\n' {
                    *line += 1;
                }
                out.push(c);
            }
        }
    }
}

/// The plural form a key suffix names (`#zero` … `#other`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Form {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl Form {
    pub const ALL: [Form; 6] = [
        Form::Zero,
        Form::One,
        Form::Two,
        Form::Few,
        Form::Many,
        Form::Other,
    ];

    pub fn suffix(self) -> &'static str {
        match self {
            Form::Zero => "zero",
            Form::One => "one",
            Form::Two => "two",
            Form::Few => "few",
            Form::Many => "many",
            Form::Other => "other",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Splits `key#one` into (`key`, `Some(Form::One)`); a key without a
/// known suffix is ordinary.
pub fn split_plural(key: &str) -> (&str, Option<Form>) {
    if let Some((base, suffix)) = key.rsplit_once('#') {
        for form in Form::ALL {
            if form.suffix() == suffix {
                return (base, Some(form));
            }
        }
    }
    (key, None)
}

/// Whether `{` and `}` pair up (no nesting) in a value.
pub fn braces_balanced(value: &str) -> bool {
    let mut open = false;
    for c in value.chars() {
        match c {
            '{' if open => return false,
            '{' => open = true,
            '}' if !open => return false,
            '}' => open = false,
            _ => {}
        }
    }
    !open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_comments_and_escapes() {
        let text = "// header\n\"a\" = \"One \\\"two\\\"\";\n/* block\n */ \"b#one\"=\"x\\ny\" ;\n";
        let entries = parse_strings(text).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "a");
        assert_eq!(entries[0].value, "One \"two\"");
        assert_eq!(entries[1].key, "b#one");
        assert_eq!(entries[1].value, "x\ny");
        assert_eq!(entries[1].line, 4);
    }

    #[test]
    fn rejects_broken_input() {
        assert!(parse_strings("\"a\" = \"b\"").is_err());
        assert!(parse_strings("\"a\" \"b\";").is_err());
        assert!(parse_strings("\"a\" = \"b;").is_err());
        assert!(parse_strings("/* open").is_err());
    }

    #[test]
    fn plural_suffix_and_braces() {
        assert_eq!(split_plural("lng_x#few"), ("lng_x", Some(Form::Few)));
        assert_eq!(split_plural("lng_x"), ("lng_x", None));
        assert_eq!(split_plural("lng_x#odd"), ("lng_x#odd", None));
        assert!(braces_balanced("{a} and {b}"));
        assert!(!braces_balanced("{a"));
        assert!(!braces_balanced("a}"));
        assert!(!braces_balanced("{{a}}"));
    }
}

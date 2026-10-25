//! Paging offsets in `td_call` answers. TDLib builds some from the last
//! result (`searchMessages` packs the date, chat id and message id of the
//! last match), so an offset passed through would name chats the agent may
//! not see. Agents get an opaque token instead, which Quill maps back when
//! it comes in as `offset`.

use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;

const PREFIX: &str = "quill-offset:";
/// Tokens kept; older ones expire.
const KEPT: usize = 512;

struct Tokens {
    next: u64,
    issued: VecDeque<(u64, String)>,
}

static TOKENS: Mutex<Tokens> = Mutex::new(Tokens {
    next: 1,
    issued: VecDeque::new(),
});

fn issue(offset: String) -> String {
    let mut tokens = crate::locks::lock_or_recover(&TOKENS);
    let id = tokens.next;
    tokens.next = tokens.next.wrapping_add(1);
    if tokens.issued.len() >= KEPT {
        tokens.issued.pop_front();
    }
    tokens.issued.push_back((id, offset));
    format!("{PREFIX}{id}")
}

fn lookup(token: &str) -> Option<String> {
    let id: u64 = token.strip_prefix(PREFIX)?.parse().ok()?;
    let tokens = crate::locks::lock_or_recover(&TOKENS);
    tokens
        .issued
        .iter()
        .find(|(issued, _)| *issued == id)
        .map(|(_, offset)| offset.clone())
}

/// Replace every non-empty `next_offset` string in an answer with a token.
pub fn hide(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, inner) in fields.iter_mut() {
                match inner {
                    Value::String(offset) if key == "next_offset" && !offset.is_empty() => {
                        *inner = Value::String(issue(std::mem::take(offset)));
                    }
                    _ => hide(inner),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(hide),
        _ => {}
    }
}

/// The real offset behind a token in the request's top-level `offset`.
pub fn reveal(request: &mut Value) -> Result<(), String> {
    let Some(Value::String(offset)) = request.get_mut("offset") else {
        return Ok(());
    };
    if !offset.starts_with(PREFIX) {
        return Ok(());
    }
    *offset = lookup(offset).ok_or(
        "That offset has expired. Start again with \"offset\": \"\" and follow next_offset.",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{hide, reveal};
    use serde_json::json;

    #[test]
    fn offsets_round_trip_as_tokens() {
        let mut found = json!({"messages": [], "next_offset": "1761300000,-1001234567890,4567"});
        hide(&mut found);
        let token = found["next_offset"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(!token.contains("1001234567890"));
        let mut next = json!({"@type": "searchMessages", "offset": token});
        assert_eq!(reveal(&mut next), Ok(()));
        assert_eq!(next["offset"], json!("1761300000,-1001234567890,4567"));
        let mut stale = json!({"offset": "quill-offset:999999"});
        assert!(reveal(&mut stale).is_err());
        let mut end = json!({"next_offset": ""});
        hide(&mut end);
        assert_eq!(end["next_offset"], json!(""));
    }
}

//! Paging offsets in `td_call` answers. TDLib builds some from the last
//! result (`searchMessages` packs the date, chat id and message id of the
//! last match), so an offset passed through would name chats the agent may
//! not see. Agents get an opaque token instead, which Quill maps back when
//! it comes in as `offset`. A token is bound to the account, agent and method
//! that got it, and refused for any other.

use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;

const PREFIX: &str = "quill-offset:";
/// Tokens kept; older ones expire.
const KEPT: usize = 512;

struct Tokens {
    next: u64,
    issued: VecDeque<(u64, String, String)>,
}

static TOKENS: Mutex<Tokens> = Mutex::new(Tokens {
    next: 1,
    issued: VecDeque::new(),
});

/// What a token is bound to: the account, the agent and the method.
pub fn binding(account: &crate::ids::AccountKey, agent: &str, method: &str) -> String {
    format!("{}\u{1f}{agent}\u{1f}{method}", account.0)
}

fn issue(offset: String, binding: &str) -> String {
    let mut tokens = crate::locks::lock_or_recover(&TOKENS);
    let id = tokens.next;
    tokens.next = tokens.next.wrapping_add(1);
    if tokens.issued.len() >= KEPT {
        tokens.issued.pop_front();
    }
    tokens.issued.push_back((id, binding.to_string(), offset));
    format!("{PREFIX}{id}")
}

enum Lookup {
    Found(String),
    Unknown,
    OtherOwner,
}

fn lookup(token: &str, binding: &str) -> Lookup {
    let Some(id) = token
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.parse::<u64>().ok())
    else {
        return Lookup::Unknown;
    };
    let tokens = crate::locks::lock_or_recover(&TOKENS);
    match tokens.issued.iter().find(|(issued, _, _)| *issued == id) {
        Some((_, owner, offset)) if owner == binding => Lookup::Found(offset.clone()),
        Some(_) => Lookup::OtherOwner,
        None => Lookup::Unknown,
    }
}

/// Replace every non-empty `next_offset` string in an answer with a token.
pub fn hide(value: &mut Value, binding: &str) {
    match value {
        Value::Object(fields) => {
            for (key, inner) in fields.iter_mut() {
                match inner {
                    Value::String(offset) if key == "next_offset" && !offset.is_empty() => {
                        *inner = Value::String(issue(std::mem::take(offset), binding));
                    }
                    _ => hide(inner, binding),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| hide(item, binding)),
        _ => {}
    }
}

/// The real offset behind a token in the request's top-level `offset`.
pub fn reveal(request: &mut Value, binding: &str) -> Result<(), String> {
    let Some(Value::String(offset)) = request.get_mut("offset") else {
        return Ok(());
    };
    if !offset.starts_with(PREFIX) {
        return Ok(());
    }
    match lookup(offset, binding) {
        Lookup::Found(real) => {
            *offset = real;
            Ok(())
        }
        Lookup::Unknown => Err(
            "That offset has expired. Start again with \"offset\": \"\" and follow next_offset."
                .to_string(),
        ),
        Lookup::OtherOwner => Err(
            "That offset came from another account, agent or method. Start again with \"offset\": \"\" and follow next_offset."
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{binding, hide, reveal};
    use serde_json::json;

    #[test]
    fn offsets_round_trip_as_tokens() {
        let mut found = json!({"messages": [], "next_offset": "1761300000,-1001234567890,4567"});
        hide(&mut found, "a");
        let token = found["next_offset"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(!token.contains("1001234567890"));
        let mut next = json!({"@type": "searchMessages", "offset": token});
        assert_eq!(reveal(&mut next, "a"), Ok(()));
        assert_eq!(next["offset"], json!("1761300000,-1001234567890,4567"));
        let mut stale = json!({"offset": "quill-offset:999999"});
        assert!(reveal(&mut stale, "a").is_err());
        let mut end = json!({"next_offset": ""});
        hide(&mut end, "a");
        assert_eq!(end["next_offset"], json!(""));
    }

    #[test]
    fn a_token_is_refused_for_another_account_agent_or_method() {
        let account = crate::ids::AccountKey("x".into());
        let mine = binding(&account, "Agent A", "searchMessages");
        let mut found = json!({"next_offset": "1,2,3"});
        hide(&mut found, &mine);
        let token = found["next_offset"].clone();
        for other in [
            binding(
                &crate::ids::AccountKey("y".into()),
                "Agent A",
                "searchMessages",
            ),
            binding(&account, "Agent B", "searchMessages"),
            binding(&account, "Agent A", "searchChatMessages"),
        ] {
            let mut request = json!({"offset": token});
            assert!(reveal(&mut request, &other).is_err());
        }
        let mut request = json!({"offset": token});
        assert_eq!(reveal(&mut request, &mine), Ok(()));
    }
}

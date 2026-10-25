//! The TDLib schema Quill is built against (`schema/td_api.tl`), read once
//! when an agent first calls `td_call`. `td_call` uses it to accept only
//! real methods, to drop fields TDLib would ignore (so a request cannot
//! carry padding or decoy ids), and to learn whether a method acts on a
//! chat (a top-level `chat_id`).

use std::collections::HashMap;
use std::sync::OnceLock;

const SCHEMA: &str = include_str!("../../../schema/td_api.tl");

/// One function or constructor: its fields and their TL types.
pub struct Entry {
    pub fields: Vec<(String, String)>,
}

impl Entry {
    pub fn has(&self, field: &str) -> bool {
        self.fields.iter().any(|(name, _)| name == field)
    }

    pub fn type_of(&self, field: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, kind)| kind.as_str())
    }
}

#[derive(Default)]
pub struct Schema {
    pub functions: HashMap<String, Entry>,
    pub constructors: HashMap<String, Entry>,
}

fn parse(text: &str) -> Schema {
    let mut schema = Schema::default();
    let mut in_functions = false;
    for line in text.lines() {
        let line = line.trim();
        if line == "---functions---" {
            in_functions = true;
            continue;
        }
        if line.is_empty() || line.starts_with("//") || line.starts_with("---") {
            continue;
        }
        let Some((left, _result)) = line.split_once(" = ") else {
            continue;
        };
        let mut words = left.split_whitespace();
        let Some(name) = words.next() else {
            continue;
        };
        let fields = words
            .filter_map(|word| word.split_once(':'))
            .map(|(field, kind)| (field.to_string(), kind.to_string()))
            .collect();
        let target = if in_functions {
            &mut schema.functions
        } else {
            &mut schema.constructors
        };
        target.insert(name.to_string(), Entry { fields });
    }
    schema
}

pub fn get() -> &'static Schema {
    static PARSED: OnceLock<Schema> = OnceLock::new();
    PARSED.get_or_init(|| parse(SCHEMA))
}

#[cfg(test)]
mod tests {
    use super::get;

    #[test]
    fn the_schema_knows_methods_and_their_fields() {
        let schema = get();
        let send = schema.functions.get("sendMessage");
        assert!(send.is_some_and(|f| f.type_of("chat_id") == Some("int53")));
        let join = schema.functions.get("joinChatByInviteLink");
        assert!(join.is_some_and(|f| !f.has("chat_id")));
        assert!(schema.constructors.contains_key("inputMessageText"));
        assert!(!schema.functions.contains_key("inputMessageText"));
        assert!(schema.functions.len() > 900);
    }
}

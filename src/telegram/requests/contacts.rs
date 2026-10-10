use crate::ids::{ChatId, RequestId};
use serde_json::json;

/// `getMe` (TDLib 1.8.67). Sent once so `getChatMember` can resolve the
/// current user; only the response id is kept.
pub fn get_me(extra: RequestId) -> String {
    json!({
        "@type": "getMe",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getSupportUser` (TDLib 1.8.68): the Telegram Support volunteer account
/// that Settings > Ask a Question writes to. Response is `user`.
pub fn get_support_user(extra: RequestId) -> String {
    json!({
        "@type": "getSupportUser",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getChatMember` for the current user in a channel (TDLib 1.8.67). Response
/// is `chatMember`.
pub fn get_chat_member(extra: RequestId, chat_id: ChatId, user_id: i64) -> String {
    json!({
        "@type": "getChatMember",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "member_id": { "@type": "messageSenderUser", "user_id": user_id },
    })
    .to_string()
}

/// `getUserFullInfo` for a bot user (TDLib 1.8.67,
/// `getUserFullInfo user_id:int53 = UserFullInfo`). Response is
/// `userFullInfo`; `bot_info` feeds the bot panel.
pub fn get_user_full_info(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "getUserFullInfo",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// Phase 6: `getContacts` (TDLib 1.8.67, `schema/td_api.tl:14520`):
/// `getContacts = Users;` — no parameters. Response is `users`
/// (`total_count:int32 user_ids:vector<int53>`, line 2471); the user
/// objects themselves arrive via `updateUser`.
pub fn get_contacts(extra: RequestId) -> String {
    json!({
        "@type": "getContacts",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Phase 6: `addContact` (TDLib 1.8.67, `schema/td_api.tl:14513`):
/// `addContact user_id:int53 contact:importedContact
/// share_phone_number:Bool = Ok;`
/// with `importedContact phone_number:string first_name:string
/// last_name:string note:formattedText = ImportedContact` (line 7382).
/// `note` is the contact's private note as plain text (an empty string
/// clears it, so the edit-contact box always sends the current note) and
/// `share_phone_number` shares the current user's number with the contact.
pub fn add_contact(
    extra: RequestId,
    user_id: i64,
    phone_number: &str,
    first_name: &str,
    last_name: &str,
    note: &str,
    share_phone_number: bool,
) -> String {
    json!({
        "@type": "addContact",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "contact": {
            "@type": "importedContact",
            "phone_number": phone_number,
            "first_name": first_name,
            "last_name": last_name,
            "note": { "@type": "formattedText", "text": note, "entities": [] },
        },
        "share_phone_number": share_phone_number,
    })
    .to_string()
}

/// Slice A6: `removeContacts` (TDLib 1.8.67, `schema/td_api.tl:14528`):
/// `removeContacts user_ids:vector<int53> = Ok;` (TGX
/// `TdlibUi.deleteContact` → `RemoveContacts`).
pub fn remove_contacts(extra: RequestId, user_ids: &[i64]) -> String {
    json!({
        "@type": "removeContacts",
        "@extra": extra.as_extra(),
        "user_ids": user_ids,
    })
    .to_string()
}

/// Slice A6: `clearImportedContacts` (TDLib 1.8.67,
/// `schema/td_api.tl:14539`): `clearImportedContacts = Ok;` — "Clears
/// all imported contacts, contact list remains unchanged". This is the
/// server-side half of TGX's "Delete synced contacts"
/// (`TdlibContactManager.deleteContacts`, `SyncContactsDeleteInfo`).
pub fn clear_imported_contacts(extra: RequestId) -> String {
    json!({
        "@type": "clearImportedContacts",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice A6: one parsed vCard contact — the honest subset of RFC 6350
/// that maps onto `importedContact` (schema 1.8.67, line 7382):
/// `importedContact phone_number:string first_name:string
/// last_name:string note:formattedText = ImportedContact;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedContact {
    pub phone_number: String,
    pub first_name: String,
    pub last_name: String,
    pub note: String,
}

/// Slice A6: cap on one vCard import — one `importContacts` call with an
/// unbounded contact list is a timeout farm; `parse_vcard` truncates at
/// this limit and reports the dropped count so the import dialog can say
/// so honestly. Raise only if TDLib documents a real limit.
pub const VCARD_IMPORT_LIMIT: usize = 500;

/// Slice A6: parse vCard text (`.vcf`, RFC 6350) into
/// `ImportedContact`s. Honest mapping — what the schema supports and
/// nothing else:
/// - `FN` → first/last name (split on the first space); `N` (family;
///   given) wins when both are present;
/// - `TEL` → `phone_number` — one contact per number, preferring
///   `TYPE=CELL`, then `TYPE=VOICE`, then the first number;
/// - `NOTE` → `note` (plain text, no entities);
/// - everything else (`EMAIL`, `ADR`, `ORG`, `URL`, `PHOTO`, `BDAY`,
///   …) has no `importedContact` field and is dropped.
///
/// Returns `(contacts, skipped_without_phone, truncated_by_limit)`.
pub fn parse_vcard(text: &str) -> (Vec<ImportedContact>, usize, usize) {
    // Unfold: a line starting with space/tab continues the previous
    // line (the continuation whitespace is dropped).
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if (line.starts_with(' ') || line.starts_with('\t')) && !lines.is_empty() {
            let prev = lines.len() - 1;
            lines[prev].push_str(line[1..].trim_start_matches([' ', '\t']));
        } else {
            lines.push(line.to_string());
        }
    }
    let mut contacts = Vec::new();
    let mut skipped = 0usize;
    let mut card: Vec<(String, Vec<String>, String)> = Vec::new();
    let mut in_card = false;
    let flush = |card: &mut Vec<(String, Vec<String>, String)>,
                 contacts: &mut Vec<ImportedContact>,
                 skipped: &mut usize| {
        if !card.is_empty() {
            let parsed = parse_vcard_block(card);
            if parsed.is_empty() {
                *skipped += 1;
            } else {
                contacts.extend(parsed);
            }
        }
        card.clear();
    };
    for line in &lines {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VCARD" {
            flush(&mut card, &mut contacts, &mut skipped);
            in_card = true;
        } else if upper == "END:VCARD" {
            flush(&mut card, &mut contacts, &mut skipped);
            in_card = false;
        } else if in_card && let Some((name, params, value)) = split_vcard_line(line) {
            card.push((name, params, value));
        }
    }
    flush(&mut card, &mut contacts, &mut skipped);
    // Over-limit pastes are truncated here so no caller can silently
    // drop cards — the dropped count rides back so the dialog reports
    // it honestly.
    let truncated = contacts.len().saturating_sub(VCARD_IMPORT_LIMIT);
    if contacts.len() > VCARD_IMPORT_LIMIT {
        contacts.truncate(VCARD_IMPORT_LIMIT);
    }
    (contacts, skipped, truncated)
}

/// Split one unfolded vCard content line into
/// (UPPERCASE-NAME, params, unescaped value).
fn split_vcard_line(line: &str) -> Option<(String, Vec<String>, String)> {
    let (head, value) = line.split_once(':')?;
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params: Vec<String> = parts.map(|p| p.trim().to_ascii_uppercase()).collect();
    Some((name, params, unescape_vcard_value(value.trim())))
}

fn unescape_vcard_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(next) = chars.next()
        {
            match next {
                'n' | 'N' => out.push('\n'),
                other => out.push(other),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Keep the dialable characters: an optional leading `+` plus digits.
fn normalize_phone(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();
    // A `+` is only meaningful as the first character.
    let mut cleaned = String::with_capacity(out.len());
    for (i, c) in out.chars().enumerate() {
        if c != '+' || i == 0 {
            cleaned.push(c);
        }
    }
    out = cleaned;
    out
}

fn parse_vcard_block(props: &[(String, Vec<String>, String)]) -> Vec<ImportedContact> {
    let mut first_name = String::new();
    let mut last_name = String::new();
    let mut note = String::new();
    let mut tels: Vec<(Vec<String>, String)> = Vec::new();
    for (name, params, value) in props {
        match name.as_str() {
            "FN" => {
                // `N` wins when both are present.
                if first_name.is_empty() && last_name.is_empty() && !value.is_empty() {
                    match value.split_once(' ') {
                        Some((first, rest)) => {
                            first_name = first.to_string();
                            last_name = rest.trim().to_string();
                        }
                        None => first_name = value.clone(),
                    }
                }
            }
            "N" => {
                let mut parts = value.split(';');
                let family = parts.next().unwrap_or("").trim();
                let given = parts.next().unwrap_or("").trim();
                if !given.is_empty() || !family.is_empty() {
                    first_name = given.to_string();
                    last_name = family.to_string();
                }
            }
            "NOTE" => note = value.clone(),
            "TEL" => {
                let phone = normalize_phone(value);
                if !phone.is_empty() {
                    // vCard 2.1 style (`TEL;CELL:…`) and 3.0/4.0 style
                    // (`TEL;TYPE=CELL:…`); types may be comma-separated.
                    let mut types: Vec<String> = Vec::new();
                    for param in params {
                        let param = param.strip_prefix("TYPE=").unwrap_or(param);
                        types.extend(param.split(',').map(str::to_string));
                    }
                    tels.push((types, phone));
                }
            }
            _ => {}
        }
    }
    if tels.is_empty() {
        return Vec::new();
    }
    // One contact per number; CELL first, then VOICE, then the rest —
    // mirroring TGX importing every device number.
    tels.sort_by_key(|(types, _)| {
        if types.iter().any(|t| t == "CELL") {
            0
        } else if types.iter().any(|t| t == "VOICE") {
            1
        } else {
            2
        }
    });
    tels.into_iter()
        .map(|(_, phone)| ImportedContact {
            phone_number: phone,
            first_name: first_name.clone(),
            last_name: last_name.clone(),
            note: note.clone(),
        })
        .collect()
}

/// Slice A6: `importContacts` (TDLib 1.8.67, `schema/td_api.tl:14517`):
/// `importContacts contacts:vector<importedContact> = ImportedContacts;`
/// The `note` rides an entity-less `formattedText` (same shape as
/// `addContact`'s, requests.rs:695).
pub fn import_contacts(extra: RequestId, contacts: &[ImportedContact]) -> String {
    json!({
        "@type": "importContacts",
        "@extra": extra.as_extra(),
        "contacts": contacts
            .iter()
            .map(|c| json!({
                "@type": "importedContact",
                "phone_number": c.phone_number,
                "first_name": c.first_name,
                "last_name": c.last_name,
                "note": { "@type": "formattedText", "text": c.note, "entities": [] },
            }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

//! Pass one of a chat's export: page the history newest first, keep the
//! messages inside the date range (and only the viewer's own when asked),
//! resolve the names the pages print, and spool each page to a small file
//! so a long chat never sits in memory.
use super::model::{Body, ExportMessage, Origin, Who, parse_message};
use super::worker::{ExportError, Job, Progress, QUERY_TIMEOUT, Tdlib, query};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// `getChatHistory` page size (TDLib's maximum).
pub const PAGE_LIMIT: i64 = 100;

/// Display names, fetched once per user or chat.
#[derive(Default)]
pub struct Names {
    known: HashMap<Who, String>,
}

/// "First Last" of a TDLib `user`, or tdesktop's "Deleted Account".
pub fn user_name(user: &Value) -> String {
    let first = user["first_name"].as_str().unwrap_or_default();
    let last = user["last_name"].as_str().unwrap_or_default();
    let full = format!("{first} {last}").trim().to_string();
    if full.is_empty() {
        "Deleted Account".to_string()
    } else {
        full
    }
}

impl Names {
    pub fn name<T: Tdlib>(&mut self, tdlib: &mut T, who: Who) -> Result<String, ExportError> {
        if let Some(name) = self.known.get(&who) {
            return Ok(name.clone());
        }
        let request = match who {
            Who::User(id) => json!({"@type": "getUser", "user_id": id}),
            Who::Chat(id) => json!({"@type": "getChat", "chat_id": id}),
        };
        let name = match tdlib.call(request, QUERY_TIMEOUT)? {
            answer if answer["@type"] == "error" => "Unknown".to_string(),
            answer if matches!(who, Who::Chat(_)) => {
                answer["title"].as_str().unwrap_or("Unknown").to_string()
            }
            answer => user_name(&answer),
        };
        self.known.insert(who, name.clone());
        Ok(name)
    }

    fn fill<T: Tdlib>(
        &mut self,
        tdlib: &mut T,
        msg: &mut ExportMessage,
    ) -> Result<(), ExportError> {
        if let Some(who) = msg.sender {
            msg.from = Some(self.name(tdlib, who)?);
        }
        if let Some(Origin::Who(who)) = msg.forward {
            msg.forwarded_from = Some(self.name(tdlib, who)?);
        }
        if let Body::Service { users, names, .. } = &mut msg.body {
            let list = users.clone();
            for id in list {
                names.push(self.name(tdlib, Who::User(id))?);
            }
        }
        Ok(())
    }
}

pub fn spool_name(index: usize) -> String {
    format!("{index:08}.json")
}

/// One page of history: the raw messages and where the next page starts
/// (0 when there is none).
fn next_page<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    from: i64,
) -> Result<(Vec<Value>, Option<i64>), ExportError> {
    let settings = job.settings;
    match job.me.filter(|_| settings.only_mine) {
        // Only the viewer's own messages: let TDLib search by sender
        // instead of paging every message of a big group.
        Some(me) => {
            let page = query(
                tdlib,
                json!({"@type": "searchChatMessages", "chat_id": settings.chat_id,
                    "topic_id": null, "query": "",
                    "sender_id": {"@type": "messageSenderUser", "user_id": me},
                    "from_message_id": from, "offset": 0, "limit": PAGE_LIMIT, "filter": null}),
            )?;
            let next = page["next_from_message_id"].as_i64().filter(|id| *id > 0);
            Ok((
                page["messages"].as_array().cloned().unwrap_or_default(),
                next,
            ))
        }
        None => {
            let page = query(
                tdlib,
                json!({"@type": "getChatHistory", "chat_id": settings.chat_id, "from_message_id": from,
                    "offset": 0, "limit": PAGE_LIMIT, "only_local": false}),
            )?;
            // The cursor is the oldest id seen; the caller works it out.
            Ok((
                page["messages"].as_array().cloned().unwrap_or_default(),
                None,
            ))
        }
    }
}

/// Pass one. Returns the ids of the kept messages, newest first, and the
/// number of spool chunks written to `spool`.
pub fn collect<T: Tdlib>(
    job: &Job<'_>,
    tdlib: &mut T,
    spool: &Path,
    progress: &mut Progress,
) -> Result<(Vec<i64>, usize), ExportError> {
    let settings = job.settings;
    let searching = settings.only_mine && job.me.is_some();
    let mut names = Names::default();
    // Start at the last message before the range's end date.
    let mut from = 0;
    if settings.till > 0 {
        let date = (settings.till - 1).clamp(0, i64::from(i32::MAX));
        match tdlib.call(
            json!({"@type": "getChatMessageByDate", "chat_id": settings.chat_id, "date": date}),
            QUERY_TIMEOUT,
        )? {
            answer if answer["@type"] == "error" => {
                // Nothing at or before that date.
                return Ok((Vec::new(), 0));
            }
            // A search page starts after `from_message_id`; history pages
            // include it, so a search starts one id later.
            answer => from = answer["id"].as_i64().unwrap_or(0) + i64::from(searching),
        }
    }
    let mut oldest = 0i64;
    let mut ids = Vec::new();
    let mut chunks = 0;
    loop {
        let (raw, next) = next_page(job, tdlib, from)?;
        // TDLib may answer with fewer messages than asked; only an empty
        // page (or one with nothing new) ends the history.
        let mut kept = Vec::new();
        let mut progressed = false;
        let mut past_range = false;
        for value in &raw {
            let Some(id) = value["id"].as_i64() else {
                continue;
            };
            if oldest != 0 && id >= oldest {
                continue;
            }
            oldest = id;
            progressed = true;
            let Some(mut msg) = parse_message(value) else {
                continue;
            };
            if settings.before_range(msg.date) {
                past_range = true;
                continue;
            }
            if settings.skips_date(msg.date) || (settings.only_mine && !msg.outgoing) {
                continue;
            }
            names.fill(tdlib, &mut msg)?;
            kept.push(msg);
        }
        if !kept.is_empty() {
            ids.extend(kept.iter().map(|m| m.id));
            fs::write(spool.join(spool_name(chunks)), serde_json::to_vec(&kept)?)?;
            chunks += 1;
            progress.done = ids.len();
            (job.report)(progress.clone());
        }
        if !progressed || past_range {
            break;
        }
        from = match (searching, next) {
            (true, Some(next)) => next,
            (true, None) => break,
            (false, _) => oldest,
        };
    }
    Ok((ids, chunks))
}

/// One spooled page, oldest first.
pub fn read_chunk(spool: &Path, index: usize) -> Result<Vec<ExportMessage>, ExportError> {
    let bytes = fs::read(spool.join(spool_name(index)))?;
    let mut messages: Vec<ExportMessage> = serde_json::from_slice(&bytes)?;
    messages.reverse();
    Ok(messages)
}

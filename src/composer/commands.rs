//! Bot command menu, inline-query and @mention triggers.

use super::*;
use std::ops::Range;

/// A `@` or `/` token being typed: `range` runs from the trigger character
/// to the caret, `query` is the text after the trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerToken<'a> {
    pub range: Range<usize>,
    pub query: &'a str,
}

/// Longest `@name` query tdesktop scans back over.
const MENTION_QUERY_MAX: usize = 63;
/// Longest command name and `@bot` suffix tdesktop matches.
const COMMAND_NAME_MAX: usize = 64;
const COMMAND_BOT_MAX: usize = 32;

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The `@name` being typed at `caret` (tdesktop
/// `ParseMentionHashtagBotCommandQuery`): letters, digits and `_` back to
/// an `@` that does not follow a name character. Only the text before the
/// caret counts, so editing in the middle of a message works too.
/// `Some("")` right after a bare `@`.
pub fn mention_trigger(text: &str, caret: usize) -> Option<TriggerToken<'_>> {
    if !text.is_char_boundary(caret) {
        return None;
    }
    let head = text.get(..caret)?;
    let mut count = 0;
    for (at, c) in head.char_indices().rev() {
        if c == '@' {
            let starts_word = head
                .get(..at)?
                .chars()
                .next_back()
                .is_none_or(|before| !is_name_char(before));
            let query = head.get(at + 1..).unwrap_or_default();
            // tdesktop wants a letter or digit first: `@_` is not a mention.
            let valid_start = query.chars().next().is_none_or(char::is_alphanumeric);
            return (starts_word && valid_start).then_some(TriggerToken {
                range: at..caret,
                query,
            });
        }
        count += 1;
        if !is_name_char(c) || count > MENTION_QUERY_MAX {
            return None;
        }
    }
    None
}

/// Replace the `@query` before `caret` with a mention of the chosen user:
/// their `@username` when they have one, else a `tg://user?id=` link around
/// the name (TDLib turns it into a mention-name entity). Ends with a space.
/// Returns the new text and the caret after the mention.
pub fn complete_mention(
    text: &str,
    caret: usize,
    user_id: i64,
    username: &str,
    name: &str,
) -> Option<(String, usize)> {
    let token = mention_trigger(text, caret)?;
    let head = text.get(..token.range.start)?;
    let tail = text.get(token.range.end..)?;
    let mention = if username.is_empty() {
        let name = name.replace(['[', ']'], "");
        format!("[{name}](tg://user?id={user_id}) ")
    } else {
        format!("@{username} ")
    };
    let caret = head.len() + mention.len();
    Some((format!("{head}{mention}{tail}"), caret))
}

/// tdesktop `HistoryWidget::insertBotCommand`: the composer text after
/// choosing a bot command from a panel. The command goes first, then a
/// space, then what was typed. A command already leading the field is
/// replaced instead of stacked. The panels are in a bot's own chat, which
/// never adds `@bot`; a clicked command link in a group message adds it in
/// `send_bot_command`.
pub fn insert_bot_command_text(current: &str, command: &str) -> String {
    let mut insertion = if command.starts_with('/') {
        command.to_string()
    } else {
        format!("/{command}")
    };
    insertion.push(' ');
    insertion.push_str(strip_leading_command(current));
    insertion
}

/// `current` without a leading `/command` or `/command@bot` token and the
/// one whitespace character after it. A token longer than tdesktop's limits
/// (64 and 32 characters) is not a command.
fn strip_leading_command(current: &str) -> &str {
    let Some(rest) = current.strip_prefix('/') else {
        return current;
    };
    let word_len = |s: &str| {
        s.char_indices()
            .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_')
            .map_or(s.len(), |(at, _)| at)
    };
    let name_len = word_len(rest);
    if name_len > COMMAND_NAME_MAX {
        return current;
    }
    let after_name = rest.get(name_len..).unwrap_or_default();
    let after = match after_name.strip_prefix('@') {
        Some(bot) => {
            let bot_len = word_len(bot);
            if bot_len > COMMAND_BOT_MAX {
                return current;
            }
            bot.get(bot_len..).unwrap_or_default()
        }
        None => after_name,
    };
    let mut chars = after.chars();
    match chars.next() {
        None => "",
        Some(c) if c.is_whitespace() => chars.as_str(),
        Some(_) => current,
    }
}

/// What replaces the `range` token (the `/` and what was typed after it)
/// when a `/` menu row is chosen: `command` and a space. A whitespace
/// character already after the token is reused instead. Also returns the
/// caret, in the new text, after that space, so the menu does not reopen.
pub fn complete_command_at(
    text: &str,
    range: Range<usize>,
    command: &str,
) -> Option<(String, usize)> {
    let head = text.get(..range.start)?;
    let tail = text.get(range.end..)?;
    let existing = tail.chars().next().filter(|c| c.is_whitespace());
    let insert = if existing.is_some() {
        command.to_string()
    } else {
        format!("{command} ")
    };
    let caret = head.len() + command.len() + existing.map_or(1, char::len_utf8);
    Some((insert, caret))
}

/// The bot behind a `/` menu row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandBot {
    pub user_id: i64,
    pub username: String,
    pub name: String,
    /// True when the row comes from a group: choosing it inserts
    /// `/command@bot`, because several bots may share a command name.
    pub in_group: bool,
}

/// Phase 3.3: one row in the composer `/` command menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandMenuItem {
    /// Command name without the leading `/`.
    pub command: String,
    pub description: String,
    /// True when the row comes from `getCommands` (global/default scope)
    /// rather than the bot's `botInfo`; rendered in the "Global" section
    /// below the bot-specific commands.
    pub global: bool,
    /// True when the command is ephemeral (schema 1.8.67 `botCommand`,
    /// line 826) — the row shows the ephemeral icon; the result is only
    /// visible to the sender.
    pub is_ephemeral: bool,
    /// A quick reply shortcut (Telegram Business): choosing the row sends
    /// the shortcut's messages instead of inserting the command.
    pub quick_reply: Option<i32>,
    /// The bot the command belongs to; `None` for quick replies and the
    /// edit row.
    pub bot: Option<CommandBot>,
}

impl CommandMenuItem {
    /// `/command`, with `@bot` appended when `with_username` and the bot's
    /// username is known (tdesktop shows and sends the same string).
    pub fn command_text(&self, with_username: bool) -> String {
        let name = self.command.trim_start_matches('/');
        match self.bot.as_ref().filter(|bot| !bot.username.is_empty()) {
            Some(bot) if with_username => format!("/{name}@{}", bot.username),
            _ => format!("/{name}"),
        }
    }
}

/// Whether the rows show `/command@bot` (tdesktop `commandsWithUsername`):
/// the chat is a group with bots, or the typed filter already has an `@`.
pub fn commands_with_username(items: &[CommandMenuItem], prefix: &str) -> bool {
    prefix.find('@').is_some_and(|at| at > 0)
        || items
            .iter()
            .any(|item| item.bot.as_ref().is_some_and(|bot| bot.in_group))
}

/// `/` command-menu trigger at `caret` (tdesktop
/// `ParseMentionHashtagBotCommandQuery`): the text before the caret is a
/// `/` that opens the field, then a command name of letters, digits and
/// `_`, optionally `@` and a bot name. `query` is what follows the `/`.
/// `None` for a `/` anywhere else (`a/b`, `http://…`, after other text),
/// for a second `/`, and once whitespace follows the command.
pub fn command_menu_trigger(text: &str, caret: usize) -> Option<TriggerToken<'_>> {
    if !text.is_char_boundary(caret) {
        return None;
    }
    let query = text.get(..caret)?.strip_prefix('/')?;
    let (name, bot) = match query.split_once('@') {
        Some((name, bot)) if !name.is_empty() => (name, Some(bot)),
        Some(_) => return None,
        None => (query, None),
    };
    let valid = name.chars().all(is_name_char)
        && name.chars().count() <= COMMAND_NAME_MAX
        && bot.is_none_or(|bot| {
            bot.chars().all(is_name_char) && bot.chars().count() <= COMMAND_BOT_MAX
        });
    valid.then_some(TriggerToken {
        range: 0..caret,
        query,
    })
}

/// Bots slice: `@botname query` inline-mode trigger. Returns
/// `(username, query)` when the composer text starts with an `@`-led
/// token — TGX `InlineSearchContext` only runs inline lookup from the
/// message-composer start (`startIndex == 0`), never mid-text or in
/// captions. `@bot` → `("bot", "")`; `@bot cats` → `("bot", "cats")`.
/// `None` for a non-leading `@` (`hi @bot`), a bare `@`, or an empty
/// token. Usernames are ASCII alphanumeric + underscore; the query is
/// everything after the token's first whitespace run.
pub fn inline_query_trigger(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix('@')?;
    let username_len = rest
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    let query = rest.get(username_len..).unwrap_or_default().trim_start();
    Some((rest.get(..username_len).unwrap_or_default(), query))
}

/// Phase 3.3: merge `botInfo.commands` (bot-specific) with `getCommands`
/// results (global/default scope) into menu rows. Bot-specific rows come
/// first; global rows follow, skipping command names already listed, so a
/// command defined in both scopes appears once (the bot-specific
/// description wins).
pub fn merge_command_menu_items(
    specific: &[BotCommand],
    global: &[BotCommand],
) -> Vec<CommandMenuItem> {
    let mut items: Vec<CommandMenuItem> = specific
        .iter()
        .map(|command| CommandMenuItem {
            command: command.command.clone(),
            description: command.description.clone(),
            global: false,
            is_ephemeral: command.is_ephemeral,
            quick_reply: None,
            bot: None,
        })
        .collect();
    for command in global {
        if items.iter().any(|item| item.command == command.command) {
            continue;
        }
        items.push(CommandMenuItem {
            command: command.command.clone(),
            description: command.description.clone(),
            global: true,
            is_ephemeral: command.is_ephemeral,
            quick_reply: None,
            bot: None,
        });
    }
    items
}

/// The menu rows of every bot in a group, in the order given (tdesktop
/// lists the bots that wrote last first, then the rest).
pub fn group_command_menu_items(bots: &[(CommandBot, Vec<BotCommand>)]) -> Vec<CommandMenuItem> {
    bots.iter()
        .flat_map(|(bot, commands)| {
            commands.iter().map(|command| CommandMenuItem {
                command: command.command.clone(),
                description: command.description.clone(),
                global: false,
                is_ephemeral: command.is_ephemeral,
                quick_reply: None,
                bot: Some(bot.clone()),
            })
        })
        .collect()
}

/// Phase 3.3: filter menu rows by the typed prefix (case-insensitive; bot
/// commands are lowercase `a-z0-9_` but users may type capitals). An empty
/// prefix matches everything. When the rows carry `@bot` (see
/// [`commands_with_username`]) the prefix is matched against
/// `command@bot`, so `/start@my` finds one bot's `start` among several.
pub fn filter_command_menu_items<'a>(
    items: &'a [CommandMenuItem],
    prefix: &str,
) -> Vec<&'a CommandMenuItem> {
    let prefix = prefix.to_lowercase();
    let with_username = commands_with_username(items, &prefix);
    items
        .iter()
        .filter(|item| {
            item.command_text(with_username)
                .trim_start_matches('/')
                .to_lowercase()
                .starts_with(&prefix)
        })
        .collect()
}

/// Phase 3.2: composer text after tapping a `switchInline` keyboard button.
/// The button's query is inserted — bare when the field is empty, otherwise
/// appended after a space (or directly when the field already ends in
/// whitespace). `targetChatChosen` / `targetChatInternalLink` (no chat
/// picker in this slice) use the current chat, same as `targetChatCurrent`.
pub fn insert_switch_inline_text(current: &str, query: &str) -> String {
    if current.trim().is_empty() {
        query.to_string()
    } else if current.ends_with(char::is_whitespace) {
        format!("{current}{query}")
    } else {
        format!("{current} {query}")
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

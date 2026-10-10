//! Bot command menu, inline-query and @mention triggers.

use super::*;

/// The `@name` being typed at the end of the composer: the text after an
/// `@` that starts a word, made of letters, digits and `_` only. `Some("")`
/// right after a bare `@`; `None` once a space or other character follows.
pub fn mention_trigger(text: &str) -> Option<&str> {
    let at = text.rfind('@')?;
    let query = &text[at + 1..];
    let starts_word = text[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    let name_chars = query.chars().all(|c| c.is_alphanumeric() || c == '_');
    (starts_word && name_chars && query.chars().count() <= 32).then_some(query)
}

/// Replace the trailing `@query` with a mention of the chosen user: their
/// `@username` when they have one, else a `tg://user?id=` link around the
/// name (TDLib turns it into a mention-name entity). Ends with a space.
pub fn complete_mention(text: &str, user_id: i64, username: &str, name: &str) -> String {
    let Some(query) = mention_trigger(text) else {
        return text.to_string();
    };
    let head = &text[..text.len() - query.len() - 1];
    if username.is_empty() {
        let name = name.replace(['[', ']'], "");
        format!("{head}[{name}](tg://user?id={user_id}) ")
    } else {
        format!("{head}@{username} ")
    }
}

/// Phase 3.1: composer text after tapping a bot command. Empty field → the
/// bare `/command`; otherwise appended after a space (or directly when the
/// field already ends in whitespace).
pub fn insert_bot_command_text(current: &str, command: &str) -> String {
    let insertion = format!("/{command}");
    if current.trim().is_empty() {
        insertion
    } else if current.ends_with(char::is_whitespace) {
        format!("{current}{insertion}")
    } else {
        format!("{current} {insertion}")
    }
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
}

/// Phase 3.3: `/` command-menu trigger. Returns the filter prefix typed
/// after the `/` when the composer text ends with a `/`-led token at a
/// word boundary — start of text or right after whitespace — e.g.
/// `Some("")` for a bare `/`, `Some("st")` for `/st`. Returns `None` for
/// mid-word slashes (`a/b`, `http://…`), so the menu never opens inside
/// words or URLs. The trailing-token convention matches the input's lack
/// of an exposed cursor offset (documented in DECISIONS).
pub fn command_menu_trigger(text: &str) -> Option<&str> {
    let token = text.split(char::is_whitespace).next_back()?;
    let prefix = token.strip_prefix('/')?;
    // `/` alone or `/` + command chars; a second `/` (`/a/b`, `//`) is not
    // a command token.
    if prefix.contains('/') {
        return None;
    }
    Some(prefix)
}

/// Phase 3.3: composer text with the trailing `/`-token removed, for menu
/// picks. The menu replaces the partial token the user typed, so `/st` +
/// pick `start` → `/start`, not `/st /start`. Returns `None` when there is
/// no trigger token (the caller should not pick).
pub fn strip_command_menu_trigger(text: &str) -> Option<&str> {
    let prefix = command_menu_trigger(text)?;
    let token_len = prefix.len() + 1; // leading `/`
    text.get(..text.len() - token_len)
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
    let query = rest[username_len..].trim_start();
    Some((&rest[..username_len], query))
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
        });
    }
    items
}

/// Phase 3.3: filter menu rows by the typed prefix (case-insensitive; bot
/// commands are lowercase `a-z0-9_` but users may type capitals). An empty
/// prefix matches everything.
pub fn filter_command_menu_items<'a>(
    items: &'a [CommandMenuItem],
    prefix: &str,
) -> Vec<&'a CommandMenuItem> {
    let prefix = prefix.to_lowercase();
    items
        .iter()
        .filter(|item| item.command.to_lowercase().starts_with(&prefix))
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
mod tests {
    use super::*;

    #[test]
    fn mention_trigger_needs_a_word_start_and_name_chars() {
        assert_eq!(mention_trigger("hi @al"), Some("al"));
        assert_eq!(mention_trigger("@"), Some(""));
        assert_eq!(mention_trigger("mail@host"), None);
        assert_eq!(mention_trigger("hi @al "), None);
        assert_eq!(mention_trigger("hi @al-x"), None);
        assert_eq!(mention_trigger("no at"), None);
    }

    #[test]
    fn complete_mention_prefers_username_then_name_link() {
        assert_eq!(
            complete_mention("hi @al", 7, "alice", "Alice"),
            "hi @alice "
        );
        assert_eq!(
            complete_mention("hi @al", 7, "", "Al [x]"),
            "hi [Al x](tg://user?id=7) "
        );
        assert_eq!(complete_mention("plain", 7, "alice", "Alice"), "plain");
    }
    #[test]
    fn bot_command_insert_text() {
        // Phase 3.1: tapping a command chip composes the new composer text.
        assert_eq!(insert_bot_command_text("", "start"), "/start");
        assert_eq!(insert_bot_command_text("   ", "start"), "/start");
        assert_eq!(insert_bot_command_text("hello", "start"), "hello /start");
        assert_eq!(insert_bot_command_text("hello ", "start"), "hello /start");
        assert_eq!(insert_bot_command_text("/help", "start"), "/help /start");
    }

    #[test]
    fn switch_inline_insert_text() {
        // Phase 3.2: tapping a switchInline button inserts the query.
        assert_eq!(insert_switch_inline_text("", "pic"), "pic");
        assert_eq!(insert_switch_inline_text("   ", "pic"), "pic");
        assert_eq!(insert_switch_inline_text("hello", "pic"), "hello pic");
        assert_eq!(insert_switch_inline_text("hello ", "pic"), "hello pic");
    }

    // Phase 3.3: `/` command-menu trigger / merge / filter.
    fn bot_command(command: &str, description: &str) -> BotCommand {
        BotCommand {
            command: command.into(),
            description: description.into(),
            is_ephemeral: false,
        }
    }

    #[test]
    fn command_menu_trigger_matches_trailing_slash_token() {
        assert_eq!(command_menu_trigger("/"), Some(""));
        assert_eq!(command_menu_trigger("/st"), Some("st"));
        assert_eq!(command_menu_trigger("hello /st"), Some("st"));
        assert_eq!(command_menu_trigger("hi\n/he"), Some("he"));
        assert_eq!(command_menu_trigger("/ST"), Some("ST"));
    }

    #[test]
    fn command_menu_trigger_rejects_mid_word_slashes() {
        assert_eq!(command_menu_trigger(""), None);
        assert_eq!(command_menu_trigger("hello"), None);
        assert_eq!(command_menu_trigger("a/b"), None);
        assert_eq!(command_menu_trigger("hello a/b"), None);
        assert_eq!(command_menu_trigger("http://x"), None);
        assert_eq!(command_menu_trigger("/a/b"), None);
        assert_eq!(command_menu_trigger("//"), None);
        // Trailing whitespace ends the token: the menu closes once the
        // user commits the token with a space.
        assert_eq!(command_menu_trigger("/start "), None);
        assert_eq!(command_menu_trigger("hello /st "), None);
    }

    #[test]
    fn inline_query_trigger_parses_leading_at_token() {
        assert_eq!(inline_query_trigger("@bot"), Some(("bot", "")));
        assert_eq!(inline_query_trigger("@bot "), Some(("bot", "")));
        assert_eq!(inline_query_trigger("@bot cats"), Some(("bot", "cats")));
        assert_eq!(
            inline_query_trigger("@gif_bot_1 cute cats"),
            Some(("gif_bot_1", "cute cats"))
        );
        // Token ends at the first non-username char; the rest is query.
        assert_eq!(inline_query_trigger("@bot!"), Some(("bot", "!")));
    }

    #[test]
    fn inline_query_trigger_rejects_non_leading_at() {
        assert_eq!(inline_query_trigger(""), None);
        assert_eq!(inline_query_trigger("@"), None);
        assert_eq!(inline_query_trigger("hi @bot"), None);
        assert_eq!(inline_query_trigger(" @bot"), None);
        assert_eq!(inline_query_trigger("@@bot"), None);
    }

    #[test]
    fn strip_command_menu_trigger_removes_trailing_token() {
        assert_eq!(strip_command_menu_trigger("/"), Some(""));
        assert_eq!(strip_command_menu_trigger("/st"), Some(""));
        assert_eq!(strip_command_menu_trigger("hello /st"), Some("hello "));
        assert_eq!(strip_command_menu_trigger("hi\n/he"), Some("hi\n"));
        assert_eq!(strip_command_menu_trigger("a/b"), None);
        assert_eq!(strip_command_menu_trigger("/start "), None);
    }

    #[test]
    fn merge_command_menu_items_prefers_specific_descriptions() {
        let specific = vec![bot_command("start", "Start the bot")];
        let global = vec![
            bot_command("start", "Global start"),
            bot_command("settings", "Tweak the bot"),
        ];
        let items = merge_command_menu_items(&specific, &global);
        assert_eq!(items.len(), 2);
        // Bot-specific rows first; the duplicate keeps the bot-specific
        // description.
        assert_eq!(
            items[0],
            CommandMenuItem {
                command: "start".into(),
                description: "Start the bot".into(),
                global: false,
                is_ephemeral: false,
            }
        );
        assert_eq!(
            items[1],
            CommandMenuItem {
                command: "settings".into(),
                description: "Tweak the bot".into(),
                global: true,
                is_ephemeral: false,
            }
        );
    }

    #[test]
    fn merge_command_menu_items_empty_sides() {
        assert!(merge_command_menu_items(&[], &[]).is_empty());
        let items = merge_command_menu_items(&[bot_command("start", "")], &[]);
        assert_eq!(items.len(), 1);
        assert!(!items[0].global);
    }

    #[test]
    fn merge_command_menu_items_keeps_ephemeral_flag() {
        // The `is_ephemeral` flag is what the menu icon reads — it must
        // survive the merge from both the bot-specific and the global
        // side.
        let mut secret = bot_command("secret", "Only you see this");
        secret.is_ephemeral = true;
        let mut gsettings = bot_command("gsettings", "Global ephemeral");
        gsettings.is_ephemeral = true;
        let items = merge_command_menu_items(&[secret, bot_command("start", "")], &[gsettings]);
        let secret_item = items.iter().find(|i| i.command == "secret").unwrap();
        assert!(secret_item.is_ephemeral);
        assert!(!secret_item.global);
        let global_item = items.iter().find(|i| i.command == "gsettings").unwrap();
        assert!(global_item.is_ephemeral);
        assert!(global_item.global);
        assert!(
            !items
                .iter()
                .find(|i| i.command == "start")
                .unwrap()
                .is_ephemeral
        );
    }

    #[test]
    fn filter_command_menu_items_matches_prefix_case_insensitively() {
        let items = vec![
            CommandMenuItem {
                command: "start".into(),
                description: String::new(),
                global: false,
                is_ephemeral: false,
            },
            CommandMenuItem {
                command: "settings".into(),
                description: String::new(),
                global: true,
                is_ephemeral: false,
            },
            CommandMenuItem {
                command: "help".into(),
                description: String::new(),
                global: false,
                is_ephemeral: false,
            },
        ];
        let all: Vec<&str> = filter_command_menu_items(&items, "")
            .iter()
            .map(|item| item.command.as_str())
            .collect();
        assert_eq!(all, vec!["start", "settings", "help"]);
        let st: Vec<&str> = filter_command_menu_items(&items, "st")
            .iter()
            .map(|item| item.command.as_str())
            .collect();
        assert_eq!(st, vec!["start"]);
        // Capitalized input still matches lowercase commands.
        let caps: Vec<&str> = filter_command_menu_items(&items, "ST")
            .iter()
            .map(|item| item.command.as_str())
            .collect();
        assert_eq!(caps, vec!["start"]);
        assert!(filter_command_menu_items(&items, "zzz").is_empty());
    }
}

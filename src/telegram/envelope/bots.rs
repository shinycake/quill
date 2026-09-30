use serde_json::Value;

/// `botCommand` (TDLib 1.8.67, `schema/td_api.tl:826`):
/// `botCommand command:string description:string is_ephemeral:Bool =
/// BotCommand`. `is_ephemeral` is not kept — the panel only lists commands;
/// tapping one inserts plain text into the composer (Phase 3.3 owns the
/// command menu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

/// `botInfo` subset (TDLib 1.8.67, `schema/td_api.tl:2430`): only what the
/// bot panel renders — `short_description`, `description`,
/// `commands:vector<botCommand>` (a bare vector of `botCommand`, not the
/// `botCommands` wrapper), plus the B2 additions `menu_button`
/// (`botMenuButton`, line 834) and `privacy_policy_url` (line 2414).
/// Photo, rights, and the edit links are intentionally not kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BotInfo {
    pub short_description: String,
    pub description: String,
    pub commands: Vec<BotCommand>,
    /// Slice B2: `botInfo.menu_button` — `(text, url)`. Absent/null in the
    /// payload → `None` (bots without a menu button, or older payloads).
    pub menu_button: Option<BotMenuButton>,
    /// Slice B2: `botInfo.privacy_policy_url` — the HTTP link to the bot's
    /// privacy policy (schema line 2414); empty when the bot published none.
    pub privacy_policy_url: String,
}

/// Slice B2: `botMenuButton text:string url:string = BotMenuButton;`
/// (schema 1.8.67 line 834).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BotMenuButton {
    pub text: String,
    pub url: String,
}

/// `botCommand` (schema 1.8.67 line 826). A command without `command` text
/// is dropped; the description may be empty.
pub(crate) fn parse_bot_command(value: Option<&Value>) -> Option<BotCommand> {
    let value = value.filter(|v| !v.is_null())?;
    Some(BotCommand {
        command: value.get("command")?.as_str()?.to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// `botInfo` subset (schema 1.8.67 line 2430). `bot_info` arrives as null
/// for non-bots (the schema comment says "may be null if the user isn't a
/// bot"), so null → `None` rather than an empty struct.
pub(crate) fn parse_bot_info(value: Option<&Value>) -> Option<BotInfo> {
    let value = value.filter(|v| !v.is_null())?;
    Some(BotInfo {
        short_description: value
            .get("short_description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        commands: value
            .get("commands")
            .and_then(Value::as_array)
            .map(|commands| {
                commands
                    .iter()
                    .filter_map(|v| parse_bot_command(Some(v)))
                    .collect()
            })
            .unwrap_or_default(),
        menu_button: value
            .get("menu_button")
            .filter(|v| !v.is_null())
            .map(|v| BotMenuButton {
                text: v
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                url: v
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
        privacy_policy_url: value
            .get("privacy_policy_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

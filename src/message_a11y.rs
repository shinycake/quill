//! What a screen reader says for one message in the history, after Telegram
//! Desktop's `HistoryView::MessageAccessibilityName`
//! (`history/history_inner_widget_accessibility.cpp`): one line each for the
//! seen state, the sender, the bot, the reply, the forward origin, the media,
//! the link preview, the text, the fact check and "Pinned", then the delivery
//! status with the time, then the reactions.
#![deny(clippy::indexing_slicing)]

use crate::ids::FileId;
use crate::state::{HistoryMessage, OutboxReceipt};
use crate::telegram::envelope::{MessageContent, ParsedFile};
use crate::telegram::envelope::{PollType, ReactionType};

/// What the name needs beyond the message itself, resolved by the caller.
#[derive(Clone, Debug, Default)]
pub struct MessageA11yContext {
    /// The incoming message's sender (`displayFrom()->name()`).
    pub sender_name: Option<String>,
    /// Outgoing: the peer has read past this message.
    pub receipt: Option<OutboxReceipt>,
    /// The inline bot's username, without the "@".
    pub via_bot: Option<String>,
    /// The resolved reply: the replied sender (`None` reads "Me") and its
    /// one-line text.
    pub reply: Option<(Option<String>, String)>,
    /// The forward origin's name.
    pub forwarded_from: Option<String>,
    /// The message time, `HH:MM` local.
    pub time: Option<String>,
}

/// The message's accessible name. `file` looks up a TDLib file the message
/// carries, for its download state and size.
pub fn message_accessibility_name(
    message: &HistoryMessage,
    context: &MessageA11yContext,
    file: &dyn Fn(FileId) -> Option<ParsedFile>,
) -> String {
    if let Some(text) = service_text(&message.content) {
        return text;
    }
    let mut lines: Vec<String> = Vec::new();
    if message.is_outgoing {
        lines.push(
            if context.receipt == Some(OutboxReceipt::Read) {
                "Seen"
            } else {
                "Not seen"
            }
            .to_string(),
        );
        lines.push("Me".to_string());
    } else if let Some(name) = context.sender_name.as_ref().filter(|name| !name.is_empty()) {
        lines.push(name.clone());
    }
    if let Some(bot) = context.via_bot.as_ref().filter(|bot| !bot.is_empty()) {
        lines.push(format!("via @{bot}"));
    }
    if let Some((name, text)) = &context.reply {
        let name = name.as_deref().filter(|name| !name.is_empty());
        if name.is_some() || !text.is_empty() {
            lines.push(format!("In reply to {}: {text}", name.unwrap_or("Me")));
        }
    }
    if let Some(origin) = &context.forwarded_from {
        lines.push(format!("Forwarded from {origin}"));
    }
    let (media, caption) = media_parts(message, file);
    if !media.is_empty() {
        lines.push(media.join(", "));
    }
    if let MessageContent::Text(text) = &message.content
        && let Some(preview) = &text.link_preview
    {
        let web: Vec<&str> = [
            preview.site_name.as_str(),
            preview.title.as_str(),
            preview.description.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
        if !web.is_empty() {
            lines.push(web.join(": "));
        }
    }
    if !caption.is_empty() {
        lines.push(caption);
    }
    if let Some(check) = &message.extras.fact_check
        && !check.text.is_empty()
    {
        lines.push(check.text.clone());
    }
    if message.is_pinned {
        lines.push("Pinned".to_string());
    }
    lines.push(status_line(message, context));
    if let Some(reactions) = reactions_line(message) {
        lines.push(reactions);
    }
    lines.join("\n")
}

/// Service messages read as their notification text.
fn service_text(content: &MessageContent) -> Option<String> {
    match content {
        MessageContent::Action(_)
        | MessageContent::ChatTtlChanged { .. }
        | MessageContent::ScreenshotTaken
        | MessageContent::ChatAddedToCommunity { .. }
        | MessageContent::ChatRemovedFromCommunity
        | MessageContent::ChatJoinFromCommunity { .. }
        | MessageContent::PaymentSuccessful(_)
        | MessageContent::PaymentReceived(_) => Some(content.preview()),
        _ => None,
    }
}

/// "Sent at 14:05", "Received edited at 9:30", with views and signature.
fn status_line(message: &HistoryMessage, context: &MessageA11yContext) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(
        if message.is_outgoing {
            if message.pending {
                "Sending"
            } else if message.failed {
                "Failed"
            } else {
                "Sent"
            }
        } else {
            "Received"
        }
        .to_string(),
    );
    if message.extras.edit_date > 0 {
        parts.push("edited".to_string());
    }
    if let Some(time) = &context.time {
        parts.push(format!("at {time}"));
    }
    if let Some(info) = &message.interaction_info
        && info.view_count > 0
    {
        parts.push(format!("{} Views", info.view_count));
    }
    if let Some(signature) = message.author_signature.as_ref().filter(|s| !s.is_empty()) {
        parts.push(signature.clone());
    }
    parts.join(" ")
}

/// "Reactions: 👍 2, ❤️". Custom and paid reactions have no emoji to read
/// and are left out, as in tdesktop when the sticker is unknown.
fn reactions_line(message: &HistoryMessage) -> Option<String> {
    let reactions = message.interaction_info.as_ref()?.reactions.as_ref()?;
    let parts: Vec<String> = reactions
        .reactions
        .iter()
        .filter_map(|reaction| match &reaction.reaction_type {
            ReactionType::Emoji { emoji } if !emoji.is_empty() => {
                Some(if reaction.total_count > 1 {
                    format!("{emoji} {}", reaction.total_count)
                } else {
                    emoji.clone()
                })
            }
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| format!("Reactions: {}", parts.join(", ")))
}

/// Where a document's file stands (`Downloading` / `Downloaded` /
/// `Not downloaded`) and its size.
fn file_parts(id: FileId, file: &dyn Fn(FileId) -> Option<ParsedFile>) -> (String, Option<i64>) {
    let parsed = file(id);
    let state = match &parsed {
        Some(parsed) if parsed.local.is_downloading_active => "Downloading",
        Some(parsed) if parsed.usable_path().is_some() => "Downloaded",
        _ => "Not downloaded",
    };
    let size = parsed
        .map(|parsed| parsed.display_size())
        .filter(|size| *size > 0);
    (state.to_string(), size)
}

/// The media line's parts and the caption (or the text of a text message).
fn media_parts(
    message: &HistoryMessage,
    file: &dyn Fn(FileId) -> Option<ParsedFile>,
) -> (Vec<String>, String) {
    let mut parts: Vec<String> = Vec::new();
    // A document: kind, state, then the details tdesktop lists in order.
    let document = |kind: &str,
                    file_id: FileId,
                    played: Option<bool>,
                    song: Option<(&str, &str)>,
                    name: Option<&str>,
                    duration: i32,
                    dimensions: Option<(i32, i32)>| {
        let mut out = vec![kind.to_string()];
        let (state, size) = file_parts(file_id, file);
        out.push(state);
        if let Some(played) = played {
            out.push(if played { "Played" } else { "Not played" }.to_string());
        }
        if let Some((performer, title)) = song {
            out.extend(
                [performer, title]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .map(str::to_string),
            );
        }
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            out.push(name.to_string());
        }
        if duration > 0 {
            out.push(format_duration_text(i64::from(duration)));
        }
        if let Some((width, height)) = dimensions.filter(|(w, h)| *w > 0 && *h > 0) {
            out.push(format!("{width}\u{d7}{height}"));
        }
        if let Some(size) = size {
            out.push(crate::telegram::envelope::format_size_text(size));
        }
        out
    };
    let caption = match &message.content {
        MessageContent::Text(text) => text.text.clone(),
        MessageContent::VoiceNote(note) => {
            parts = document(
                "Voice message",
                note.file_id,
                Some(note.is_listened),
                None,
                None,
                note.duration,
                None,
            );
            note.caption.clone()
        }
        MessageContent::VideoNote(note) => {
            parts = document(
                "Video message",
                note.file_id,
                Some(note.is_viewed),
                None,
                None,
                note.duration,
                None,
            );
            String::new()
        }
        MessageContent::Audio(audio) => {
            parts = document(
                "Audio file",
                audio.file_id,
                None,
                Some((&audio.performer, &audio.title)),
                None,
                audio.duration,
                None,
            );
            audio.caption.clone()
        }
        MessageContent::Video(video) => {
            parts = document(
                "Video",
                video.file_id,
                None,
                None,
                Some(&video.file_name),
                video.duration,
                Some((video.width, video.height)),
            );
            video.caption.clone()
        }
        MessageContent::Animation(animation) => {
            parts = document(
                "GIF",
                animation.file_id,
                None,
                None,
                Some(&animation.file_name),
                animation.duration,
                None,
            );
            animation.caption.clone()
        }
        MessageContent::Sticker(sticker) => {
            let mut out = vec!["Sticker".to_string()];
            if !sticker.emoji.is_empty() {
                out.push(sticker.emoji.clone());
            }
            let (state, size) = file_parts(sticker.file_id, file);
            out.push(state);
            if let Some(size) = size {
                out.push(crate::telegram::envelope::format_size_text(size));
            }
            parts = out;
            String::new()
        }
        MessageContent::Document(doc) => {
            parts = document(
                "File",
                doc.file_id,
                None,
                None,
                Some(&doc.file_name),
                0,
                None,
            );
            doc.caption.clone()
        }
        MessageContent::Photo(photo) => {
            parts.push("Photo".to_string());
            if photo.has_spoiler {
                parts.push("Hidden media".to_string());
            }
            let large = photo
                .sizes
                .iter()
                .max_by_key(|size| i64::from(size.width) * i64::from(size.height));
            if let Some(size) = large.filter(|size| size.width > 0 && size.height > 0) {
                parts.push(format!("{}\u{d7}{}", size.width, size.height));
            }
            photo.caption.clone()
        }
        MessageContent::Call {
            is_video,
            discard_reason,
            duration,
        } => {
            parts.push(crate::telegram::envelope::call_entry_label(
                *is_video,
                discard_reason,
                *duration,
                message.is_outgoing,
            ));
            if *duration > 0 {
                parts.push(format_duration_text(i64::from(*duration)));
            }
            String::new()
        }
        MessageContent::Poll(poll) => {
            let poll = &poll.poll;
            parts.push(
                if matches!(poll.poll_type, PollType::Quiz { .. }) {
                    "Quiz"
                } else {
                    "Poll"
                }
                .to_string(),
            );
            if !poll.question.is_empty() {
                parts.push(poll.question.clone());
            }
            if poll.is_closed {
                parts.push("Closed".to_string());
            }
            parts.push(match poll.total_voter_count {
                1 => "1 vote".to_string(),
                count => format!("{count} votes"),
            });
            String::new()
        }
        MessageContent::Contact(contact) => {
            let name = contact.display_name();
            if !name.is_empty() {
                parts.push(name);
            }
            if !contact.phone_number.is_empty() {
                parts.push(contact.phone_number.clone());
            }
            String::new()
        }
        MessageContent::Location(location) => {
            parts.push(
                if location.live.is_some() {
                    "Live Location"
                } else {
                    "Location"
                }
                .to_string(),
            );
            String::new()
        }
        MessageContent::Venue(venue) => {
            parts.push("Location".to_string());
            if !venue.title.is_empty() {
                parts.push(venue.title.clone());
            }
            String::new()
        }
        MessageContent::Game(game) => {
            if !game.title.is_empty() {
                parts.push(game.title.clone());
            }
            String::new()
        }
        MessageContent::Invoice(invoice) => {
            if !invoice.title.is_empty() {
                parts.push(invoice.title.clone());
            }
            parts.push(format!(
                "{} {}",
                invoice.currency,
                format_amount(invoice.total_amount)
            ));
            String::new()
        }
        MessageContent::Checklist(checklist) => {
            if !checklist.list.title.is_empty() {
                parts.push(checklist.list.title.clone());
            }
            String::new()
        }
        MessageContent::Dice(dice) => {
            parts.push(dice.emoji.clone());
            String::new()
        }
        other => {
            let text = other.preview();
            if !text.is_empty() {
                parts.push(text);
            }
            String::new()
        }
    };
    (parts, caption)
}

/// tdesktop `QString::number(amount / 100.0, 'f', 2)`.
fn format_amount(amount: i64) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// tdesktop `Ui::FormatDurationText`: "0:07", "12:34", "1:02:03".
pub fn format_duration_text(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes, secs) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

#[cfg(test)]
#[path = "message_a11y_tests.rs"]
mod tests;

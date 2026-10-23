//! Recent actions: how one entry looks. A day label when the day changes,
//! then the acting admin's avatar beside the service line and, where
//! tdesktop shows the affected message, that message in a bubble (an edit
//! carries the original text as a quote, like a reply).
#![deny(clippy::indexing_slicing)]

use super::history::service_pill;
use super::message_text::kit_avatar_element;
use gpui_kit::component::bubble::{Bubble, BubbleVariant};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::path::PathBuf;

/// Side of the acting admin's avatar.
const AVATAR: f32 = 32.;

/// The message a row shows under its service line.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum LogBody {
    None,
    Message(String),
    Edited { original: String, new: String },
}

/// One row of the log, ready to draw (no state is read while drawing).
#[derive(Debug, Clone)]
pub(super) struct LogEntry {
    pub id: i64,
    /// Set on the first row of a day.
    pub day: Option<String>,
    pub time: String,
    /// The service line: the actor's name, then what they did.
    pub line: String,
    /// Length of the actor's name at the start of `line` (bytes).
    pub actor_len: usize,
    pub actor: String,
    pub photo: Option<PathBuf>,
    pub body: LogBody,
}

pub(super) fn render_entry(entry: &LogEntry, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let bold = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let line = StyledText::new(entry.line.clone())
        .with_highlights(vec![(0..entry.actor_len.min(entry.line.len()), bold)]);
    let head = div()
        .flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .text_color(theme.foreground)
                .child(line),
        )
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(entry.time.clone()),
        );
    let mut column = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(head);
    if let Some(bubble) = body_bubble(&entry.body, cx) {
        column = column.child(bubble);
    }
    let mut row = div()
        .id(("event-log-row", entry.id as u64))
        .flex()
        .flex_col()
        .w_full()
        .px_1()
        .py_1p5();
    if let Some(day) = &entry.day {
        row = row.child(service_pill(
            ("event-log-day", entry.id as u64),
            day.clone(),
            cx,
        ));
    }
    row.child(
        div()
            .flex()
            .items_start()
            .gap_2()
            .child(div().flex_none().child(kit_avatar_element(
                &entry.actor,
                entry.photo.as_deref(),
                px(AVATAR),
            )))
            .child(column),
    )
    .into_any_element()
}

fn body_bubble(body: &LogBody, cx: &App) -> Option<AnyElement> {
    let theme = cx.theme();
    match body {
        LogBody::None => None,
        LogBody::Message(text) if text.is_empty() => None,
        LogBody::Message(text) => Some(
            Bubble::new()
                .with_variant(BubbleVariant::Muted)
                .child(div().child(text.clone()))
                .into_any_element(),
        ),
        LogBody::Edited { original, new } => {
            let mut inner = div().flex().flex_col().gap_1();
            if !original.is_empty() {
                // The original text sits above the new one as a quote, the
                // way a reply shows what it answers.
                inner = inner.child(
                    div()
                        .flex()
                        .flex_col()
                        .pl_2()
                        .border_l_2()
                        .border_color(theme.primary)
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(theme.primary)
                                .child(tr(keys::ADMIN_LOG_PREVIOUS_MESSAGE)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(original.clone()),
                        ),
                );
            }
            if !new.is_empty() {
                inner = inner.child(div().child(new.clone()));
            }
            Some(
                Bubble::new()
                    .with_variant(BubbleVariant::Muted)
                    .child(inner)
                    .into_any_element(),
            )
        }
    }
}

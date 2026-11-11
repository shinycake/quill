//! `ready-bot-stream`: a bot streams a reply into the open chat, draft by
//! draft (`updatePendingMessage`), then sends the final message.
//!
//! With `QUILL_DEMO_STREAM_FRAMES=<dir>` the demo records the reply as
//! numbered PNG frames at 60 fps on a pinned clock, so the reveal can be
//! watched frame by frame (`ffmpeg -framerate 60 -i frame-%03d.png`). Set
//! `QUILL_DEMO_LINGER_MS` long enough for the recording; it writes `done`
//! when it is over. Without it the stream plays on the wall clock.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::telegram::client::OwnedEnvelope;
use quill::telegram::envelope::parse_envelope;
use std::sync::atomic::Ordering;
use std::time::Duration;

register_demos![
    DemoSpec::chats(
        "ready-bot-stream",
        "screenshot demo — a bot streaming its reply"
    )
    .setup(QuillApp::demo_ready_bot_stream)
    .timing(4500, 3500),
];

const CHAT: i64 = 21;
const DRAFT: i64 = 77;
/// The final message's id.
const FINAL: i64 = 310;

const REPLY: &str = "Streaming replies arrive in pieces: the bot sends a draft, then a longer one, \
until the final message replaces them all. A good client hides those seams. \
It lays the text out once, sweeps each line in from the left with a soft edge, \
and eases the bubble open as new lines start.\n\n\
When the real message lands, the reveal finishes where it was instead of \
jumping to the end, so the last words read like the first.";

/// When each draft arrives (ms after the stream starts) and how many
/// characters of [`REPLY`] it holds. Pieces come unevenly, as model
/// output does: short bursts, a pause, then a long one.
fn drafts() -> Vec<(u64, usize)> {
    let words: Vec<usize> = REPLY
        .char_indices()
        .filter(|(_, c)| *c == ' ' || *c == '\n')
        .map(|(at, _)| at)
        .collect();
    let mut out = Vec::new();
    let mut at = 0;
    let mut time = 0;
    let steps = [3usize, 5, 2, 7, 4, 9, 3, 6, 8, 4];
    let gaps = [180u64, 240, 140, 320, 200, 260, 160, 380, 220, 200];
    let mut step = 0;
    while at < words.len() {
        at = (at + steps.get(step % steps.len()).copied().unwrap_or(4)).min(words.len());
        let cut = words.get(at).copied().unwrap_or(REPLY.len());
        out.push((time, REPLY.get(..cut).unwrap_or(REPLY).chars().count()));
        time += gaps.get(step % gaps.len()).copied().unwrap_or(200);
        step += 1;
    }
    out.push((time, REPLY.chars().count()));
    out
}

/// How long after the last draft the final message arrives.
const FINAL_AFTER_MS: u64 = 120;
/// Frames recorded after the final message.
const TAIL_MS: u64 = 1200;
const FRAME_MS: u64 = 1000 / 60;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn draft_json(chars: usize) -> String {
    let text: String = REPLY.chars().take(chars).collect();
    format!(
        r#"{{"@type":"updatePendingMessage","chat_id":{CHAT},"forum_topic_id":0,"draft_id":{DRAFT},"can_stop":true,"keep_on_stop":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{}","entities":[]}}}}}}"#,
        escape(&text)
    )
}

fn final_json() -> String {
    let date = quill::state::unix_ms_now() / 1000;
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{FINAL},"chat_id":{CHAT},"is_outgoing":false,"date":{date},"sender_id":{{"@type":"messageSenderUser","user_id":{CHAT}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{}","entities":[]}}}}}}}}"#,
        escape(REPLY)
    )
}

fn apply_json(app: &mut QuillApp, json: &str, cx: &mut Context<QuillApp>) {
    let Some(session) = app.demo_session.as_mut() else {
        return;
    };
    let Ok(envelope) = parse_envelope(json) else {
        return;
    };
    let need = quill::state::redraw_need(session, &envelope);
    let seq = session.last_seq + 1;
    session.apply(OwnedEnvelope {
        seq,
        client_id: None,
        envelope,
    });
    app.redraw_polled(need, false, cx);
}

/// The stream's updates in order, with when each arrives.
fn script() -> Vec<(u64, String)> {
    let drafts = drafts();
    let end = drafts.last().map_or(0, |(at, _)| *at);
    let mut out: Vec<(u64, String)> = drafts
        .into_iter()
        .map(|(at, chars)| (at, draft_json(chars)))
        .collect();
    out.push((end + FINAL_AFTER_MS, final_json()));
    out
}

impl QuillApp {
    fn demo_ready_bot_stream(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            super::bots::apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        let question = r#"{"@type":"updateNewMessage","message":{"id":303,"chat_id":21,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"How should a chat app show a streamed reply?","entities":[]}}}}"#;
        apply_json(self, question, cx);
        match std::env::var_os("QUILL_DEMO_STREAM_FRAMES") {
            Some(dir) => record(std::path::PathBuf::from(dir), window, cx),
            None => play(window, cx),
        }
    }
}

/// The stream on the wall clock, starting once the window has drawn.
fn play(window: &mut Window, cx: &mut Context<QuillApp>) {
    QuillApp::spawn_app_in(window, cx, async move |app, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        let mut time = 0;
        for (at, json) in script() {
            cx.background_executor()
                .timer(Duration::from_millis(at.saturating_sub(time)))
                .await;
            time = at;
            if app
                .update(cx, |this, cx| apply_json(this, &json, cx))
                .is_err()
            {
                return;
            }
        }
    })
    .detach();
}

/// The stream on a pinned clock, one PNG per 60 fps frame.
#[cfg(feature = "demo-capture")]
fn record(dir: std::path::PathBuf, window: &mut Window, cx: &mut Context<QuillApp>) {
    let _ = std::fs::create_dir_all(&dir);
    QuillApp::spawn_app_in(window, cx, async move |app, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        let t0 = std::time::Instant::now();
        let script = script();
        let end = script.last().map_or(0, |(at, _)| *at) + TAIL_MS;
        let mut next = 0usize;
        let mut frame = 0u32;
        let mut time = 0u64;
        while time <= end {
            super::bot_stream::pin_clock(Some(t0 + Duration::from_millis(time)));
            while let Some((at, json)) = script.get(next)
                && *at <= time
            {
                if app
                    .update(cx, |this, cx| apply_json(this, json, cx))
                    .is_err()
                {
                    return;
                }
                next += 1;
            }
            let path = dir.join(format!("frame-{frame:03}.png"));
            let saved = cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                window
                    .render_to_image()
                    .map_err(|err| err.to_string())
                    .and_then(|image| image.save(&path).map_err(|err| err.to_string()))
            });
            if let Ok(Err(err)) = saved {
                eprintln!("quill bot stream recording: frame {frame}: {err}");
            }
            frame += 1;
            time += FRAME_MS;
        }
        super::bot_stream::pin_clock(None);
        let _ = std::fs::write(dir.join("done"), b"ok\n");
    })
    .detach();
}

#[cfg(not(feature = "demo-capture"))]
fn record(_: std::path::PathBuf, window: &mut Window, cx: &mut Context<QuillApp>) {
    play(window, cx);
}

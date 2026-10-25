//! `ready-ai-compose`: the AI compose box over a three-line draft in the
//! open chat, with four styles (one with a resolved custom emoji, one
//! with a creator). `QUILL_DEMO_AI` picks what shows: `translate` (the
//! default: "To English" with a result), `style` (a picked style with its
//! author line), `fix` (a diff with inserted, replaced and deleted text),
//! `loading` (Fix waiting on its answer), `waiting` (the Style tab with
//! nothing picked), `flood` (the daily limit: Increase Limit),
//! `languages` (the language picker), `about` (About AI Editor) or
//! `button` (the box closed, the composer's AI button and its hint).

use super::session_mut;
use crate::ui::ai_compose_state::AiComposeView;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ai_compose::{
    AiBoxError, AiComposeMode, AiResult, CardState, DiffEntity, DiffKind, DiffText,
    TextCompositionStyle, prefs,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{AiBoxAnswer, Session};
use quill::text::{TextEntity, TextEntityKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::ready(
        "ready-ai-compose",
        crate::ui::demo::seed_ready_custom_emoji_session,
        "screenshot demo — the AI compose box over a three-line draft"
    )
    .setup(QuillApp::demo_ai_compose),
];

/// The demo user who created the "Pirate" style.
pub(super) const CREATOR: i64 = 21;

pub(super) const DRAFT: &str =
    "Hi team,\nthe launch moves to Thursday afternoon.\nLet me know if anything blocks the review.";
const SPANISH_DRAFT: &str = "Hola equipo,\nel lanzamiento pasa al jueves por la tarde.\nAvísenme si algo bloquea la revisión.";
const TYPO_DRAFT: &str =
    "Hi team,\nthe lanch moves to thursday afternon.\nLet me know if anythig really blocks review.";

fn style(name: &str, title: &str, custom_emoji_id: i64, creator_user_id: i64) -> TextCompositionStyle {
    TextCompositionStyle {
        name: name.into(),
        custom_emoji_id,
        title: title.into(),
        is_custom: creator_user_id != 0,
        is_creator: false,
        install_count: 0,
        prompt: String::new(),
        creator_user_id,
        english_example: None,
    }
}

/// The styles, and the creator so "Style by" has a name.
pub(super) fn seed_ai_compose(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>) {
    let sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user = format!(
        r#"{{"@type":"updateUser","user":{{"id":{CREATOR},"first_name":"Maya","last_name":"Chen","accent_color_id":2,"type":{{"@type":"userTypeRegular"}}}}}}"#
    );
    if let Some(owned) = quill::telegram::client::copy_and_parse(&user, seq, &sink) {
        session.apply(owned);
    }
    let ai = &mut session.messages.ai_compose;
    ai.styles.set_server(vec![
        style("formal", "Formal", 0, 0),
        style("friendly", "Friendly", 4242, 0),
        style("pirate", "Pirate", 0, CREATOR),
        style("poem", "Poem", 0, 0),
    ]);
    ai.styles_received = true;
}

fn text_result(text: &str, bold: Option<&str>) -> AiResult {
    let entities = bold
        .and_then(|word| text.find(word).map(|start| (start, start + word.len())))
        .map(|(utf8_start, utf8_end)| TextEntity {
            utf8_start,
            utf8_end,
            kind: TextEntityKind::Bold,
        })
        .into_iter()
        .collect();
    AiResult {
        text: text.into(),
        entities,
        diff: None,
    }
}

/// A diff entity over `needle` in the ASCII `text`.
fn edit(text: &str, needle: &str, kind: DiffKind) -> DiffEntity {
    let offset = text.find(needle).unwrap_or(0);
    DiffEntity {
        offset: i32::try_from(offset).unwrap_or(0),
        length: i32::try_from(needle.len()).unwrap_or(0),
        kind,
    }
}

pub(super) fn fix_result() -> AiResult {
    let shown = "Hi team,\nthe launch moves to Thursday afternoon.\nPlease let me know if anything really blocks the review.";
    let replace = |old: &str| DiffKind::Replace {
        old_text: old.into(),
    };
    AiResult {
        text: "Hi team,\nthe launch moves to Thursday afternoon.\nPlease let me know if anything blocks the review.".into(),
        entities: Vec::new(),
        diff: Some(DiffText {
            text: shown.into(),
            edits: vec![
                edit(shown, "launch", replace("lanch")),
                edit(shown, "Thursday", replace("thursday")),
                edit(shown, "afternoon", replace("afternon")),
                edit(shown, "Please let", replace("Let")),
                edit(shown, "anything", replace("anythig")),
                edit(shown, "really ", DiffKind::Delete),
                DiffEntity {
                    length: 4,
                    ..edit(shown, "the review", DiffKind::Insert)
                },
            ],
        }),
    }
}

impl QuillApp {
    /// The answer for the box's pending request, through the session slot
    /// the upkeep drains, as a live answer arrives.
    pub(super) fn demo_ai_answer(&mut self, outcome: Result<AiResult, AiBoxError>) {
        let token = match self.ai_compose.open.as_ref().map(|open| open.ai.card()) {
            Some(CardState::Loading { token }) => *token,
            _ => return,
        };
        if let Some(session) = session_mut(self) {
            session.messages.ai_compose.box_answer = Some(AiBoxAnswer { token, outcome });
        }
    }

    fn demo_ai_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let variant = std::env::var("QUILL_DEMO_AI").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            seed_ai_compose(session, &self.demo_ui.seq, &self.demo_ui.sink);
        }
        let draft = match variant.as_str() {
            "fix" | "loading" => TYPO_DRAFT,
            "style" | "waiting" | "about" | "button" => DRAFT,
            _ => SPANISH_DRAFT,
        };
        self.set_composer_markup(draft, window, cx);
        prefs::update(|prefs| {
            prefs.tooltip_hidden = variant != "button";
            prefs.style_tooltip_hidden = variant != "waiting";
        });
        if variant == "button" {
            cx.notify();
            return;
        }
        self.open_ai_compose(window, cx);
        match variant.as_str() {
            "waiting" => {}
            "about" => self.set_ai_compose_view(AiComposeView::About, cx),
            "loading" => {
                self.ai_compose_update(window, cx, |ai| ai.set_mode(AiComposeMode::Fix));
            }
            "style" => {
                self.ai_compose_update(window, cx, |ai| ai.select_style("pirate"));
                self.demo_ai_answer(Ok(text_result(
                    "Ahoy, crew!\nThe launch be sailin' to Thursday afternoon.\nSing out if aught be blockin' the review, arr!",
                    None,
                )));
            }
            "fix" => {
                self.ai_compose_update(window, cx, |ai| ai.set_mode(AiComposeMode::Fix));
                self.demo_ai_answer(Ok(fix_result()));
            }
            _ => {
                self.ai_compose_update(window, cx, |ai| ai.set_mode(AiComposeMode::Translate));
                self.ai_compose_update(window, cx, |ai| ai.set_translate_to("en"));
                if variant == "flood" {
                    self.demo_ai_answer(Err(AiBoxError::FloodPremium));
                } else {
                    self.demo_ai_answer(Ok(text_result(
                        "Hi team,\nthe launch moves to Thursday afternoon.\nLet me know if anything blocks the review.",
                        Some("Thursday afternoon"),
                    )));
                }
                if variant == "languages" {
                    self.set_ai_compose_view(AiComposeView::Languages, cx);
                }
            }
        }
        cx.notify();
    }
}

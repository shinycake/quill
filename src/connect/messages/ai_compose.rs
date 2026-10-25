//! Connect driver: the AI compose box, the apply-in-place shortcut and the
//! text composition style requests (TDLib 1.8.69).
use super::*;
use crate::ai_compose::AiRequest;
use crate::state::{AiComposePurpose, ExampleAnswer, StyleLookup};
use crate::telegram::requests::{
    StyleByName, create_text_composition_style, edit_text_composition_style,
    get_text_composition_style_example, text_composition_style_by_name,
};

/// Non-blank and, when TDLib sent the option, at most `max` characters.
fn fits(text: &str, max: Option<usize>) -> bool {
    !text.trim().is_empty() && max.is_none_or(|max| text.chars().count() <= max)
}

impl<S: JsonSender> ConnectDriver<S> {
    fn send_ai_compose(
        &mut self,
        purpose: AiComposePurpose,
        chat_id: Option<ChatId>,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.session.request(
            RequestPurpose::Messages(MessagesPurpose::AiCompose(purpose)),
            chat_id,
        );
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// A style request that only names the style; the name rides
    /// `AiComposeState::op_names` for the answer.
    fn send_style_op(
        &mut self,
        purpose: AiComposePurpose,
        method: StyleByName,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if name.is_empty() || !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.send_ai_compose(purpose, None, |extra| {
            text_composition_style_by_name(extra, method, name)
        })?;
        self.session
            .messages
            .ai_compose
            .op_names
            .insert(extra, name.to_string());
        Ok(extra)
    }

    /// The compose box's request: `composeTextWithAi` for Translate and
    /// Style, `fixTextWithAi` for Fix. The answer lands in
    /// `AiComposeState::box_answer` with `token`.
    pub fn ai_box_request(
        &mut self,
        chat_id: ChatId,
        request: &AiRequest,
        token: u64,
    ) -> Result<RequestId, ConnectSendError> {
        let text = match request {
            AiRequest::Compose { text, .. } | AiRequest::Fix { text } => text,
        };
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        match request {
            AiRequest::Compose {
                text,
                translate_to,
                style_name,
                add_emojis,
            } => self.send_ai_compose(
                AiComposePurpose::BoxCompose { token },
                Some(chat_id),
                |extra| compose_text_with_ai(extra, text, translate_to, style_name, *add_emojis),
            ),
            AiRequest::Fix { text } => {
                self.send_ai_compose(AiComposePurpose::BoxFix { token }, Some(chat_id), |extra| {
                    fix_text_with_ai(extra, text)
                })
            }
        }
    }

    /// The apply-in-place shortcut (tdesktop `TriggerAiApplyInPlace`):
    /// `composeTextWithAi(text, "", style_name, false)` on the selection or
    /// the whole field. The answer lands in `AiComposeState::apply_in_place`.
    pub fn ai_apply_in_place(
        &mut self,
        chat_id: ChatId,
        text: &str,
        style_name: &str,
        token: u64,
    ) -> Result<RequestId, ConnectSendError> {
        if text.trim().is_empty() || style_name.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        self.send_ai_compose(
            AiComposePurpose::ApplyInPlace { token },
            Some(chat_id),
            |extra| compose_text_with_ai(extra, text, "", style_name, false),
        )
    }

    fn style_fields_fit(&self, title: &str, prompt: &str) -> bool {
        let limits = self.session.messages.ai_compose.limits;
        fits(title, limits.title_length_max) && fits(prompt, limits.prompt_length_max)
    }

    /// `createTextCompositionStyle`; the new style lands in
    /// `AiComposeState::style_edit` and the front of the style list.
    pub fn create_text_composition_style(
        &mut self,
        title: &str,
        custom_emoji_id: i64,
        prompt: &str,
        show_creator: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.style_fields_fit(title, prompt) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_ai_compose(AiComposePurpose::CreateStyle, None, |extra| {
            create_text_composition_style(extra, title, custom_emoji_id, prompt, show_creator)
        })
    }

    /// `editTextCompositionStyle` for a style the user created.
    pub fn edit_text_composition_style(
        &mut self,
        name: &str,
        title: &str,
        custom_emoji_id: i64,
        prompt: &str,
        show_creator: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if name.is_empty() || !self.chats_path_active() || !self.style_fields_fit(title, prompt) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_ai_compose(AiComposePurpose::EditStyle, None, |extra| {
            edit_text_composition_style(extra, name, title, custom_emoji_id, prompt, show_creator)
        })
    }

    /// `deleteTextCompositionStyle` for a style the user created.
    pub fn delete_text_composition_style(
        &mut self,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_style_op(AiComposePurpose::DeleteStyle, StyleByName::Delete, name)
    }

    /// `removeTextCompositionStyle`: drop an added style from the list.
    pub fn remove_text_composition_style(
        &mut self,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_style_op(AiComposePurpose::RemoveStyle, StyleByName::Remove, name)
    }

    /// `addTextCompositionStyle`: save a shared style to the list.
    pub fn add_text_composition_style(
        &mut self,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_style_op(AiComposePurpose::AddStyle, StyleByName::Add, name)
    }

    /// `searchTextCompositionStyle` for an `addstyle/<name>` link; the
    /// answer fills `AiComposeState::style_lookup`.
    pub fn search_text_composition_style(
        &mut self,
        name: &str,
        token: u64,
    ) -> Result<RequestId, ConnectSendError> {
        if name.is_empty() || !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra =
            self.send_ai_compose(AiComposePurpose::SearchStyle { token }, None, |extra| {
                text_composition_style_by_name(extra, StyleByName::Search, name)
            })?;
        self.session.messages.ai_compose.style_lookup = Some(StyleLookup {
            token,
            name: name.to_string(),
            outcome: None,
        });
        Ok(extra)
    }

    /// `getTextCompositionStyleExample` number `example_number` (0-based,
    /// below `text_composition_style_example_count` when known); the answer
    /// fills `AiComposeState::example`.
    pub fn get_text_composition_style_example(
        &mut self,
        name: &str,
        example_number: i32,
        token: u64,
    ) -> Result<RequestId, ConnectSendError> {
        let count = self.session.messages.ai_compose.limits.example_count;
        let in_range = usize::try_from(example_number)
            .is_ok_and(|number| count.is_none_or(|count| number < count));
        if name.is_empty() || !in_range || !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra =
            self.send_ai_compose(AiComposePurpose::GetExample { token }, None, |extra| {
                get_text_composition_style_example(extra, name, example_number)
            })?;
        self.session.messages.ai_compose.example = Some(ExampleAnswer {
            token,
            style_name: name.to_string(),
            outcome: None,
        });
        Ok(extra)
    }
}

//! What each two-step password form sends. A form reads only the inputs it
//! draws, so text left in a field by another form (typed, then Back) can
//! never ride along: Turn off sends an empty new password on purpose, and
//! Change refuses one.

use quill::lang::{keys, tr};

/// The three forms that end in one `setPassword`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PasswordForm {
    Enable,
    Change,
    Disable,
}

/// The raw text of the shared inputs.
pub(super) struct FormInputs<'a> {
    pub current: &'a str,
    pub new: &'a str,
    pub hint: &'a str,
    pub email: &'a str,
}

/// The arguments of one `set_two_step_password` call.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PasswordRequest<'a> {
    pub current: &'a str,
    pub new: &'a str,
    pub hint: &'a str,
    pub email: Option<&'a str>,
}

impl PasswordForm {
    /// The request this form sends for `inputs`, or the notice to show.
    pub(super) fn request<'a>(
        self,
        inputs: &FormInputs<'a>,
    ) -> Result<PasswordRequest<'a>, &'static str> {
        match self {
            Self::Enable => {
                if inputs.new.is_empty() {
                    return Err(tr(keys::Q_PREFS_ENTER_A_NEW_PASSWORD));
                }
                Ok(PasswordRequest {
                    current: "",
                    new: inputs.new,
                    hint: inputs.hint,
                    email: Some(inputs.email).filter(|email| !email.is_empty()),
                })
            }
            Self::Change => {
                if inputs.current.is_empty() {
                    return Err(tr(keys::Q_PREFS_ENTER_YOUR_CURRENT_PASSWORD));
                }
                if inputs.new.is_empty() {
                    return Err(tr(keys::Q_PREFS_ENTER_A_NEW_PASSWORD));
                }
                Ok(PasswordRequest {
                    current: inputs.current,
                    new: inputs.new,
                    hint: inputs.hint,
                    email: None,
                })
            }
            Self::Disable => {
                if inputs.current.is_empty() {
                    return Err(tr(keys::Q_PREFS_ENTER_YOUR_CURRENT_PASSWORD));
                }
                Ok(PasswordRequest {
                    current: inputs.current,
                    new: "",
                    hint: "",
                    email: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FormInputs, PasswordForm, PasswordRequest};

    fn stale<'a>() -> FormInputs<'a> {
        FormInputs {
            current: "old",
            new: "hidden-new",
            hint: "hidden-hint",
            email: "a@b.c",
        }
    }

    #[test]
    fn change_with_an_empty_new_password_is_refused_not_a_disable() {
        let inputs = FormInputs { new: "", ..stale() };
        assert!(PasswordForm::Change.request(&inputs).is_err());
    }

    #[test]
    fn turn_off_ignores_text_left_in_the_other_forms_fields() {
        assert_eq!(
            PasswordForm::Disable.request(&stale()),
            Ok(PasswordRequest {
                current: "old",
                new: "",
                hint: "",
                email: None
            })
        );
    }

    #[test]
    fn change_never_carries_a_recovery_email() {
        let sent = PasswordForm::Change.request(&stale());
        assert_eq!(sent.map(|request| request.email), Ok(None));
    }

    #[test]
    fn enable_ignores_a_stale_current_password() {
        let sent = PasswordForm::Enable.request(&stale());
        assert_eq!(
            sent,
            Ok(PasswordRequest {
                current: "",
                new: "hidden-new",
                hint: "hidden-hint",
                email: Some("a@b.c")
            })
        );
    }

    #[test]
    fn turn_off_needs_the_current_password() {
        let inputs = FormInputs {
            current: "",
            ..stale()
        };
        assert!(PasswordForm::Disable.request(&inputs).is_err());
    }
}

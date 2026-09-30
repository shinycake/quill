use quill::state::LoginUrlRequest;
/// B1: a `loginUrlInfoRequestConfirmation` (schema 1.8.67,
/// `schema/td_api.tl:12985` / `:3869`) awaiting user consent before the
/// `getLoginUrl` round-trip.
pub struct LoginUrlConfirm {
    pub(crate) domain: String,
    pub(crate) request_write_access: bool,
    pub(crate) request: LoginUrlRequest,
}

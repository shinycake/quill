# Sign-in screens

Sign-in used to live in the sidebar: an "Authorization" section, a developer status note ("Phone entry (live TDLib)"), "No chat list until Ready." and ghost "Submit phone" / "Submit code" buttons. Meanwhile the main pane said "Not signed in" over an inert composer. Until the account is ready, the window now shows one centered step card (new `ui/onboarding.rs`): app mark, step title and explanation, the step's form, a full-width primary **Continue** (disabled and loading while a sign-in request is in flight), and secondary actions ("Send the code again", "Forgot password?", "Sign in with QR code").

- QR sign-in shows the code on a white tile with the path to the scanner (Telegram → Settings → Devices → Link Desktop Device).
- Step copy in `auth.rs` drops developer wording ("The QR payload is never logged.", TDLib lifecycle sentences). Titles read as actions: "Enter the code", "Enter your password".
- The window title is "Quill" in every state.
- Registration, email login, password recovery and unsupported-state messages render in the same card through their existing forms and actions.

The synthetic (no-credentials) layout keeps its sidebar form.

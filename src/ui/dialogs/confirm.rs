/// Slice A3: which terminate the sessions overlay is confirming (TGX
/// `TerminateSessionQuestion` / `TerminateIncompleteSessionQuestion` /
/// `AreYouSureSessions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionsConfirm {
    /// `terminateSession` for one session; `incomplete` picks the login-
    /// attempt wording.
    TerminateOne { session_id: i64, incomplete: bool },
    /// `terminateAllOtherSessions`.
    TerminateAll,
}

/// Slice A4: which disconnect the websites overlay is confirming (TGX
/// `TerminateWebSessionQuestion` "Disconnect %1$s?" /
/// `DisconnectAllWebsitesHint` "Are you sure you want to disconnect all
/// websites?").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebsitesConfirm {
    /// `disconnectWebsite` for one website.
    DisconnectOne { website_id: i64 },
    /// `disconnectAllWebsites`.
    DisconnectAll,
}

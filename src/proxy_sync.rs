//! One proxy list for the whole app. Telegram Desktop keeps proxy settings
//! app-wide (`Core::Settings::proxy()`: the list, the selected proxy and
//! "try IPv6"), and every account connects through them. TDLib keeps them
//! per client, in each account's database, so Quill treats the list of the
//! account on screen as the app's list and makes every background account's
//! list match it: same proxies, same one enabled (or none), same IPv6
//! choice. The proxy auto-switch runs for the account on screen only; the
//! proxy it picks is mirrored like any other selection.
//!
//! Nothing is written to disk here: the proxies, with their logins and
//! secrets, stay inside TDLib's encrypted databases.

use crate::proxy::{ProxyDraft, ProxyEntry};

/// One change that brings a background account's list closer to the app's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxySyncStep {
    /// A proxy the app's list has and this account lacks.
    Add {
        proxy: ProxyDraft,
        comment: String,
        enable: bool,
    },
    /// A proxy this account has and the app's list does not.
    Remove { id: i32 },
    /// Enable this account's copy of the app's enabled proxy.
    Enable { id: i32 },
    /// The app connects directly: so does this account.
    Disable,
}

/// The steps that make `current` (a background account's list) match
/// `target` (the list of the account on screen). Proxies match by their
/// whole connection data (kind, address, login, secret); TDLib ids differ
/// between accounts. Empty once the lists match.
///
/// TDLib takes one proxy change at a time in Quill (`send_proxy_mutation`)
/// and every change refetches the list, so callers apply the first step
/// and plan again on the fresh list.
pub fn plan_proxy_sync(target: &[ProxyEntry], current: &[ProxyEntry]) -> Vec<ProxySyncStep> {
    let mut steps = Vec::new();
    for entry in current {
        if !target.iter().any(|t| t.proxy == entry.proxy) {
            steps.push(ProxySyncStep::Remove { id: entry.id });
        }
    }
    for entry in target {
        if !current.iter().any(|c| c.proxy == entry.proxy) {
            steps.push(ProxySyncStep::Add {
                proxy: entry.proxy.clone(),
                comment: entry.comment.clone(),
                enable: entry.is_enabled,
            });
        }
    }
    let wanted = target.iter().find(|t| t.is_enabled).map(|t| &t.proxy);
    let enabled = current.iter().find(|c| c.is_enabled);
    match (wanted, enabled) {
        (Some(wanted), _) => {
            // Its copy, when this account already has it; a missing one is
            // added enabled above.
            if let Some(copy) = current.iter().find(|c| &c.proxy == wanted)
                && !copy.is_enabled
            {
                steps.push(ProxySyncStep::Enable { id: copy.id });
            }
        }
        (None, Some(enabled)) => {
            // Removing the enabled proxy already drops it; otherwise go
            // direct explicitly.
            if target.iter().any(|t| t.proxy == enabled.proxy) {
                steps.push(ProxySyncStep::Disable);
            }
        }
        (None, None) => {}
    }
    steps
}

#[cfg(test)]
mod tests;

//! A paged list the hub dialogs show (the Stars history, the received
//! gifts): one value for the request's stage with the list inside it, so
//! "loading", "failed" and "answered" can never be set together and the
//! list never outlives a stage that says it is gone.
//!
//! The request id sits in the loading stage: an answer or an error whose
//! `@extra` is not the newest request's is dropped (a filter switched
//! mid-flight, an older page answering late).

/// Where a paged list is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing<L> {
    /// Not asked, or dropped for a new ask (a filter switched, a
    /// conversion invalidated it). `L` is what was shown meanwhile.
    Idle(L),
    /// The request `request` is out; `append` is whether it is the next
    /// page of `list` (else a first page that replaces it).
    Loading { request: u64, append: bool, list: L },
    /// The newest request answered.
    Ready(L),
    /// The newest request failed; `append` says which page it was.
    Failed {
        error: String,
        append: bool,
        list: L,
    },
}

impl<L: Default> Default for Listing<L> {
    fn default() -> Self {
        Self::Idle(L::default())
    }
}

impl<L> Listing<L> {
    /// The list as it stands, whatever the stage.
    pub fn list(&self) -> &L {
        match self {
            Self::Idle(list)
            | Self::Ready(list)
            | Self::Loading { list, .. }
            | Self::Failed { list, .. } => list,
        }
    }

    pub fn list_mut(&mut self) -> &mut L {
        match self {
            Self::Idle(list)
            | Self::Ready(list)
            | Self::Loading { list, .. }
            | Self::Failed { list, .. } => list,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// A list was answered at some point and is shown now.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Whether `request` is the newest one out.
    pub fn is_request(&self, request: u64) -> bool {
        matches!(self, Self::Loading { request: out, .. } if *out == request)
    }

    /// The first page may be asked for: never asked, dropped, or its first
    /// page failed. A next page that failed is asked by scrolling again,
    /// and a request out or an answer is not asked twice.
    pub fn may_ask_first(&self) -> bool {
        matches!(self, Self::Idle(_) | Self::Failed { append: false, .. })
    }

    /// Take the list out, leaving the stage as it was.
    fn into_list(self) -> L {
        match self {
            Self::Idle(list)
            | Self::Ready(list)
            | Self::Loading { list, .. }
            | Self::Failed { list, .. } => list,
        }
    }

    /// A request goes out. The error line is gone; the list stays shown.
    pub fn begin(&mut self, request: u64, append: bool)
    where
        L: Default,
    {
        let list = std::mem::take(self).into_list();
        *self = Self::Loading {
            request,
            append,
            list,
        };
    }

    /// The newest request answered with `list`.
    pub fn succeed(&mut self, list: L) {
        *self = Self::Ready(list);
    }

    /// The newest request failed; the list stays shown.
    pub fn fail(&mut self, error: String)
    where
        L: Default,
    {
        let append = matches!(self, Self::Loading { append: true, .. });
        let list = std::mem::take(self).into_list();
        *self = Self::Failed {
            error,
            append,
            list,
        };
    }

    /// The list is out of date: the next ask is a first page again, and
    /// what is shown stays until it lands. A request out is abandoned (its
    /// answer no longer matches), so a mutation's confirmation that comes
    /// in between is never overwritten by an older page.
    pub fn invalidate(&mut self)
    where
        L: Default,
    {
        let list = std::mem::take(self).into_list();
        *self = Self::Idle(list);
    }

    /// Forget the error line, keeping the list; a request out is left.
    pub fn clear_error(&mut self)
    where
        L: Default,
    {
        if let Self::Failed { append, .. } = self {
            let append = *append;
            let list = std::mem::take(self).into_list();
            *self = if append {
                Self::Ready(list)
            } else {
                Self::Idle(list)
            };
        }
    }

    /// Drop everything for a new owner or filter.
    pub fn reset(&mut self)
    where
        L: Default,
    {
        *self = Self::default();
    }
}

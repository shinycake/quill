//! The Business dialog's open screen as one value: the section being
//! edited with its draft, and the sub-screen over it (the include/exclude
//! picker, a weekday, the time zones, the sticker panel, the link box).
//! A draft exists exactly while its section does, and a sub-screen is part
//! of the section it hangs from, so the screen and the drafts can never
//! disagree.

use super::state::{
    AwayDraft, ChatbotDraft, DayRow, GreetingDraft, HoursDraft, LinkEditDraft, LocationDraft,
    PickerDraft, StartPageDraft, View,
};

/// A section's draft: waiting for the full info it is built from, or ready.
pub(crate) enum Draft<T> {
    /// The screen shows "Loading…" (or the error that came instead).
    Waiting,
    Ready(T),
}

impl<T> Draft<T> {
    pub(super) fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(draft) => Some(draft),
            Self::Waiting => None,
        }
    }

    pub(super) fn ready_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Ready(draft) => Some(draft),
            Self::Waiting => None,
        }
    }

    /// Take the draft out; the section waits until it is put back.
    pub(super) fn take(&mut self) -> Option<T> {
        match std::mem::replace(self, Self::Waiting) {
            Self::Ready(draft) => Some(draft),
            Self::Waiting => None,
        }
    }

    pub(super) fn is_waiting(&self) -> bool {
        matches!(self, Self::Waiting)
    }
}

/// Over the hours: the week, a weekday's intervals, or the time zones.
pub(crate) enum HoursSub {
    Week,
    Day { day: usize, rows: Vec<DayRow> },
    TimeZone,
}

/// Over the start page: the page, or the sticker panel choosing its sticker.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartPageSub {
    Page,
    Stickers,
}

pub(crate) enum Screen {
    Hub,
    Greeting {
        draft: Draft<GreetingDraft>,
        picker: Option<PickerDraft>,
    },
    Away {
        draft: Draft<AwayDraft>,
        picker: Option<PickerDraft>,
    },
    Hours {
        draft: Draft<HoursDraft>,
        sub: HoursSub,
    },
    Location(Draft<LocationDraft>),
    StartPage {
        draft: Draft<StartPageDraft>,
        sub: StartPageSub,
    },
    ChatLinks {
        /// The new / edit link box.
        edit: Option<LinkEditDraft>,
        /// The link waiting for "Delete" to be confirmed.
        confirm_delete: Option<String>,
    },
    Chatbots {
        draft: Draft<ChatbotDraft>,
        picker: Option<PickerDraft>,
    },
}

impl Screen {
    /// The screen the user sees.
    pub(crate) fn view(&self) -> View {
        match self {
            Self::Hub => View::Hub,
            Self::Greeting { picker, .. } => {
                picker.as_ref().map_or(View::Greeting, |_| View::Picker)
            }
            Self::Away { picker, .. } => picker.as_ref().map_or(View::Away, |_| View::Picker),
            Self::Hours { sub, .. } => match sub {
                HoursSub::Week => View::Hours,
                HoursSub::Day { day, .. } => View::Day(*day),
                HoursSub::TimeZone => View::TimeZone,
            },
            Self::Location(_) => View::Location,
            Self::StartPage { sub, .. } => match sub {
                StartPageSub::Page => View::StartPage,
                StartPageSub::Stickers => View::StickerPick,
            },
            Self::ChatLinks { edit, .. } => {
                edit.as_ref().map_or(View::ChatLinks, |_| View::LinkEdit)
            }
            Self::Chatbots { picker, .. } => {
                picker.as_ref().map_or(View::Chatbots, |_| View::Picker)
            }
        }
    }

    /// Where Back goes: from a sub-screen to its section, else the hub.
    pub(crate) fn parent(&self) -> View {
        match self {
            Self::Hub => View::Hub,
            Self::Greeting {
                picker: Some(_), ..
            } => View::Greeting,
            Self::Away {
                picker: Some(_), ..
            } => View::Away,
            Self::Chatbots {
                picker: Some(_), ..
            } => View::Chatbots,
            Self::Hours { sub, .. } if !matches!(sub, HoursSub::Week) => View::Hours,
            Self::StartPage {
                sub: StartPageSub::Stickers,
                ..
            } => View::StartPage,
            Self::ChatLinks { edit: Some(_), .. } => View::ChatLinks,
            _ => View::Hub,
        }
    }

    /// Show `view`. The section of `view` keeps its draft when it is the
    /// one open (coming back from a sub-screen); any other starts waiting
    /// for its draft. A sub-screen needing data (a picker, a link box) is
    /// opened with it ([`Self::open_picker`], [`Self::open_link_edit`]),
    /// and one that is not part of the open section is not shown.
    pub(crate) fn go(&mut self, view: View) {
        let kept = std::mem::replace(self, Self::Hub);
        *self = match (kept, view) {
            (_, View::Hub) => Self::Hub,
            (Self::Greeting { draft, picker }, View::Picker) if picker.is_some() => {
                Self::Greeting { draft, picker }
            }
            (Self::Away { draft, picker }, View::Picker) if picker.is_some() => {
                Self::Away { draft, picker }
            }
            (Self::Chatbots { draft, picker }, View::Picker) if picker.is_some() => {
                Self::Chatbots { draft, picker }
            }
            (
                Self::ChatLinks {
                    edit,
                    confirm_delete,
                },
                View::LinkEdit,
            ) => Self::ChatLinks {
                edit,
                confirm_delete,
            },
            (Self::Greeting { draft, .. }, View::Greeting) => Self::Greeting {
                draft,
                picker: None,
            },
            (Self::Away { draft, .. }, View::Away) => Self::Away {
                draft,
                picker: None,
            },
            (Self::Chatbots { draft, .. }, View::Chatbots) => Self::Chatbots {
                draft,
                picker: None,
            },
            (Self::Hours { draft, .. }, View::Hours) => Self::Hours {
                draft,
                sub: HoursSub::Week,
            },
            (Self::Hours { draft, .. }, View::Day(day)) => Self::Hours {
                draft,
                sub: HoursSub::Day {
                    day,
                    rows: Vec::new(),
                },
            },
            (Self::Hours { draft, .. }, View::TimeZone) => Self::Hours {
                draft,
                sub: HoursSub::TimeZone,
            },
            (Self::StartPage { draft, .. }, View::StartPage) => Self::StartPage {
                draft,
                sub: StartPageSub::Page,
            },
            (Self::StartPage { draft, .. }, View::StickerPick) => Self::StartPage {
                draft,
                sub: StartPageSub::Stickers,
            },
            (_, View::Greeting) => Self::Greeting {
                draft: Draft::Waiting,
                picker: None,
            },
            (_, View::Away) => Self::Away {
                draft: Draft::Waiting,
                picker: None,
            },
            (_, View::Hours) => Self::Hours {
                draft: Draft::Waiting,
                sub: HoursSub::Week,
            },
            (_, View::Location) => Self::Location(Draft::Waiting),
            (_, View::StartPage) => Self::StartPage {
                draft: Draft::Waiting,
                sub: StartPageSub::Page,
            },
            (_, View::ChatLinks | View::LinkEdit) => Self::ChatLinks {
                edit: None,
                confirm_delete: None,
            },
            (_, View::Chatbots) => Self::Chatbots {
                draft: Draft::Waiting,
                picker: None,
            },
            // A sub-screen of a section that is not open: stays where it was.
            (kept, _) => kept,
        };
    }

    /// Open the picker over the greeting, away or chatbot section.
    pub(crate) fn open_picker(&mut self, draft: PickerDraft) {
        match self {
            Self::Greeting { picker, .. }
            | Self::Away { picker, .. }
            | Self::Chatbots { picker, .. } => *picker = Some(draft),
            _ => {}
        }
    }

    /// Open the new / edit link box over the chat links.
    pub(crate) fn open_link_edit(&mut self, draft: LinkEditDraft) {
        if let Self::ChatLinks { edit, .. } = self {
            *edit = Some(draft);
        }
    }

    pub(crate) fn picker(&self) -> Option<&PickerDraft> {
        match self {
            Self::Greeting { picker, .. }
            | Self::Away { picker, .. }
            | Self::Chatbots { picker, .. } => picker.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn picker_mut(&mut self) -> Option<&mut PickerDraft> {
        match self {
            Self::Greeting { picker, .. }
            | Self::Away { picker, .. }
            | Self::Chatbots { picker, .. } => picker.as_mut(),
            _ => None,
        }
    }

    /// Close the picker, handing it back.
    pub(crate) fn take_picker(&mut self) -> Option<PickerDraft> {
        match self {
            Self::Greeting { picker, .. }
            | Self::Away { picker, .. }
            | Self::Chatbots { picker, .. } => picker.take(),
            _ => None,
        }
    }

    pub(crate) fn greeting(&self) -> Option<&GreetingDraft> {
        match self {
            Self::Greeting { draft, .. } => draft.ready(),
            _ => None,
        }
    }

    pub(crate) fn greeting_mut(&mut self) -> Option<&mut GreetingDraft> {
        match self {
            Self::Greeting { draft, .. } => draft.ready_mut(),
            _ => None,
        }
    }

    pub(crate) fn away(&self) -> Option<&AwayDraft> {
        match self {
            Self::Away { draft, .. } => draft.ready(),
            _ => None,
        }
    }

    pub(crate) fn away_mut(&mut self) -> Option<&mut AwayDraft> {
        match self {
            Self::Away { draft, .. } => draft.ready_mut(),
            _ => None,
        }
    }

    pub(crate) fn hours(&self) -> Option<&HoursDraft> {
        match self {
            Self::Hours { draft, .. } => draft.ready(),
            _ => None,
        }
    }

    pub(crate) fn hours_mut(&mut self) -> Option<&mut HoursDraft> {
        match self {
            Self::Hours { draft, .. } => draft.ready_mut(),
            _ => None,
        }
    }

    pub(crate) fn location(&self) -> Option<&LocationDraft> {
        match self {
            Self::Location(draft) => draft.ready(),
            _ => None,
        }
    }

    pub(crate) fn location_mut(&mut self) -> Option<&mut LocationDraft> {
        match self {
            Self::Location(draft) => draft.ready_mut(),
            _ => None,
        }
    }

    pub(crate) fn start_page(&self) -> Option<&StartPageDraft> {
        match self {
            Self::StartPage { draft, .. } => draft.ready(),
            _ => None,
        }
    }

    pub(crate) fn start_page_mut(&mut self) -> Option<&mut StartPageDraft> {
        match self {
            Self::StartPage { draft, .. } => draft.ready_mut(),
            _ => None,
        }
    }

    pub(crate) fn chatbot(&self) -> Option<&ChatbotDraft> {
        match self {
            Self::Chatbots { draft, .. } => draft.ready(),
            _ => None,
        }
    }

    pub(crate) fn chatbot_mut(&mut self) -> Option<&mut ChatbotDraft> {
        match self {
            Self::Chatbots { draft, .. } => draft.ready_mut(),
            _ => None,
        }
    }

    /// Take the chatbot draft out of its section (put back with
    /// [`Self::put_chatbot`]).
    pub(crate) fn take_chatbot(&mut self) -> Option<ChatbotDraft> {
        match self {
            Self::Chatbots { draft, .. } => draft.take(),
            _ => None,
        }
    }

    pub(crate) fn put_chatbot(&mut self, new: ChatbotDraft) {
        if let Self::Chatbots { draft, .. } = self {
            *draft = Draft::Ready(new);
        }
    }

    /// Whether the open section still waits for its draft.
    pub(crate) fn draft_missing(&self) -> bool {
        match self {
            Self::Hub | Self::ChatLinks { .. } => false,
            Self::Greeting { draft, .. } => draft.is_waiting(),
            Self::Away { draft, .. } => draft.is_waiting(),
            Self::Hours { draft, .. } => draft.is_waiting(),
            Self::Location(draft) => draft.is_waiting(),
            Self::StartPage { draft, .. } => draft.is_waiting(),
            Self::Chatbots { draft, .. } => draft.is_waiting(),
        }
    }

    pub(crate) fn day_rows(&self) -> &[DayRow] {
        match self {
            Self::Hours {
                sub: HoursSub::Day { rows, .. },
                ..
            } => rows,
            _ => &[],
        }
    }

    pub(crate) fn day_rows_mut(&mut self) -> Option<&mut Vec<DayRow>> {
        match self {
            Self::Hours {
                sub: HoursSub::Day { rows, .. },
                ..
            } => Some(rows),
            _ => None,
        }
    }

    pub(crate) fn link_edit_mut(&mut self) -> Option<&mut LinkEditDraft> {
        match self {
            Self::ChatLinks { edit, .. } => edit.as_mut(),
            _ => None,
        }
    }

    pub(crate) fn link_edit(&self) -> Option<&LinkEditDraft> {
        match self {
            Self::ChatLinks { edit, .. } => edit.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn confirm_delete(&self) -> Option<&String> {
        match self {
            Self::ChatLinks { confirm_delete, .. } => confirm_delete.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn set_confirm_delete(&mut self, link: Option<String>) {
        if let Self::ChatLinks { confirm_delete, .. } = self {
            *confirm_delete = link;
        }
    }
}

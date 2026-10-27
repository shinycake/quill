//! Per-page entry tables, in display order.

mod about;
mod account;
mod advanced;
mod agent_access;
mod appearance;
mod calls;
mod chats;
mod connection;
mod data;
mod devices;
mod folders;
mod language;
mod notifications;
mod power;
mod privacy;
mod shortcuts;
mod system;
mod telegram;

use super::{Entry, Page};

pub(super) fn of(page: Page) -> &'static [Entry] {
    match page {
        Page::Account => account::ENTRIES,
        Page::Notifications => notifications::ENTRIES,
        Page::Privacy => privacy::ENTRIES,
        Page::Devices => devices::ENTRIES,
        Page::AgentAccess => agent_access::ENTRIES,
        Page::Chats => chats::ENTRIES,
        Page::Appearance => appearance::ENTRIES,
        Page::Folders => folders::ENTRIES,
        Page::DataStorage => data::ENTRIES,
        Page::Connection => connection::ENTRIES,
        Page::Calls => calls::ENTRIES,
        Page::Power => power::ENTRIES,
        Page::Language => language::ENTRIES,
        Page::Shortcuts => shortcuts::ENTRIES,
        Page::System => system::ENTRIES,
        Page::Telegram => telegram::ENTRIES,
        Page::About => about::ENTRIES,
        Page::Advanced => advanced::ENTRIES,
    }
}

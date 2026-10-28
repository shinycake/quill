//! Ranking settings for a query: fuzzy title matches first, then English
//! titles, keywords and synonyms, then the section and page a row sits in.
//! Ties go to shallower entries (tdesktop sorts by depth) and then to the
//! ones the user opened from search recently.

use super::{Entry, Page, Platforms, all, fuzzy, synonyms};
use crate::lang::Key;

/// What a search result opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Page(Page),
    Entry(&'static Entry),
}

impl Target {
    /// The stable id stored in search history.
    pub fn id(self) -> &'static str {
        match self {
            Target::Page(page) => page.id(),
            Target::Entry(entry) => entry.id,
        }
    }

    pub fn title(self) -> Key {
        match self {
            Target::Page(page) => page.title(),
            Target::Entry(entry) => entry.title,
        }
    }

    /// The page it opens on.
    pub fn page(self) -> Page {
        match self {
            Target::Page(page) => page,
            Target::Entry(entry) => entry.page,
        }
    }

    /// Whether it shows, given whether Agent access is on ([`Page::shown`]).
    pub fn shown(self, agent_access: bool) -> bool {
        match self {
            Target::Page(page) => page.shown(agent_access),
            Target::Entry(entry) => entry.shown(agent_access),
        }
    }

    fn depth(self) -> u8 {
        match self {
            Target::Page(_) => 0,
            Target::Entry(entry) => entry.depth(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub target: Target,
    pub score: i32,
    /// Char indices of the matched letters in the translated title.
    pub highlight: Vec<usize>,
}

/// What search needs from the app: strings in the current language and in
/// English, the platform, recent search picks (newest first) and whether
/// Agent access is on (off, its page and rows are not found).
pub struct SearchContext<'a> {
    pub text: &'a dyn Fn(Key) -> &'static str,
    pub english: &'a dyn Fn(Key) -> &'static str,
    pub platform: Platforms,
    pub recent: &'a [String],
    pub agent_access: bool,
}

/// Most results shown for one query.
pub const MAX_RESULTS: usize = 50;

// Field weights, in percent of a title match.
const ENGLISH_TITLE: i32 = 90;
const KEYWORD: i32 = 75;
const SYNONYM: i32 = 85;
const CONTEXT: i32 = 45;
const RECENT_BONUS: i32 = 30;

fn weighted(m: Option<fuzzy::Match>, percent: i32) -> Option<(i32, Vec<usize>)> {
    m.map(|m| (m.score * percent / 100, Vec::new()))
}

fn better(a: Option<(i32, Vec<usize>)>, b: Option<(i32, Vec<usize>)>) -> Option<(i32, Vec<usize>)> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if b.0 > a.0 { b } else { a }),
        (a, b) => a.or(b),
    }
}

fn score_target(query: &str, target: Target, ctx: &SearchContext) -> Option<(i32, Vec<usize>)> {
    let title = (ctx.text)(target.title());
    let english = (ctx.english)(target.title());
    let mut best = fuzzy::best(query, title).map(|m| (m.score, m.positions));
    if english != title {
        best = better(best, weighted(fuzzy::best(query, english), ENGLISH_TITLE));
    }
    if let Target::Entry(entry) = target {
        for keyword in entry.keywords {
            best = better(best, weighted(fuzzy::subsequence(query, keyword), KEYWORD));
        }
        let mut context = String::new();
        if let Some(section) = entry.section {
            context.push_str((ctx.text)(section));
            context.push(' ');
        }
        context.push_str((ctx.text)(entry.page.title()));
        best = better(best, weighted(fuzzy::subsequence(query, &context), CONTEXT));
    }
    for alias in synonyms::expand(query) {
        best = better(best, weighted(fuzzy::subsequence(alias, title), SYNONYM));
        best = better(best, weighted(fuzzy::subsequence(alias, english), SYNONYM));
    }
    best
}

/// Settings matching `query`, best first. An empty query matches nothing;
/// the caller shows recent searches instead.
pub fn search(query: &str, ctx: &SearchContext) -> Vec<Hit> {
    if fuzzy::words(query).is_empty() {
        return Vec::new();
    }
    let pages = Page::ALL.into_iter().map(Target::Page);
    let entries = all()
        .filter(|entry| entry.platforms.contains(ctx.platform))
        .map(Target::Entry);
    let mut hits: Vec<Hit> = pages
        .chain(entries)
        .filter(|target| target.shown(ctx.agent_access))
        .filter_map(|target| {
            let (mut score, highlight) = score_target(query, target, ctx)?;
            if let Some(rank) = ctx.recent.iter().position(|id| id == target.id()) {
                score += RECENT_BONUS / (rank as i32 + 1);
            }
            Some(Hit {
                target,
                score,
                highlight,
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.target.depth().cmp(&b.target.depth()))
            .then_with(|| (ctx.text)(a.target.title()).cmp((ctx.text)(b.target.title())))
    });
    hits.truncate(MAX_RESULTS);
    hits
}

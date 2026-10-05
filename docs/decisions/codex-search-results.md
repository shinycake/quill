# codex/search-results — search results that look like the chat list

- Global search rows have the chat-row anatomy: a 40 px avatar (chat photo or initials), the
  title with the message date (message hits), and the preview with the query's matches in
  semibold. Matching is case-insensitive and skipped when lowercasing would shift byte offsets.
- In-chat search hits are compact rows: highlighted snippet plus date. The selected hit (the
  one the history jumped to) is tinted and `aria-selected`. This replaces the
  message-text-as-title row with a "Jump" / "Jump · selected" subtitle.
- Status lines only cover loading with nothing yet, no matches, and failure. "Searching…" no
  longer lingers over results, and the "Results for…" line and the "Search results" caption
  above the tabs are gone.
- While searching, folder tabs and the story tray step aside, since search spans every chat.
  Closing search is an X icon button instead of "Clear".
- Test: `match_ranges` (case-insensitive, unsafe-text skip).

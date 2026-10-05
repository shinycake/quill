# codex/history-rows-owned — one message clone per render, not two

- `history_message_list` takes the snapshotted `Vec<HistoryMessage>` by value. Album grouping
  is reduced to spans (album id, count), and rows take their messages from the owned iterator
  instead of cloning each one a second time.
- Behavior is unchanged: albums, sender runs, day separators and remeasure comparisons use the
  same inputs. Verified with the `ready-albums` and `ready-rich-message` captures.
- Still open: a history revision counter to skip rebuilding rows when nothing changed.

# Composer send/mic swap, placeholder, and date separators (tdesktop)

- **Send vs mic.** tdesktop's `HistoryWidget::showRecordButton` shows the
  record (mic) button while there is no sendable content and swaps it for
  Send once there is (text, attachments, an edit). Quill showed both at
  all times. The composer now shows exactly one.
- **Placeholder.** tdesktop `lng_message_ph`: "Write a message...".
- **Date separators.** tdesktop paints `langDayOfMonthFull`: always the
  date ("October 6"; "October 6, 2025" in other years), never "Today" or
  a weekday. `local_time::day_label` now matches.

Verified live.

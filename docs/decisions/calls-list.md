# Calls list

## What Telegram Desktop does

`Calls::BoxController::Row` (calls/calls_box_controller.cpp) draws one row
per call:

- the userpic and name;
- an arrow for the direction (`callArrowIn`, `callArrowOut`, and
  `callArrowMissed` in red), then the date: the time alone today, "yesterday
  at …" yesterday, "{date} at {time}" before that;
- a phone or camera button on the right that calls back.

Clicking the row opens the chat.

## What Quill does now

The calls tab follows that row:

- a 40 px userpic, with the name through `name_line`;
- the arrow before `chatlist_calls::call_status`, which uses tdesktop's
  `lng_call_box_status_*` strings, plus the duration when there was one;
- an icon button that calls back, with "Call again" as its tooltip.

The row itself opens the chat. The words "Missed" and "Call again" are gone
from the row: the red arrow and the icon say it, and the accessible label
keeps the full "Missed call" text.

## Verified

`ready-calls-history` was captured before and after, in light and dark.

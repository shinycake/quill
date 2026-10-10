# Refactor 4: state and envelope split by domain

## Why

Six files in `src/state` and `src/telegram/envelope` took most of the merge
conflicts between parallel feature PRs. Each one was a single enum or a
single `match` that every feature appended to:

| File | Lines before | PRs touching it in 30 h |
| --- | ---: | ---: |
| `src/state/session_apply.rs` | 3,654 | 36 |
| `src/state/session_apply_error.rs` | 2,064 | 29 |
| `src/state/request_purpose.rs` | 1,840 | 33 |
| `src/state/session.rs` | 1,733 | 36 |
| `src/telegram/envelope/payload.rs` | 2,863 | 29 |
| `src/telegram/envelope/envelope_types.rs` | 2,106 | 31 |

## What changed

There are 16 domains: `auth`, `bots`, `calls`, `chat_list`, `chats`,
`common` (payloads only), `groups`, `media`, `messages`, `payments`,
`search`, `settings`, `stickers`, `stories`, `threads`, `users`.

- `RequestPurpose` wraps one enum per domain:
  `RequestPurpose::Messages(MessagesPurpose)`. Each lives in
  `src/state/domains/<domain>/purpose.rs`.
- Unit variants keep their old flat spelling. `flat_purposes!` in each
  domain file declares an associated const, so `RequestPurpose::SendMessage`
  still works, in expressions and in patterns. The ~2,000 existing call
  sites did not change. Variants with fields are written in full:
  `RequestPurpose::Calls(CallsPurpose::CreateCall { is_video })`.
- Eleven variants with fields stay flat on the hub (`GetRepliedMessage`,
  `GetSharedMedia`, `GetMessageThread`, `GetMessageThreadHistory`,
  `GetSavedMessagesTopicHistory`, `GetSavedMessagesTags`,
  `SearchSavedMessages`, `GetChatBoosts`, `GetAdminChatInviteLinks`,
  `GetLinkJoinRequests`, `GetProfileChats`). The screenshot demo fixtures
  build them with field syntax, and those files belonged to another
  refactor. Moving them into their domains is a small follow-up.
- `Session::apply_error` keeps only the steps that apply to every request:
  the send-permission notice, the flood notice, rollbacks, failed downloads
  and auth errors. Everything else moved to
  `src/state/domains/<domain>/error.rs` and is reached through one dispatch
  on the purpose's domain. For any single purpose, its handlers still run
  in their original order. Domain handlers now run after the rollback step.
  No rollback-carrying purpose had a handler before it, so nothing changes.
- `EnvelopePayload` wraps one enum per domain
  (`src/telegram/envelope/domains/<domain>/mod.rs`). `Ok`, `Error` and
  `Unknown` stay on the hub. Each domain has a parser
  (`domains/<domain>/parse.rs`) that claims its TDLib type names and
  returns `Ok(None)` for every other name. `parse_payload` tries the
  domains in turn, then falls back to `ok` / `error` / unknown.
- `Session::apply_payload` passes each domain payload to
  `src/state/domains/<domain>/apply.rs`.
- Both hubs implement `Debug` by printing only the inner variant, so log
  lines, the frame-trace name and the user-action classifier (which reads
  `format!("{purpose:?}")`) print exactly what they printed before.
- Smaller moves: `Session::new` went to `session_new.rs`, the connection
  indicator to `session_connection.rs`, `RequestRollback` to
  `request_rollback.rs`, and the group/admin envelope tests to
  `channel_tests_admin.rs`. The four state test files over 1,300 lines
  (`groups`, `messages`, `chat_list`, `stories`) each gained a `*_more.rs`.

## How to add a request/update handler now

New request:

1. Add the variant to `src/state/domains/<domain>/purpose.rs`. If it is a
   unit variant, also add its name to that file's `flat_purposes!` list so
   it reads `RequestPurpose::Foo` like the others.
2. Send it from `src/connect/...` with `session.request(RequestPurpose::Foo, ...)`.
   For a variant with fields, use `RequestPurpose::<Domain>(<Domain>Purpose::Foo { .. })`
   or `<Domain>Purpose::Foo { .. }.into()`.
3. Handle the answer where it lands. That is the domain's `apply.rs` arm for
   the answer's payload type, or `session_apply_ok.rs` for a plain `ok`.
4. Handle failure in `src/state/domains/<domain>/error.rs`.

New update or answer type:

1. Add the variant to `<Domain>Payload` in
   `src/telegram/envelope/domains/<domain>/mod.rs`.
2. Add the `"tdlibTypeName" => ...` arm to that domain's `parse.rs`. Each
   type name must belong to exactly one domain. The compiler cannot catch
   a name claimed twice across files, so grep for it first.
3. Add the arm to `src/state/domains/<domain>/apply.rs`. That match is
   exhaustive, so the compiler points you there.

A new domain is the only change that touches the hubs
(`request_purpose.rs`, `envelope_types.rs`, `payload.rs`,
`session_apply.rs`, `session_apply_error.rs`, both `domains/mod.rs`).

## Line counts after

| File | Before | After |
| --- | ---: | ---: |
| `src/state/session_apply.rs` | 3,654 | 182 |
| `src/state/session_apply_error.rs` | 2,064 | 267 |
| `src/state/request_purpose.rs` | 1,840 | 226 |
| `src/state/session.rs` | 1,733 | 1,262 |
| `src/telegram/envelope/payload.rs` | 2,863 | 75 |
| `src/telegram/envelope/envelope_types.rs` | 2,106 | 359 |

The largest new file is `src/state/domains/groups/apply.rs` at 747 lines.
`session.rs` is still over the target because it is the `Session` struct
declaration itself (about 400 fields). Getting it smaller means grouping
fields into sub-structs, which changes `session.field` paths across the UI.
That is a separate change.

## Measurements

The benchmark was a release build of a scratch program that parsed and
applied 1,300 envelopes taken from the test suite's raw JSON (430 type
names). It was run three times, alternating old and new builds, on an M1
Pro:

| | Before | After |
| --- | ---: | ---: |
| `parse_envelope`, ns per envelope | 1,933–1,995 | 2,085–2,211 |
| `Session::apply`, ns per envelope (with clone) | 316–329 | 330–346 |
| `size_of::<Envelope>()` | 1,240 B | 1,240 B |
| `size_of::<RequestPurpose>()` | 48 B | 48 B |

Parsing costs about 150 ns more per envelope, roughly 8%. Each domain's
`match` now does its own length dispatch, and a type name that no domain
knows walks all 16. The JSON decode still dominates. If this ever shows
up in a profile, the fix is to cache type name → domain. Memory layout is
unchanged.

Tests: core 2,990 before and after. UI went from 224 on the old base to 230
after merging main, which brought new UI tests. Gate OK.

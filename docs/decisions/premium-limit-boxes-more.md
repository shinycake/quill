# More Premium limit boxes, with values from TDLib

Date: 2026-10-25. Branch: `premium-limit-boxes-more`. Parity id:
`parity:premium-limit-boxes-more`.

Before this change Quill showed limit boxes for pinned chats, captions,
joined communities, public links, file size and saved GIFs. The values came
from `getPremiumFeatures` when it had answered, and otherwise from constants
copied from tdesktop. Favorite stickers, pinned Saved Messages chats, story
posting, similar channels and owned bots had no limit surface at all.

## What Telegram Desktop does

Some limits get a box and some get a toast:

| Limit | tdesktop surface | Source |
| --- | --- | --- |
| Pinned Saved Messages chats | `SublistsPinsLimitBox`, which is the pins box with the sublist values | `boxes/premium_limits_box.cpp` |
| Saved GIFs | `SavedGifsToast`, shown when the oldest GIF is dropped | `data/stickers/data_stickers.cpp`, `lng_saved_gif_limit_*` |
| Favorite stickers | `FaveStickersToast`, same pattern | `data/stickers/data_stickers.cpp`, `lng_fave_sticker_limit_*` |
| Similar channels | A "Show more channels" button with a lock under the list, plus a Premium toast from the locked tile | `history_view_similar_channels.cpp`, `info_similar_peers_widget.cpp`, `lng_similar_channels_premium_all*` |
| Owned bots | A toast over the create-bot box. It offers Premium with a link to @BotFather, or only @BotFather once the Premium value is reached | `create_managed_bot_box.cpp`, `lng_bots_create_limit*` |
| Story posting | None, because tdesktop cannot post stories | none |

For the values, tdesktop uses `Data::PremiumLimits`, which reads the app
config and falls back to the constants in `data_premium_limits.cpp`.

## What changed

- `PremiumLimitKind` gains `FavoriteStickers`, `SavedPins`, `SimilarChats`,
  `OwnedBots`, `ActiveStories`, `WeeklyStories` and `MonthlyStories`, each
  with its `premiumLimitType*` constructor and a fallback pair.
- A new `LimitStore` on the Premium hub holds what TDLib reports:
  - `getPremiumLimit` answers. This reuses the folder request purpose, so
    `RequestPurpose` gains no variant.
  - The per-account options `favorite_stickers_limit`,
    `pinned_saved_messages_topic_count_max` and `owned_bot_count_max`.
  - Which limits have been asked for. The driver asks once per kind, the
    first time a box or toast needs it (`ensure_premium_limit`).
- `Session::premium_limit_pair` resolves a value in this order: the
  `getPremiumFeatures` rows, then the `getPremiumLimit` answer, then the
  fallback. `premium_limit_in_force` reads the option when TDLib has one.
  The box, the toasts and the similar-channels strip all use these two, so
  `session_similar.rs` no longer has its own constants.
- `open_premium_limit` sends the toast kinds (GIFs, stickers, similar
  channels, bots) to a toast queue that frame upkeep turns into a kit
  `Notification`. Links in the toast text open the Premium page or
  @BotFather (`start=deletebot`). Quill's saved-GIF limit used to be a box,
  so it is now a toast, as in tdesktop.
- Where each limit now shows:
  - **Saved Messages pins:** Quill counts the pinned sublists against the
    limit before sending the pin. It also maps the server's pins error on
    `toggleSavedMessagesTopicIsPinned` to the Saved Messages box rather than
    the chat-list one.
  - **Favorite stickers:** the toast shows when a favorite is added to a
    full list from the sticker panel, the message menu or the custom emoji
    menu.
  - **Owned bots:** `BOT_CREATE_LIMIT_EXCEEDED` on `createBot` shows the
    toast and leaves the create dialog open with no error line.
  - **Similar channels:** the profile's similar channels list shows
    "Show more channels" with a lock and the Premium line to free accounts.
    The locked strip tile now shows the toast.
  - **Story posting:** when `canPostStory` answers with an active, weekly or
    monthly limit, the box opens over the composer. The composer keeps its
    reason line, which has the retry time. tdesktop has nothing to copy
    here, so the wording follows the pins box.
- The Premium links send tdesktop's start refs:
  - `double_limits__saved_dialog_pinned`
  - `double_limits__stickers_faved`
  - `similar_channels`
  - `managed_bots`

  The story boxes, which have no tdesktop counterpart, send `stories`.
- The English strings are tdesktop's, in `lang/en/premium_limits.strings`.

## Deviations

- The story boxes are Quill's own design, as explained above.
- Similar bots have no unlock button. The users payload carries no total
  count to compare against.
- The "Doubled limits" page in the Premium hub still lists only the rows
  tdesktop lists. Rows that `getPremiumFeatures` returns without a title
  are used for values only.

## How it was verified

- Unit tests (`premium_limits.rs`):
  - Box text and toast routing for every kind.
  - The store's folding, ask-once and option rules.
  - The bots toast choice.
  - The story box sentences.
- Reducer tests:
  - The Saved Messages pins error and its precheck.
  - Toast kinds queue a toast.
  - The similar strip follows the TDLib value.
  - The bot limit error keeps the dialog open.
- Driver tests:
  - `getPremiumLimit` is sent once with the right type.
  - Its answer is used.
  - FileSize, and kinds already known from `getPremiumFeatures`, are never
    asked for.
  - The options set the limit in force.
- UI tests (`limit_demo_tests.rs`) with the real window:
  - The Saved Messages box shows Increase Limit only to a free account.
  - Each toast kind pushes one notification and opens no box.
  - The similar unlock button shows for free accounts only.
  - The three `canPostStory` limit answers each open their box over the
    composer, and other refusals do not.
- Demo `ready-premium-limits` (`QUILL_DEMO_PREMIUM_LIMIT` =
  saved-pins | stickers | gifs | similar | bots | similar-unlock | stories,
  plus `QUILL_DEMO_PREMIUM_ACCOUNT=1`). Each was captured in light and dark
  and read back.

Not verified: live TDLib answers for `getPremiumLimit` and the options, a
real `BOT_CREATE_LIMIT_EXCEEDED` or `canPostStory` limit answer, and a real
favorite sticker being replaced. The toast depends on the favorites list
having been loaded, which happens when the sticker panel opens.

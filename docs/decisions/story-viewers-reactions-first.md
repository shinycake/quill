# Story viewers list: reactions first

## What Telegram Desktop does

- `data/data_stories.cpp:1600-1604` `sendViewsSliceRequest` sends
  `stories.getStoryViewsList` with only `Flag::f_reactions_first`. It sets
  neither `just_contacts` nor `forwards_first`, and sends an empty query.
- The viewers list in `media/stories/media_stories_recent_views.cpp` and
  `media_stories_controller.cpp` has no sort or filter toggle. The order is
  fixed by that flag.

## What Quill did

`get_story_interactions` sent `prefer_with_reaction: false`, so viewers who
reacted were mixed into the list by view time.

## What changed

`prefer_with_reaction` is now `true`. `only_contacts` and `prefer_forwards`
stay `false`, matching tdesktop. The request test asserts the three flags.

## Not done

tdesktop has no sort or filter toggle, so there is nothing to add in Quill.
The `query` argument stays for name search, which tdesktop does not expose.

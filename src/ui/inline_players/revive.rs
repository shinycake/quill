//! Waking a sleeping inline player when its tile is painted in view.

use super::*;

impl InlineVideos {
    /// A sleeping slot was painted in view: open a new player for it (if
    /// the budget admits one), showing the sleeping one's last picture
    /// until its own, and redraw the histories showing it, whose tiles
    /// still hold the sleeping one.
    pub(super) fn revive(&self, key: Key, source: Source, cx: &mut App) {
        let Some((held, inactive, views, sleeping)) = self
            .with(|state| {
                let slot = state.players.get(&key)?;
                let views: Vec<EntityId> = slot
                    .windows
                    .keys()
                    .filter_map(|window| state.views.get(window).copied())
                    .collect();
                Some((
                    state.players.values().map(|slot| slot.bytes).sum::<usize>(),
                    state.inactive,
                    views,
                    slot.player.clone(),
                ))
            })
            .flatten()
        else {
            return;
        };
        if !admits(held, source.bytes) {
            return;
        }
        let Ok(player) = VideoPlayer::open(
            &source.path,
            Purpose::Inline {
                max_edge: source.max_edge,
            },
            source.duration,
            cx,
        ) else {
            self.with(|state| {
                if let Some(slot) = state.players.get_mut(&key) {
                    slot.rest = Rest::Stuck;
                }
            });
            return;
        };
        player.take_picture_from(&sleeping);
        drop(sleeping);
        player.set_volume(0.0);
        player.set_looping(true);
        // It is in view: play, unless the window is behind another app.
        if !inactive {
            player.play();
        }
        let mark = player.shown_count();
        let replaced = self.with(|state| {
            let slot = state.players.get_mut(&key)?;
            slot.rest = Rest::Awake;
            slot.bytes = source.bytes;
            for seen in slot.windows.values_mut() {
                seen.mark = mark;
            }
            slot.hidden_since = None;
            Some(std::mem::replace(&mut slot.player, player))
        });
        drop(replaced);
        for view in views {
            cx.notify(view);
        }
    }
}

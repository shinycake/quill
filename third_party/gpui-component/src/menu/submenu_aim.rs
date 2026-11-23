//! Menu aim: an open submenu stays open while the pointer travels
//! diagonally toward it, across the sibling rows in between.
//!
//! Telegram Desktop's rule (lib_ui `PopupMenu::handleMouseMoved` and
//! `insideSubmenuAim`, popup_menu.cpp). While the pointer is on the row that
//! owns the open submenu, that point is the apex of a triangle whose base is
//! the submenu's near edge. When the pointer leaves the row inside that
//! triangle, with no button down, the rows it crosses do not take the hover:
//! the submenu stays and the row under the pointer waits. Each move inside
//! the triangle starts the wait again; once the pointer rests for
//! [`SUBMENU_AIM_REST`] the row under it takes the hover after all. A move
//! out of the triangle, a press, or reaching the submenu ends the aim at
//! once, and hover works as usual again. Past the submenu's near edge and
//! outside this menu, the triangle widens to the submenu's full height (the
//! panels may overlap).
//!
//! [`SubmenuAim`] is only the decision: the caller says which row owns the
//! open submenu (if any), where the pointer is and over which row, and where
//! the two panels are. It holds the wake-up `W` (a `Task` in an app) for as
//! long as the aim waits, so leaving the stage cancels it; tests use `()`.

use std::time::{Duration, Instant};

use gpui::{Bounds, Pixels, Point, point};

/// How long the pointer may rest inside the triangle before the row under
/// it takes the hover (lib_ui `kSubmenuAimDelay`).
pub const SUBMENU_AIM_REST: Duration = Duration::from_millis(300);

/// Where a menu and its open submenu are, in the coordinates the pointer
/// positions use (window coordinates in GPUI).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimGeometry {
    /// The parent menu's rows area.
    pub menu: Bounds<Pixels>,
    /// The open submenu's rows area.
    pub submenu: Bounds<Pixels>,
}

impl AimGeometry {
    /// Whether `position` is on the way from `apex` to the submenu.
    pub fn corridor_contains(&self, apex: Point<Pixels>, position: Point<Pixels>) -> bool {
        let submenu = self.submenu;
        if submenu.size.width <= Pixels::ZERO || submenu.size.height <= Pixels::ZERO {
            return false;
        }
        let opens_right = submenu.center().x >= self.menu.center().x;
        let edge = if opens_right {
            submenu.left()
        } else {
            submenu.right()
        };
        let beyond = if opens_right {
            position.x >= edge
        } else {
            position.x <= edge
        };
        if beyond && !self.menu.contains(&position) {
            return position.y >= submenu.top() && position.y <= submenu.bottom();
        }
        point_in_triangle(
            position,
            apex,
            point(edge, submenu.top()),
            point(edge, submenu.bottom()),
        )
    }
}

/// Inside or on the edge of the triangle `a`, `b`, `c`, in either winding.
fn point_in_triangle(
    p: Point<Pixels>,
    a: Point<Pixels>,
    b: Point<Pixels>,
    c: Point<Pixels>,
) -> bool {
    let side = |from: Point<Pixels>, till: Point<Pixels>| {
        let (px, py) = (f32::from(p.x), f32::from(p.y));
        (f32::from(from.x) - px) * (f32::from(till.y) - py)
            - (f32::from(till.x) - px) * (f32::from(from.y) - py)
    };
    let sides = [side(a, b), side(b, c), side(c, a)];
    let negative = sides.iter().any(|side| *side < 0.);
    let positive = sides.iter().any(|side| *side > 0.);
    !(negative && positive)
}

/// One pointer move over the parent menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimPointer<T> {
    pub position: Point<Pixels>,
    /// The parent menu's row under the pointer, if any.
    pub row: Option<T>,
    /// Whether a mouse button is down (a drag never aims).
    pub pressed: bool,
}

/// Where the aim is. `T` names a row of the parent menu, `W` is the wake-up
/// the waiting stage holds.
enum AimStage<T, W> {
    /// Hover works as usual.
    Free,
    /// The pointer is on the row that owns the open submenu, at `apex`.
    OnOwner { owner: T, apex: Point<Pixels> },
    /// The pointer left the owner row toward the submenu. `held` is the row
    /// under it, which takes the hover if the pointer rests until
    /// `rests_at`. `_wake` calls [`SubmenuAim::rested`] then.
    Heading {
        owner: T,
        apex: Point<Pixels>,
        held: Option<T>,
        rests_at: Instant,
        _wake: W,
    },
}

/// The aim of one menu toward its open submenu. See the module docs.
pub struct SubmenuAim<T, W> {
    stage: AimStage<T, W>,
}

impl<T, W> Default for SubmenuAim<T, W> {
    fn default() -> Self {
        Self {
            stage: AimStage::Free,
        }
    }
}

impl<T: Copy + PartialEq, W> SubmenuAim<T, W> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The pointer moved. `owner` is the row whose submenu is open (`None`
    /// with no submenu open), `geometry` where the panels are (`None` before
    /// they were measured). Returns whether the row under the pointer takes
    /// the hover; `false` means it waits and the open submenu stays. On the
    /// move that starts the wait, `wake(wait)` makes the wake-up that must
    /// call [`Self::rested`] after `wait`.
    pub fn pointer_moved(
        &mut self,
        owner: Option<T>,
        pointer: AimPointer<T>,
        geometry: Option<AimGeometry>,
        now: Instant,
        wake: impl FnOnce(Duration) -> W,
    ) -> bool {
        let Some(owner) = owner else {
            self.stage = AimStage::Free;
            return true;
        };
        let position = pointer.position;
        if pointer.row == Some(owner) {
            self.stage = AimStage::OnOwner {
                owner,
                apex: position,
            };
            return true;
        }
        let apex = match &self.stage {
            AimStage::OnOwner { owner: aimed, apex }
            | AimStage::Heading {
                owner: aimed, apex, ..
            } if *aimed == owner => *apex,
            _ => {
                self.stage = AimStage::Free;
                return true;
            }
        };
        let heading = !pointer.pressed
            && geometry.is_some_and(|geometry| {
                !geometry.submenu.contains(&position)
                    && geometry.corridor_contains(apex, position)
            });
        if !heading {
            self.stage = AimStage::Free;
            return true;
        }
        let rests_at = now + SUBMENU_AIM_REST;
        match &mut self.stage {
            AimStage::Heading {
                held,
                rests_at: due,
                ..
            } => {
                *held = pointer.row;
                *due = rests_at;
            }
            _ => {
                self.stage = AimStage::Heading {
                    owner,
                    apex,
                    held: pointer.row,
                    rests_at,
                    _wake: wake(SUBMENU_AIM_REST),
                };
            }
        }
        false
    }

    /// The pointer left `row`: it no longer waits for the hover.
    pub fn row_left(&mut self, row: T) {
        if let AimStage::Heading { held, .. } = &mut self.stage
            && *held == Some(row)
        {
            *held = None;
        }
    }

    /// The wake-up fired. Before the pointer has rested long enough (it
    /// moved since) a new wake-up is made for the rest of the wait. After,
    /// the aim ends and the row the pointer rests on is returned: it takes
    /// the hover now. `pointer` is where the pointer is; on the submenu, or
    /// with `owner` no longer the aim's, the aim ends with no row.
    pub fn rested(
        &mut self,
        owner: Option<T>,
        pointer: Point<Pixels>,
        geometry: Option<AimGeometry>,
        now: Instant,
        wake: impl FnOnce(Duration) -> W,
    ) -> Option<T> {
        let AimStage::Heading {
            owner: aimed,
            held,
            rests_at,
            _wake,
            ..
        } = &mut self.stage
        else {
            return None;
        };
        let on_submenu = geometry.is_some_and(|geometry| geometry.submenu.contains(&pointer));
        if owner != Some(*aimed) || on_submenu {
            self.stage = AimStage::Free;
            return None;
        }
        if now < *rests_at {
            *_wake = wake(rests_at.saturating_duration_since(now));
            return None;
        }
        let held = *held;
        self.stage = AimStage::Free;
        held
    }

    /// Whether rows are waiting for the hover: while it is so, the rows the
    /// pointer crosses are not lit.
    pub fn holds(&self) -> bool {
        matches!(self.stage, AimStage::Heading { .. })
    }

    /// The menu closed or the submenu was opened by key.
    pub fn reset(&mut self) {
        self.stage = AimStage::Free;
    }
}

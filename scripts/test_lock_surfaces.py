"""Offline unit tests for the "what runs while locked" check in
scripts/lock_surfaces.py (run: python3 -m unittest discover -s scripts -p
'test_lock_surfaces.py'). scripts/check-screen-state.py runs the real check
on the tree."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lock_surfaces as ls  # noqa: E402

FRAME = "src/ui/app_render/frame_upkeep.rs::frame_upkeep"
POLL = "src/ui/notifications/mod.rs::poll_live"
LOCKED = {key: ["self.lock_overlay(cx);"] for key in ls.LOCKED_FRAMES}


def check(frame, poll="if !self.lock_waits() { self.poll_drains(cx); }", steps=None):
    bodies = {FRAME: [frame], POLL: [poll], **LOCKED}
    return ls.upkeep_problems(bodies, steps or {})


class WhatRunsWhileLocked(unittest.TestCase):
    def test_a_step_behind_the_early_return_waits(self):
        self.assertEqual(check("if self.lock_waits() { return; } self.open_story(cx);"), [])

    def test_a_step_inside_the_open_gate_waits(self):
        poll = "if !self.lock_waits() && self.poll_drains(cx) { self.more(cx); } "
        self.assertEqual(check("if self.lock_waits() { return; }", poll), [])

    def test_a_new_step_before_the_gate_fails_until_listed(self):
        frame = "self.open_story(cx); if self.lock_waits() { return; }"
        found = check(frame)
        self.assertEqual(len(found), 1)
        self.assertIn("open_story", found[0])
        self.assertEqual(check(frame, steps={(FRAME, "open_story"): 1}), [])

    def test_a_step_after_the_open_gate_fails(self):
        poll = "if !self.lock_waits() { self.poll_drains(cx); } self.resume_play(cx);"
        found = check("if self.lock_waits() { return; }", poll)
        self.assertEqual(len(found), 1)
        self.assertIn("resume_play", found[0])

    def test_an_upkeep_without_a_gate_fails(self):
        found = check("self.sync_signin(cx);", steps={(FRAME, "sync_signin"): 1})
        self.assertTrue(any("must test `lock_waits()`" in line for line in found))

    def test_a_listed_step_that_is_gone_fails(self):
        found = check("if self.lock_waits() { return; }", steps={(FRAME, "gone"): 4})
        self.assertTrue(any("no longer runs while locked" in line for line in found))

    def test_a_locked_frame_runs_no_upkeep(self):
        bodies = {FRAME: ["if self.lock_waits() { return; }"], POLL: ["if !self.lock_waits() {}"]}
        bodies.update(LOCKED)
        bodies[ls.LOCKED_FRAMES[0]] = ["self.per_window_upkeep(window, cx);"]
        found = ls.upkeep_problems(bodies, {})
        self.assertTrue(any("runs no upkeep" in line for line in found))


if __name__ == "__main__":
    unittest.main()

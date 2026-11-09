"""Offline unit tests for rule 5 of scripts/dialog_hosts.py: where no window
asked (the poll loop, the main window's own upkeep, the toast drain), an
open names the window its request carried (run: python3 -m unittest
discover -s scripts -p 'test_dialog_hosts.py'). scripts/check-window-scope.py
runs the real check on the tree."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dialog_hosts as dh  # noqa: E402

POLL = "src/ui/notifications/mod.rs"
TOAST = "src/ui/suggested_posts_ui.rs"
FRAME = "src/ui/app_render/frame_upkeep.rs"
CALLERS = "src/ui/callers.rs"


def tree(poll="", frame="", extra="", callers=None):
    """A src/ui with the three roots, run as nobody by their callers."""
    if callers is None:
        callers = (
            "fn a(&mut self) { self.unasked(|this| this.poll_live(cx)); }\n"
            "fn b(&mut self) { self.unasked(|this| this.frame_upkeep(window, cx)); }\n"
            "fn c(&mut self) { self.unasked(|this| this.drain_suggested_post_notes(cx)); }\n"
        )
    return [
        (POLL, f"fn poll_live(&mut self) {{ {poll} }}\n"),
        (TOAST, "fn drain_suggested_post_notes(&mut self) { }\n"),
        (FRAME, f"fn frame_upkeep(&mut self) {{ {frame} }}\n"),
        ("src/ui/extra.rs", extra),
        (CALLERS, callers),
    ]


class OpensWhereNoWindowAsked(unittest.TestCase):
    def test_an_open_the_poll_reaches_fails(self):
        found = dh.unasked_problems(
            tree("self.drain(cx);", extra="fn drain(&mut self) { self.open_box(DialogKind::X); }")
        )
        self.assertEqual(len(found), 1)
        self.assertIn("poll_live > drain", found[0])

    def test_an_open_under_the_carried_window_passes(self):
        extra = (
            "fn drain(&mut self) { let w = self.window_showing(chat); "
            "self.as_asked_by(w, |this| this.show(cx)); }\n"
            "fn show(&mut self) { self.open_box(DialogKind::X); }"
        )
        self.assertEqual(dh.unasked_problems(tree("self.drain(cx);", extra=extra)), [])

    def test_a_queued_link_counts_too(self):
        extra = "fn answer(&mut self) { self.queue_deep_link(url); }"
        found = dh.unasked_problems(tree(frame="self.answer(cx);", extra=extra))
        self.assertEqual(len(found), 1)
        self.assertIn("queue_deep_link", found[0])

    def test_the_windows_own_frame_in_the_main_frame_is_asked_in_place(self):
        frame = "self.asked_in_place(|this| this.per_window_upkeep(window, cx));"
        extra = "fn per_window_upkeep(&mut self) { self.open_box(DialogKind::X); }"
        self.assertEqual(dh.unasked_problems(tree(frame=frame, extra=extra)), [])

    def test_a_click_handler_made_in_the_pass_runs_later(self):
        extra = (
            "fn toast(&mut self) { Self::marked_toast(id, m, cx, |this, _, cx| "
            "{ this.open_premium(cx); }) }\n"
            "fn open_premium(&mut self) { self.open_box(DialogKind::X); }"
        )
        self.assertEqual(dh.unasked_problems(tree("self.toast(cx);", extra=extra)), [])

    def test_a_marked_open_passes(self):
        extra = (
            "fn drain(&mut self) {\n"
            "    // box-host: shows in the window of the last press, by design\n"
            "    self.open_box(DialogKind::X);\n}"
        )
        self.assertEqual(dh.unasked_problems(tree("self.drain(cx);", extra=extra)), [])

    def test_a_root_called_outside_unasked_fails(self):
        callers = "fn a(&mut self) { self.poll_live(cx); }\n"
        found = dh.unasked_problems(tree(callers=callers))
        self.assertEqual(len(found), 1)
        self.assertIn("unasked", found[0])

    def test_an_open_in_a_screen_task_fails(self):
        extra = (
            "fn pick(&mut self) { self.spawn_screen(cx, async move |this, cx| { "
            "this.update(cx, |this, cx| this.show(cx)); }).detach(); }\n"
            "fn show(&mut self) { self.open_box(DialogKind::X); }"
        )
        found = dh.unasked_problems(tree(extra=extra))
        self.assertEqual(len(found), 1)
        self.assertIn("spawn_screen at src/ui/extra.rs:1 > show", found[0])

    def test_a_screen_task_that_carries_its_window_passes(self):
        extra = (
            "fn pick(&mut self) { let from = self.asking_window().flatten(); "
            "self.spawn_screen(cx, async move |this, cx| { this.update(cx, |this, cx| "
            "this.as_asked_by(from, |this| this.show(cx))); }).detach(); }\n"
            "fn show(&mut self) { self.open_box(DialogKind::X); }"
        )
        self.assertEqual(dh.unasked_problems(tree(extra=extra)), [])


if __name__ == "__main__":
    unittest.main()

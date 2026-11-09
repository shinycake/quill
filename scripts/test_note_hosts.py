"""Offline unit tests for scripts/note_hosts.py: a toast takes its window
from the Asker (run: python3 -m unittest discover -s scripts -p
'test_note_hosts.py'). scripts/check-window-scope.py runs the real check
on the tree."""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import note_hosts as nh  # noqa: E402


def problems(files):
    with tempfile.TemporaryDirectory() as root:
        for rel, text in files.items():
            path = os.path.join(root, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(text)
        return nh.problems(root)


class NoteHosts(unittest.TestCase):
    def test_a_hand_named_toast_window_fails(self):
        found = problems(
            {"src/ui/bots.rs": "fn a(&mut self) { self.connection.set_note_in(w, note); }\n"}
        )
        self.assertEqual(len(found), 1)
        self.assertIn("src/ui/bots.rs:1", found[0])

    def test_a_marked_one_passes(self):
        found = problems(
            {
                "src/ui/forward.rs": (
                    "// note-host: the forward's own window\n"
                    "fn a(&mut self) { self.connection.set_note_in(w, note); }\n"
                )
            }
        )
        self.assertEqual(found, [])

    def test_the_seam_may_name_it(self):
        found = problems(
            {"src/ui/connection_state.rs": "fn set_note_for() { self.set_note_in(h, n); self.toasts.push_in(n, h, t); }\n"}
        )
        self.assertEqual(found, [])

    def test_the_last_press_past_the_asker_fails_outside_chat_window(self):
        found = problems(
            {
                "src/ui/send_fly.rs": "let w = self.windows.last_press;\n",
                "src/ui/chat_window/dialog_hosts.rs": "let w = self.last_press; self.windows.last_press;\n",
            }
        )
        self.assertEqual(len(found), 1)
        self.assertIn("src/ui/send_fly.rs:1", found[0])

    def test_set_note_passes(self):
        found = problems({"src/ui/bots.rs": "fn a(&mut self) { self.set_note(note); }\n"})
        self.assertEqual(found, [])

    def test_tests_are_skipped(self):
        found = problems(
            {"src/ui/bots_tests.rs": "fn a() { app.connection.set_note_in(None, note); }\n"}
        )
        self.assertEqual(found, [])


if __name__ == "__main__":
    unittest.main()

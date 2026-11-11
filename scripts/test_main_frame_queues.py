"""Offline unit tests for scripts/main_frame_queues.py: a queue only the
main window's frame drains brings that window forward (run: python3 -m
unittest discover -s scripts -p 'test_main_frame_queues.py').
scripts/check-window-scope.py runs the real check on the tree."""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import main_frame_queues as mfq  # noqa: E402


def problems(files):
    with tempfile.TemporaryDirectory() as root:
        for rel, text in files.items():
            path = os.path.join(root, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(text)
        return mfq.problems(root)


class MainFrameQueues(unittest.TestCase):
    def test_a_bare_resolve_outside_the_seam_fails(self):
        found = problems(
            {"src/ui/premium_ui/a.rs": "fn a(&mut self) {\n    live.driver.resolve_deep_link(action);\n}\n"}
        )
        self.assertEqual(len(found), 1)
        self.assertIn("src/ui/premium_ui/a.rs:2", found[0])

    def test_the_seam_may_resolve(self):
        found = problems(
            {"src/ui/deep_links.rs": "fn a(&mut self) {\n    live.driver.resolve_deep_link(action);\n}\n"}
        )
        self.assertEqual(found, [])

    def test_a_queue_with_no_raise_fails(self):
        found = problems(
            {"src/ui/web_app_ui/a.rs": "fn a(&mut self) {\n    self.mini_apps.render_action = Some(x);\n}\n"}
        )
        self.assertEqual(len(found), 1)
        self.assertIn("src/ui/web_app_ui/a.rs:2", found[0])

    def test_a_raise_after_it_passes(self):
        found = problems(
            {
                "src/ui/a.rs": (
                    "fn a(&mut self, cx: &mut Context<Self>) {\n"
                    "    self.links.pending_deep_link_open = Some(chat);\n"
                    "    self.raise_main_window_later(cx);\n"
                    "}\n"
                )
            }
        )
        self.assertEqual(found, [])

    def test_a_marked_one_passes(self):
        found = problems(
            {
                "src/ui/a.rs": (
                    "// main-frame: the main window asked, so it is drawing\n"
                    "self.mini_apps.render_action = Some(x);\n"
                )
            }
        )
        self.assertEqual(found, [])

    def test_tests_are_skipped(self):
        found = problems({"src/ui/a_tests.rs": "self.mini_apps.render_action = Some(x);\n"})
        self.assertEqual(found, [])


if __name__ == "__main__":
    unittest.main()

"""Offline unit tests for the one-writer check in scripts/single_writers.py
(run: python3 -m unittest discover -s scripts -p 'test_single_writers.py').
scripts/check-screen-state.py runs the real check on the tree."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import single_writers as sw  # noqa: E402

STATE = "src/ui/chat_ui_state.rs"
ENTRY = [(1, "chat_ui", STATE, "replace_chat_ui")]
WRITER = "fn replace_chat_ui(&mut self, next: ChatUi) { std::mem::replace(&mut *self.screen.chat_ui, next); }\n"


def check(other):
    return sw.writer_problems({STATE: WRITER, "src/ui/other.rs": other}, ENTRY)


class OneWriter(unittest.TestCase):
    def test_the_writer_may_replace(self):
        self.assertEqual(check("fn read(&self) { let _ = self.chat_ui.chat; }"), [])

    def test_an_assignment_elsewhere_fails(self):
        found = check("fn enter(&mut self) { *self.screen.chat_ui = ChatUi::default(); }")
        self.assertEqual(len(found), 1)
        self.assertIn("replace_chat_ui", found[0])

    def test_a_mem_take_elsewhere_fails(self):
        self.assertEqual(len(check("fn go(&mut self) { std::mem::take(&mut self.screen.chat_ui); }")), 1)

    def test_a_sub_field_write_and_a_compare_pass(self):
        other = "fn go(&mut self) { self.chat_ui.reply = None; if self.chat_ui == x {} }"
        self.assertEqual(check(other), [])

    def test_a_test_module_may_write(self):
        other = "#[cfg(test)]\nmod tests { fn t() { app.chat_ui = ChatUi::default(); } }"
        self.assertEqual(check(other), [])

    def test_a_missing_writer_fails(self):
        found = sw.writer_problems({STATE: "fn other() {}"}, ENTRY)
        self.assertTrue(any("has no `fn replace_chat_ui`" in line for line in found))


class MenuClose(unittest.TestCase):
    def test_a_bare_close_fails(self):
        found = sw.menu_close_problems({"src/ui/x.rs": "fn go(&mut self) { this.message_ui.menu = None; }"})
        self.assertEqual(len(found), 1)

    def test_the_closer_and_a_demo_pass(self):
        files = {
            "src/ui/context_menu_keys.rs": "fn close_message_menu(&mut self) { self.message_ui.menu = None; }",
            "src/ui/demos/m.rs": "fn d(&mut self) { self.message_ui.menu = None; }",
            "src/ui/media_panel/mod.rs": "fn open_reaction_selector(&mut self) { self.message_ui.menu = None; }",
        }
        self.assertEqual(sw.menu_close_problems(files), [])


if __name__ == "__main__":
    unittest.main()

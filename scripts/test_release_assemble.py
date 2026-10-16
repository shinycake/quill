"""Offline unit tests for scripts/release-assemble.py (run: python3 -m unittest discover -s scripts -p 'test_release_assemble.py')."""
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("release_assemble", os.path.join(HERE, "release-assemble.py"))
ra = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ra)


class DefuseMentions(unittest.TestCase):
    def test_bare_username_becomes_code(self):
        self.assertEqual(
            ra.defuse_mentions("- Inline @gif results download as soon as they arrive."),
            "- Inline `@gif` results download as soon as they arrive.",
        )
        self.assertEqual(ra.defuse_mentions("(@vote, @bing)"), "(`@vote`, `@bing`)")

    def test_leaves_code_emails_and_urls(self):
        for text in (
            "Run `@gif` in the composer",
            "mail idan@example.com",
            "https://t.me/@gif and https://github.com/@x",
            "```\n@gif\n```",
        ):
            self.assertEqual(ra.defuse_mentions(text), text)

    def test_keeps_changelog_credit(self):
        line = "* Inline @gif results load at once by @shinycake in https://github.com/shinycake/quill/pull/880"
        self.assertEqual(
            ra.defuse_mentions(line),
            "* Inline `@gif` results load at once by @shinycake in https://github.com/shinycake/quill/pull/880",
        )


if __name__ == "__main__":
    unittest.main()

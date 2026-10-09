"""Offline unit tests for scripts/deps-watch.py (run: python3 -m unittest discover -s scripts -p 'test_deps_watch.py')."""
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("deps_watch", os.path.join(HERE, "deps-watch.py"))
dw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dw)


class Versions(unittest.TestCase):
    def test_vtuple(self):
        self.assertEqual(dw.vtuple("n8.1.3"), (8, 1, 3))
        self.assertEqual(dw.vtuple("v3.0.0"), (3, 0, 0))
        self.assertEqual(dw.vtuple("1.8.68"), (1, 8, 68))
        self.assertGreater(dw.vtuple("1.10.0"), dw.vtuple("1.9.9"))

    def test_breaking(self):
        self.assertTrue(dw.is_breaking_newer("2.0.0", "1.9.9"))
        self.assertFalse(dw.is_breaking_newer("1.9.9", "1.0.1"))
        self.assertTrue(dw.is_breaking_newer("0.4.0", "0.3.9"))
        self.assertFalse(dw.is_breaking_newer("0.3.9", "0.3.1"))
        self.assertTrue(dw.is_breaking_newer("0.0.4", "0.0.3"))
        self.assertTrue(dw.is_breaking_newer("1.0.0", "0.9.9"))


class Cargo(unittest.TestCase):
    def test_parse_update(self):
        text = (
            "    Updating crates.io index\n     Locking 2 packages to latest compatible versions\n"
            "    Updating serde v1.0.1 -> v1.0.2\n    Updating toml v1.1.6+spec-1.1.0 -> v1.1.8+spec-1.1.0\n"
            "      Adding foo v0.1.0\n"
        )
        self.assertEqual(
            dw.parse_cargo_update(text),
            [("serde", "1.0.1", "1.0.2"), ("toml", "1.1.6+spec-1.1.0", "1.1.8+spec-1.1.0")],
        )

    def test_kit_pin(self):
        toml = '[dependencies.gpui-kit]\nversion = "=0.7.0"\noptional = true\n'
        self.assertEqual(dw.kit_pin(toml), "0.7.0")

    def test_direct_deps_and_majors(self):
        meta = {
            "resolve": {"root": "root"},
            "packages": [
                {"id": "root", "name": "quill", "version": "0.1.0", "source": None, "dependencies": [
                    {"name": "serde", "source": "registry+x"},
                    {"name": "local", "source": None},
                    {"name": "gpui-kit", "source": "registry+x"},
                ]},
                {"id": "a", "name": "serde", "version": "1.0.1", "source": "registry+x"},
                {"id": "b", "name": "gpui-kit", "version": "0.7.0", "source": "registry+x"},
            ],
        }
        deps = dw.direct_registry_deps(meta)
        self.assertEqual(deps, {"serde": "1.0.1", "gpui-kit": "0.7.0"})
        self.assertEqual(dw.find_majors(deps, lambda n: "2.0.0"), [("serde", "1.0.1", "2.0.0")])
        self.assertEqual(dw.find_majors(deps, lambda n: "1.5.0"), [])


class Actions(unittest.TestCase):
    def test_parse_uses(self):
        text = (
            "      - uses: actions/checkout@v4\n"
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4, SHA-pinned\n"
            "  uses: org/repo/.github/workflows/p.yml@abc123\n"
            "      - uses: actions/cache/restore@v4\n"
            "      - uses: ./local\n"
        )
        self.assertEqual(
            dw.parse_uses(text),
            [("actions/checkout", "v4", ""),
             ("actions/checkout", "11d5960a326750d5838078e36cf38b85af677262", "v4, SHA-pinned"),
             ("actions/cache", "v4", "")],
        )

    def test_used_major(self):
        self.assertEqual(dw.used_major("v4", ""), 4)
        self.assertEqual(dw.used_major("abc", "v4, SHA-pinned"), 4)
        self.assertIsNone(dw.used_major("master", ""))
        self.assertIsNone(dw.used_major("02cb", "SHA-pinned"))


class Sync(unittest.TestCase):
    def issue(self, key, title="t"):
        return dw.Issue(key, title, "body")

    def existing(self, number, key, title="t", body=None, labels=("deps-watch",)):
        return {"number": number, "title": title, "body": body if body is not None else self.issue(key, title).body,
                "labels": list(labels)}

    def test_create_when_missing(self):
        d = {"crates": self.issue("crates")}
        self.assertEqual(dw.plan_sync([], d, {"crates"}), [("create", d["crates"])])

    def test_unchanged_is_noop(self):
        d = {"crates": self.issue("crates")}
        self.assertEqual(dw.plan_sync([self.existing(5, "crates")], d, {"crates"}), [])

    def test_new_version_updates_same_issue(self):
        d = {"gpui-kit": self.issue("gpui-kit", "gpui-kit 0.7.2 available")}
        acts = dw.plan_sync([self.existing(7, "gpui-kit", "gpui-kit 0.7.1 available")], d, {"gpui-kit"})
        self.assertEqual([a[:2] for a in acts], [("update", 7)])

    def test_close_when_landed_but_not_when_source_failed(self):
        ex = [self.existing(3, "native:ffmpeg"), self.existing(4, "actions")]
        acts = dw.plan_sync(ex, {}, {"native:ffmpeg"})  # actions source failed
        self.assertEqual(acts, [("close", 3, "native:ffmpeg")])

    def test_legacy_tdlib_issue_adopted(self):
        legacy = {"number": 9, "title": "TDLib 1.8.69 available", "body": "old body", "labels": ["tdlib-update"]}
        d = {"tdlib": self.issue("tdlib", "TDLib 1.8.70 available")}
        self.assertEqual([a[:2] for a in dw.plan_sync([legacy], d, {"tdlib"})], [("update", 9)])
        self.assertEqual(dw.plan_sync([legacy], {}, {"tdlib"}), [("close", 9, "tdlib")])

    def test_marker_in_body(self):
        self.assertIn("<!-- deps-watch:crates -->", self.issue("crates").body)


if __name__ == "__main__":
    unittest.main()

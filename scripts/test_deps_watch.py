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

    def test_pick_newest_ignores_creation_order(self):
        rel = lambda tag, **kw: {"tag_name": tag, "html_url": f"u/{tag}", **kw}
        # actions/cache, 2026-06-26: v5.1.0 was published after v6.1.0.
        releases = [rel("v5.1.0"), rel("v6.1.0"), rel("v7.0.0-rc1", prerelease=True), rel("v8", draft=True), rel("nightly")]
        self.assertEqual(dw.pick_newest(releases), ("v6.1.0", "u/v6.1.0"))
        self.assertIsNone(dw.pick_newest([rel("v1", draft=True)]))


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
        reg = "registry+x"
        meta = {
            "workspace_members": ["root", "member"],
            "resolve": {"root": "root", "nodes": [
                {"id": "root", "deps": [{"pkg": "serde1"}, {"pkg": "local"}, {"pkg": "kit"}, {"pkg": "rand08"}]},
                {"id": "member", "deps": [{"pkg": "wry"}]},
                {"id": "tungstenite", "deps": [{"pkg": "rand010"}]},
            ]},
            "packages": [
                {"id": "root", "name": "quill", "version": "0.1.0", "source": None},
                {"id": "member", "name": "webview-host", "version": "0.1.0", "source": None},
                {"id": "local", "name": "local", "version": "0.1.0", "source": None},
                {"id": "serde1", "name": "serde", "version": "1.0.1", "source": reg},
                {"id": "kit", "name": "gpui-kit", "version": "0.7.0", "source": reg},
                {"id": "rand08", "name": "rand", "version": "0.8.5", "source": reg},
                {"id": "rand010", "name": "rand", "version": "0.10.3", "source": reg},
                {"id": "wry", "name": "wry", "version": "0.57.0", "source": reg},
                {"id": "tungstenite", "name": "tungstenite", "version": "0.30.0", "source": reg},
            ],
        }
        deps = dw.direct_registry_deps(meta)
        # rand is the member's own 0.8, not the 0.10 another crate locks.
        self.assertEqual(deps, {"serde": "1.0.1", "gpui-kit": "0.7.0", "rand": "0.8.5", "wry": "0.57.0"})
        latest = {"serde": "2.0.0", "rand": "0.10.3", "wry": "0.57.0"}
        self.assertEqual(
            dw.find_majors(deps, lambda n: latest.get(n, "1.5.0")),
            [("rand", "0.8.5", "0.10.3"), ("serde", "1.0.1", "2.0.0")],
        )

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

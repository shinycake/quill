"""Writes the synthetic golden fixtures trim-paths.json and many-paths.json.

Hand-made stand-ins for real stickers: the same Lottie features (trim paths
on dozens of animated paths; over a hundred animated paths stacked in layers
and a precomp), with no third-party artwork.

    python3 -I tests/fixtures/stickers/generate.py
"""

import json
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent
FRAMES = 60
SIZE = 512


def kf(t, s):
    return {"t": t, "s": s, "i": {"x": [0.4], "y": [1]}, "o": {"x": [0.6], "y": [0]}}


def blob(cx, cy, r, wobble, phase):
    """A closed four-point bezier whose points breathe over the clip."""
    keys = []
    for t in (0, FRAMES // 2, FRAMES):
        k = wobble * math.sin(phase + t / FRAMES * 2 * math.pi)
        pts = [[round(x, 1), round(y, 1)] for x, y in ((cx, cy - r - k), (cx + r + k, cy), (cx, cy + r - k), (cx - r - k, cy))]
        c = round(r * 0.55, 1)
        shape = {
            "c": True,
            "v": pts,
            "i": [[-c, 0], [0, -c], [c, 0], [0, c]],
            "o": [[c, 0], [0, c], [-c, 0], [0, -c]],
        }
        keys.append(kf(t, [shape]))
    return {"ty": "sh", "ks": {"a": 1, "k": keys}}


def color(h):
    r, g, b = (math.sin(h + o) * 0.5 + 0.5 for o in (0, 2.1, 4.2))
    return [round(r, 3), round(g, 3), round(b, 3), 1]


def transform():
    return {
        "ty": "tr",
        "p": {"a": 0, "k": [0, 0]},
        "a": {"a": 0, "k": [0, 0]},
        "s": {"a": 0, "k": [100, 100]},
        "r": {"a": 0, "k": 0},
        "o": {"a": 0, "k": 100},
    }


def layer_ks(rotate=False):
    return {
        "o": {"a": 0, "k": 100},
        "r": {"a": 1, "k": [kf(0, [0]), kf(FRAMES, [360])]} if rotate else {"a": 0, "k": 0},
        "p": {"a": 0, "k": [0, 0, 0]},
        "a": {"a": 0, "k": [0, 0, 0]},
        "s": {"a": 0, "k": [100, 100, 100]},
    }


def shape_layer(ind, groups, rotate=False):
    return {
        "ty": 4,
        "ind": ind,
        "ip": 0,
        "op": FRAMES,
        "st": 0,
        "ks": layer_ks(rotate),
        "shapes": groups,
    }


def trim_paths():
    groups = []
    for i in range(48):
        a = i / 48 * 2 * math.pi
        cx = SIZE / 2 + math.cos(a) * 170
        cy = SIZE / 2 + math.sin(a) * 170
        trim = {
            "ty": "tm",
            "s": {"a": 1, "k": [kf(0, [0]), kf(FRAMES, [60])]},
            "e": {"a": 1, "k": [kf(0, [40]), kf(FRAMES, [100])]},
            "o": {"a": 0, "k": i * 7.5},
            "m": 1,
        }
        stroke = {
            "ty": "st",
            "c": {"a": 0, "k": color(a)},
            "o": {"a": 0, "k": 100},
            "w": {"a": 0, "k": 9},
            "lc": 2,
            "lj": 2,
        }
        groups.append({"ty": "gr", "it": [blob(cx, cy, 34, 10, a), trim, stroke, transform()]})
    return lottie([shape_layer(1, groups)])


def many_paths():
    def ring(ind, count, radius, size, rotate):
        groups = []
        for i in range(count):
            a = i / count * 2 * math.pi
            cx = SIZE / 2 + math.cos(a) * radius
            cy = SIZE / 2 + math.sin(a) * radius
            fill = {"ty": "fl", "c": {"a": 0, "k": color(a + ind)}, "o": {"a": 0, "k": 100}}
            groups.append({"ty": "gr", "it": [blob(cx, cy, size, size / 3, a), fill, transform()]})
        return shape_layer(ind, groups, rotate)

    inner = [ring(1, 24, 60, 18, True), ring(2, 24, 100, 16, False)]
    outer = [
        ring(3, 32, 160, 14, True),
        ring(4, 24, 210, 12, False),
        {
            "ty": 0,
            "ind": 5,
            "refId": "inner",
            "w": SIZE,
            "h": SIZE,
            "ip": 0,
            "op": FRAMES,
            "st": 0,
            "ks": layer_ks(),
        },
    ]
    return lottie(outer, [{"id": "inner", "layers": inner}])


def lottie(layers, assets=()):
    return {
        "tgs": 1,
        "v": "5.5.2",
        "fr": 60,
        "ip": 0,
        "op": FRAMES,
        "w": SIZE,
        "h": SIZE,
        "assets": list(assets),
        "layers": layers,
    }


for name, build in (("trim-paths", trim_paths), ("many-paths", many_paths)):
    (HERE / f"{name}.json").write_text(json.dumps(build(), separators=(",", ":")) + "\n")

#!/usr/bin/env python3
"""Writes docs/screenshots/fixtures/demo-star-burst.tgs, the screenshot
demo's stand-in for the server's reaction burst (live story Stars effect).

An original test fixture: a ring and twelve four-point sparkles flying out
from the centre and fading, 512 px, 60 fps, 1.2 s. Run from the checkout
root: python3 scripts/make-star-burst-fixture.py
"""
import gzip
import json
import math
from pathlib import Path

SIZE = 512
FPS = 60
FRAMES = 72
CENTER = SIZE / 2
# Light colours of different luminance, so the gold tint shows its shading.
COLOURS = [[1, 1, 1, 1], [1, 0.95, 0.7, 1], [0.75, 0.85, 1, 1]]


def static(value):
    return {"a": 0, "k": value}


def keys(*frames, spatial=False):
    """Animated property: (time, value) pairs, eased out. The last key
    only holds the end value."""
    out = []
    for index, (time, value) in enumerate(frames):
        key = {"t": time, "s": value}
        if index + 1 < len(frames):
            if spatial:
                key["i"] = {"x": 0.25, "y": 1}
                key["o"] = {"x": 0.35, "y": 0}
                key["ti"] = [0, 0, 0]
                key["to"] = [0, 0, 0]
            else:
                key["i"] = {"x": [0.25] * len(value), "y": [1] * len(value)}
                key["o"] = {"x": [0.35] * len(value), "y": [0] * len(value)}
        out.append(key)
    return {"a": 1, "k": out}


def transform(position, scale, opacity):
    return {
        "o": opacity,
        "r": static(0),
        "p": position,
        "a": static([0, 0, 0]),
        "s": scale,
    }


def group(items):
    return {
        "ty": "gr",
        "it": items
        + [
            {
                "ty": "tr",
                "p": static([0, 0]),
                "a": static([0, 0]),
                "s": static([100, 100]),
                "r": static(0),
                "o": static(100),
            }
        ],
    }


def layer(index, name, ks, shapes):
    return {
        "ddd": 0,
        "ind": index,
        "ty": 4,
        "nm": name,
        "sr": 1,
        "ks": ks,
        "ao": 0,
        "shapes": shapes,
        "ip": 0,
        "op": FRAMES,
        "st": 0,
        "bm": 0,
    }


def sparkle(radius):
    """A four-point star: outer points on the axes, a narrow waist."""
    vertices = []
    for step in range(8):
        angle = step * math.pi / 4 - math.pi / 2
        reach = radius if step % 2 == 0 else radius * 0.28
        vertices.append([round(math.cos(angle) * reach, 2), round(math.sin(angle) * reach, 2)])
    zero = [[0, 0]] * len(vertices)
    return {"ty": "sh", "ks": static({"i": zero, "o": zero, "v": vertices, "c": True})}


def main():
    layers = []
    ring = layer(
        1,
        "ring",
        transform(
            static([CENTER, CENTER, 0]),
            static([100, 100, 100]),
            keys((0, [90]), (36, [0])),
        ),
        [
            group(
                [
                    {"ty": "el", "p": static([0, 0]), "s": keys((0, [60, 60]), (36, [380, 380]))},
                    {
                        "ty": "st",
                        "c": static([1, 1, 1, 1]),
                        "o": static(100),
                        "w": keys((0, [14]), (36, [2])),
                        "lc": 2,
                        "lj": 2,
                    },
                ]
            )
        ],
    )
    layers.append(ring)
    for index in range(12):
        angle = index * math.pi / 6 + math.pi / 12
        reach = 200 if index % 2 == 0 else 150
        end = [CENTER + math.cos(angle) * reach, CENTER + math.sin(angle) * reach, 0]
        layers.append(
            layer(
                index + 2,
                f"sparkle{index}",
                transform(
                    keys((0, [CENTER, CENTER, 0]), (54, end), spatial=True),
                    keys((0, [0, 0, 100]), (14, [100, 100, 100]), (FRAMES, [20, 20, 100])),
                    keys((0, [100]), (40, [100]), (FRAMES, [0])),
                ),
                [
                    group(
                        [
                            sparkle(34 if index % 2 == 0 else 24),
                            {"ty": "fl", "c": static(COLOURS[index % 3]), "o": static(100), "r": 1},
                        ]
                    )
                ],
            )
        )
    animation = {
        "v": "5.5.2",
        "fr": FPS,
        "ip": 0,
        "op": FRAMES,
        "w": SIZE,
        "h": SIZE,
        "nm": "star burst fixture",
        "ddd": 0,
        "assets": [],
        "layers": layers,
        "tgs": 1,
    }
    path = Path("docs/screenshots/fixtures/demo-star-burst.tgs")
    data = json.dumps(animation, separators=(",", ":")).encode()
    path.write_bytes(gzip.compress(data, mtime=0))
    print(f"{path}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()

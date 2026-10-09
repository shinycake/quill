"""Add a smooth synthetic cursor to a cursor-less screen recording.

Inputs: the raw take (window pixels, 2 per point), the demo's step
log (Unix ms + step), and the recording's start offset. The cursor glides
between hover targets on eased arcs, with motion blur, a press animation
and a soft ripple on clicks. Output frames are piped to ffmpeg.

usage: composite.py take.mp4 steps.log offset_s start_s end_s out.mp4
  offset_s: video time = log time (s since first step) + offset_s
"""
import json
import math
import subprocess
import sys

import numpy as np
from PIL import Image, ImageDraw

take, log_path, offset, start, end, out = sys.argv[1:7]
offset, start, end = float(offset), float(start), float(end)
SCALE = 2.0  # pixels per window point
probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                        "stream=width,height", "-of", "csv=p=0", take], capture_output=True, text=True)
W, H = map(int, probe.stdout.strip().split(","))
# ScreenCaptureKit swaps the traffic lights for a "being recorded" badge;
# paste the real ones (cut from an ordinary capture) back over it.
LIGHTS = np.asarray(Image.open("lights.png").convert("RGB")) if __import__("os").path.exists("lights.png") else None
FPS = 60
REST = (900.0, 58.0)  # where the cursor waits at the loop seam (pt)

# --- steps -> timeline (seconds in video time) --------------------------
steps = []
t0 = None
for line in open(log_path):
    ms, step = line.split(" ", 1)
    t = int(ms) / 1000.0
    t0 = t if t0 is None else t0
    steps.append((t - t0 + offset, step.strip()))

hovers, clicks = [], []
for t, step in steps:
    if step.startswith("m:"):
        x, y = map(float, step[2:].split(","))
        hovers.append((t, x, y))
    elif "," in step and step[0].isdigit():
        x, y = map(float, step.split(","))
        clicks.append((t, x, y))


def ease(u):
    # easeInOutCubic: accelerate, then settle softly on the target.
    u = min(max(u, 0.0), 1.0)
    return 4 * u ** 3 if u < 0.5 else 1 - (-2 * u + 2) ** 3 / 2


# Moves: arrive at each hover target exactly when the app saw the hover.
moves = []
pos = REST
for t, x, y in hovers:
    dist = math.hypot(x - pos[0], y - pos[1])
    dur = min(max(0.42 + dist / 2200.0, 0.5), 0.95)
    moves.append((t - dur, t, pos, (x, y)))
    pos = (x, y)
# The script ends with a hover back to the rest point; if not, drift there.
if pos != REST:
    moves.append((end - 1.1, end - 0.35, pos, REST))


def bezier_point(p0, p1, u):
    # Quadratic arc: control point offset perpendicular to the path.
    mx, my = (p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2
    dx, dy = p1[0] - p0[0], p1[1] - p0[1]
    bend = 0.12
    cx, cy = mx - dy * bend, my + dx * bend
    a = (1 - u) ** 2
    b = 2 * (1 - u) * u
    c = u * u
    return (a * p0[0] + b * cx + c * p1[0], a * p0[1] + b * cy + c * p1[1])


def cursor_at(t):
    cur = REST
    for t_start, t_end, p0, p1 in moves:
        if t < t_start:
            break
        if t >= t_end:
            cur = p1
            continue
        u = ease((t - t_start) / (t_end - t_start))
        return bezier_point(p0, p1, u)
    return cur


def press_scale(t):
    s = 1.0
    for tc, _, _ in clicks:
        d = t - tc
        if -0.06 <= d <= 0.24:
            u = (d + 0.06) / 0.30
            s = min(s, 1 - 0.16 * math.sin(math.pi * u))
    return s


# --- cursor sprite -------------------------------------------------------
sprite = Image.open(sys.argv[7] if len(sys.argv) > 7 else "cursor@4x.png").convert("RGBA")
# The svg viewBox starts at (-6,-4); tip is at (1,1) => (7,5) units of 32x40.
CURSOR_H = 30 * SCALE * 1.15  # ~30 pt tall incl. shadow, slightly enlarged
TIP = (7 / 32, 5 / 40)


def sprite_at(scale):
    h = max(4, int(CURSOR_H * scale))
    w = int(sprite.width * h / sprite.height)
    return sprite.resize((w, h), Image.LANCZOS)


sprite_cache = {}


def overlay(t):
    """Composite parts as (x0, y0, color, alpha) regions for time t."""
    parts = []
    # Ripples, each drawn in its own small box.
    for tc, x, y in clicks:
        d = t - tc
        if 0 <= d <= 0.55:
            u = d / 0.55
            r = (8 + 30 * (1 - (1 - u) ** 3)) * SCALE
            a = int(150 * (1 - u) ** 2)
            size = int(2 * r + 8)
            img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
            c = size / 2
            ImageDraw.Draw(img).ellipse((c - r, c - r, c + r, c + r), outline=(255, 255, 255, a), width=int(2 * SCALE))
            arr = np.asarray(img, dtype=np.float32)
            al = arr[..., 3:4] / 255.0
            parts.append((int(x * SCALE - c), int(y * SCALE - c), arr[..., :3] * al, al))
    # Cursor with motion blur: average sprites across the shutter interval.
    samples = 16
    shutter = 0.6 / FPS
    placed = []
    for i in range(samples):
        ts = t - shutter / 2 + shutter * i / (samples - 1)
        x, y = cursor_at(ts)
        sc = round(press_scale(ts), 3)
        spr = sprite_cache.get(sc)
        if spr is None:
            spr = sprite_cache[sc] = np.asarray(sprite_at(sc), dtype=np.float32)
        sh, sw = spr.shape[:2]
        placed.append((int(round(x * SCALE - TIP[0] * sw)), int(round(y * SCALE - TIP[1] * sh)), spr))
    bx0 = min(p[0] for p in placed); by0 = min(p[1] for p in placed)
    bx1 = max(p[0] + p[2].shape[1] for p in placed); by1 = max(p[1] + p[2].shape[0] for p in placed)
    acc = np.zeros((by1 - by0, bx1 - bx0, 4), dtype=np.float32)
    for px, py, spr in placed:
        sh, sw = spr.shape[:2]
        a = spr[..., 3:4] / 255.0
        reg = acc[py - by0 : py - by0 + sh, px - bx0 : px - bx0 + sw]
        reg[..., :3] += spr[..., :3] * a
        reg[..., 3:4] += a
    acc /= samples
    parts.append((bx0, by0, acc[..., :3], acc[..., 3:4]))
    return parts


def apply(frame, parts):
    for x0, y0, color, alpha in parts:
        h, w = alpha.shape[:2]
        fx0, fy0 = max(x0, 0), max(y0, 0)
        fx1, fy1 = min(x0 + w, W), min(y0 + h, H)
        if fx1 <= fx0 or fy1 <= fy0:
            continue
        c = color[fy0 - y0 : fy1 - y0, fx0 - x0 : fx1 - x0]
        a = alpha[fy0 - y0 : fy1 - y0, fx0 - x0 : fx1 - x0]
        region = frame[fy0:fy1, fx0:fx1].astype(np.float32)
        frame[fy0:fy1, fx0:fx1] = np.clip(c + region * (1 - a), 0, 255).astype(np.uint8)


# --- frame loop ------------------------------------------------------------
dec = subprocess.Popen(
    ["ffmpeg", "-v", "error", "-ss", f"{start:.3f}", "-to", f"{end:.3f}", "-i", take,
     "-f", "rawvideo", "-pix_fmt", "rgb24", "-fps_mode", "cfr", "-r", str(FPS), "-"],
    stdout=subprocess.PIPE,
)
enc = subprocess.Popen(
    ["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}",
     "-r", str(FPS), "-i", "-", "-c:v", "libx264", "-preset", "slow", "-crf", "12",
     "-pix_fmt", "yuv420p", out],
    stdin=subprocess.PIPE,
)
frame_bytes = W * H * 3
i = 0
while True:
    buf = dec.stdout.read(frame_bytes)
    if len(buf) < frame_bytes:
        break
    t = start + i / FPS
    frame = np.frombuffer(buf, dtype=np.uint8).reshape(H, W, 3).copy()
    if LIGHTS is not None:
        frame[: LIGHTS.shape[0], : LIGHTS.shape[1]] = LIGHTS
    apply(frame, overlay(t))
    enc.stdin.write(frame.tobytes())
    i += 1
enc.stdin.close()
enc.wait()
print(json.dumps({"frames": i, "hovers": len(hovers), "clicks": len(clicks)}))

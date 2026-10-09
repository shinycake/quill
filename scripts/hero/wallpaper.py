"""Procedural dusk-mountains wallpaper (original artwork, no external assets)."""
import numpy as np
from PIL import Image, ImageFilter
W, H = 3840, 2160
rng = np.random.default_rng(7)
y = np.linspace(0, 1, H)[:, None]
x = np.linspace(0, 1, W)[None, :]
def lerp(a, b, t): return a + (b - a) * t
# Sky: deep navy -> indigo -> violet -> warm peach at the horizon.
stops = [(0.0, (8, 12, 32)), (0.35, (32, 30, 92)), (0.6, (110, 70, 160)), (0.78, (236, 140, 150)), (0.9, (255, 196, 150))]
sky = np.zeros((H, W, 3))
for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
    m = (y >= t0) & (y <= t1)
    t = np.clip((y - t0) / (t1 - t0), 0, 1)
    for ch in range(3):
        sky[..., ch] = np.where(m, lerp(c0[ch], c1[ch], t), sky[..., ch])
sky[(y > 0.9).repeat(W, 1)] = (255, 196, 150)
# Sun glow.
sx, sy = 0.62, 0.70
d = np.sqrt(((x - sx) * W / H) ** 2 + (y - sy) ** 2)
glow = np.exp(-(d / 0.11) ** 2) * 0.9 + np.exp(-(d / 0.35) ** 2) * 0.35
sky += glow[..., None] * np.array([255, 214, 170]) * 0.6
disk = (d < 0.045)
sky[disk] = sky[disk] * 0.2 + np.array([255, 236, 205]) * 0.8
# Stars in the upper sky.
n = 900
sxs = rng.integers(0, W, n); sys_ = (rng.random(n) ** 2 * H * 0.45).astype(int)
for px, py in zip(sxs, sys_):
    b = rng.uniform(0.3, 1.0) * (1 - py / (H * 0.45))
    sky[py, px] = np.clip(sky[py, px] + 255 * b * 0.8, 0, 255)
img = sky
# Mountain ranges (back to front): smooth ridges from summed sines + noise.
def ridge(base, amp, seed, rough):
    r = np.random.default_rng(seed)
    xs = np.linspace(0, 1, W)
    h = np.zeros(W)
    for k in range(1, 7):
        h += np.sin(xs * np.pi * (k * r.uniform(1.2, 2.4)) + r.uniform(0, 6.28)) / k ** rough
    h = (h - h.min()) / (h.max() - h.min())
    return base - amp * h
layers = [
    (0.80, 0.16, 11, 1.1, (86, 64, 140), 0.55),
    (0.85, 0.15, 23, 1.0, (60, 44, 110), 0.35),
    (0.90, 0.14, 37, 0.9, (38, 28, 76), 0.18),
    (0.96, 0.12, 51, 0.8, (18, 14, 40), 0.0),
]
for base, amp, seed, rough, col, haze in layers:
    top = ridge(base, amp, seed, rough)
    mask = y >= top[None, :]
    # Vertical shading + haze toward the sky color.
    shade = np.clip((y - top[None, :]) * 3, 0, 1)
    c = np.array(col) * (1 - 0.25 * shade[..., None])
    c = c * (1 - haze) + np.array([236, 150, 160]) * haze * (1 - shade[..., None] * 0.6)
    img = np.where(mask[..., None], c, img)
out = Image.fromarray(np.clip(img, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.2))
out.save(__import__("sys").argv[1], quality=95)

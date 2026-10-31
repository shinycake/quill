"use strict";

// On load, a quill writes the headline. The nib follows each letter's
// strokes in the order a hand forms them, and the ink appears under it,
// glossy at first and drying behind the pen. Then the quill flies up to the
// header, curling as it goes, and becomes the feather that completes the
// logo's Q. Until then the logo shows only its ring.
//
// The headline stays real text the whole time. While the quill writes, the
// text is transparent (still selectable and read by screen readers) and a
// canvas over it shows the ink. The strokes come from
// data/headline-strokes.json, which scripts/site-headline-strokes.py traces
// from the headline font. A small script in the page's head hides the text
// before first paint; reduced motion skips all of this.

// The headline quill, drawn upright with the nib at (0, 0) and the feather
// 128 units long. `bow` bends the shaft to the right; the logo's feather has
// a bow of about 9.
function drawFeather(pen, bow) {
  const L = 128;
  const at = (s) => [bow * Math.sin(Math.PI * s) * (0.6 + 0.4 * s), -L * s];
  const tangent = (s) => {
    const a = at(Math.max(s - 1e-3, 0));
    const b = at(Math.min(s + 1e-3, 1));
    const n = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return [(b[0] - a[0]) / n, (b[1] - a[1]) / n];
  };
  // A point `w` out from the shaft on one side, leaning toward the tip.
  const out = (s, w, side, lean) => {
    const c = at(s);
    const t = tangent(s);
    const n = side > 0 ? [-t[1], t[0]] : [t[1], -t[0]];
    return [c[0] + n[0] * w + t[0] * w * lean, c[1] + n[1] * w + t[1] * w * lean];
  };
  // Splits in the vane: the edge steps in sharply, then grows back out.
  const notch = (s, at, depth) => {
    let m = 1;
    for (const c of at) if (s > c && s < c + 0.07) m *= 1 - depth * (1 - (s - c) / 0.07);
    return m;
  };
  const wide = (s) => (s < 0.12 ? 0 : 24 * Math.sin(Math.PI * ((s - 0.12) / 0.88) ** 0.8) ** 0.9 * notch(s, [0.36, 0.6], 0.55));
  const narrow = (s) => (s < 0.2 ? 0 : 12 * Math.sin(Math.PI * ((s - 0.2) / 0.8) ** 0.75) ** 0.9 * notch(s, [0.45, 0.7], 0.6));
  const f = (p) => `${p[0].toFixed(2)} ${p[1].toFixed(2)}`;
  const left = [];
  const right = [];
  for (let s = 0.1; s <= 1.0001; s += 0.008) {
    left.push(f(out(s, wide(s), -1, 0.55)));
    right.push(f(out(s, narrow(s), 1, 0.6)));
  }
  const barbs = [];
  for (let s = 0.16; s < 0.96; s += 0.028) {
    for (const [side, width] of [[-1, wide], [1, narrow]]) {
      const w = width(s);
      if (w > 2) barbs.push(`M${f(out(s, w * 0.12, side, 0.2))} L${f(out(s, w * 0.9, side, 0.5))}`);
    }
  }
  const line = (from, to, fn) => {
    const pts = [];
    for (let i = from; i <= to; i++) pts.push(f(fn(i / 60)));
    return "M" + pts.join(" L");
  };
  const parts = {
    vane: `M${f(at(0.1))} L${left.join(" L")} L${f(at(1))} L${right.reverse().join(" L")} Z`,
    barbs: barbs.join(" "),
    rachis: line(4, 60, at),
    groove: line(14, 57, (s) => out(s, 0.9, 1, 0)),
    nib: `M0 0 L${f(out(0.06, 1.5, -1, 0))} L${f(out(0.06, 1.5, 1, 0))} Z`,
  };
  for (const path of pen.querySelectorAll("[data-part]")) path.setAttribute("d", parts[path.dataset.part]);
}

const ease = (p) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2);
// The minimum-jerk profile: how a hand moves from rest to rest.
const minJerk = (p) => p * p * p * (10 - 15 * p + 6 * p * p);
const mix = (a, b, p) => a + (b - a) * p;
const clamp01 = (v) => Math.max(0, Math.min(1, v));

// One letter's strokes from the data file, scaled to `px` pixels per em:
// flat arrays of nib positions, pen half widths, times and arc lengths.
function scaleStrokes(glyph, px) {
  return glyph.strokes.map((flat) => {
    const n = flat.length / 4;
    const s = { x: new Float32Array(n), y: new Float32Array(n), r: new Float32Array(n), t: new Float32Array(n), arc: new Float32Array(n) };
    for (let i = 0; i < n; i++) {
      s.x[i] = (flat[4 * i] / 1000) * px;
      s.y[i] = (flat[4 * i + 1] / 1000) * px;
      s.r[i] = (flat[4 * i + 2] / 1000) * px;
      s.t[i] = flat[4 * i + 3];
      if (i) s.arc[i] = s.arc[i - 1] + Math.hypot(s.x[i] - s.x[i - 1], s.y[i] - s.y[i - 1]);
    }
    return s;
  });
}

// When the nib reaches arc length `a` along stroke `s`.
function timeAtArc(s, a) {
  const arc = s.arc;
  const n = arc.length;
  if (a <= 0) return s.t[0];
  if (a >= arc[n - 1]) return s.t[n - 1];
  let lo = 0;
  let hi = n - 1;
  while (hi - lo > 1) {
    const m = (lo + hi) >> 1;
    if (arc[m] <= a) lo = m;
    else hi = m;
  }
  return mix(s.t[lo], s.t[hi], (a - arc[lo]) / (arc[hi] - arc[lo]));
}

// When each pixel of a letter's box gets its ink, in ms from the letter's
// start. Each stroke inks a band: its centerline as wide as the pen, with
// square ends. The ink's leading edge is round, with its tip right under the
// nib: a pixel on the centerline appears as the nib reaches it, one at the
// band's edge a pen's half width later. Where strokes cross, the earlier one
// wins. Pixels in no band, like the corners of a square terminal, fill in
// from the band beside them. Pixel (i, j) of the box is at
// (box.x + i + 0.5, box.y + j + 0.5) from the glyph's origin.
function inkTimes(strokes, box, total) {
  const { w, h } = box;
  const when = new Float32Array(w * h).fill(Infinity);
  const SLACK = 0.6;
  const each = (x0, y0, x1, y1, fn) => {
    const i0 = Math.max(0, Math.floor(x0 - box.x));
    const i1 = Math.min(w - 1, Math.ceil(x1 - box.x));
    const j0 = Math.max(0, Math.floor(y0 - box.y));
    const j1 = Math.min(h - 1, Math.ceil(y1 - box.y));
    for (let j = j0; j <= j1; j++) {
      for (let i = i0; i <= i1; i++) fn(j * w + i, box.x + i + 0.5, box.y + j + 0.5);
    }
  };
  for (const s of strokes) {
    const n = s.x.length;
    if (s.arc[n - 1] === 0) {
      // A dab: the ink spreads out from under the nib.
      const R = s.r[n - 1] + 2;
      each(s.x[0] - R, s.y[0] - R, s.x[0] + R, s.y[0] + R, (k, px, py) => {
        const d = Math.hypot(px - s.x[0], py - s.y[0]);
        for (let i = 0; i < n; i++) {
          if (d <= s.r[i] + SLACK) {
            when[k] = Math.min(when[k], s.t[i]);
            break;
          }
        }
      });
      continue;
    }
    for (let i = 0; i < n - 1; i++) {
      const ax = s.x[i];
      const ay = s.y[i];
      const bx = s.x[i + 1];
      const by = s.y[i + 1];
      const abx = bx - ax;
      const aby = by - ay;
      const len = Math.hypot(abx, aby);
      if (!len) continue;
      const lo = i === 0 ? 0 : -0.5;
      const hi = i === n - 2 ? 1 : 1.5;
      const R = Math.max(s.r[i], s.r[i + 1]) + 2;
      each(Math.min(ax, bx) - R, Math.min(ay, by) - R, Math.max(ax, bx) + R, Math.max(ay, by) + R, (k, px, py) => {
        const u = ((px - ax) * abx + (py - ay) * aby) / (len * len);
        const uc = u < 0 ? 0 : u > 1 ? 1 : u;
        const d = Math.hypot(ax + abx * uc - px, ay + aby * uc - py);
        const at = s.arc[i] + uc * len;
        const r = mix(s.r[i], s.r[i + 1], uc);
        if (d <= r + SLACK && u >= lo && u <= hi) {
          const lag = d < r ? r - Math.sqrt(r * r - d * d) : r;
          const t = timeAtArc(s, at + lag);
          if (t < when[k]) when[k] = t;
        }
      });
    }
  }
  // Each pixel outside every band takes its time from the nearest band
  // pixel, found by sweeping the box forward and back and passing nearest
  // sources between neighbours.
  const src = new Int32Array(w * h).fill(-1);
  let any = false;
  for (let k = 0; k < when.length; k++) {
    if (when[k] !== Infinity) {
      src[k] = k;
      any = true;
    }
  }
  if (!any) return when.fill(total);
  const far = (k, from) => {
    const dx = (k % w) - (from % w);
    const dy = ((k / w) | 0) - ((from / w) | 0);
    return dx * dx + dy * dy;
  };
  const take = (k, i, j) => {
    if (i < 0 || j < 0 || i >= w || j >= h) return;
    const from = src[j * w + i];
    if (from >= 0 && (src[k] < 0 || far(k, from) < far(k, src[k]))) src[k] = from;
  };
  for (let pass = 0; pass < 2; pass++) {
    for (let j = 0; j < h; j++) {
      for (let i = 0; i < w; i++) {
        const k = j * w + i;
        take(k, i - 1, j);
        take(k, i - 1, j - 1);
        take(k, i, j - 1);
        take(k, i + 1, j - 1);
      }
    }
    for (let j = h - 1; j >= 0; j--) {
      for (let i = w - 1; i >= 0; i--) {
        const k = j * w + i;
        take(k, i + 1, j);
        take(k, i + 1, j + 1);
        take(k, i, j + 1);
        take(k, i - 1, j + 1);
      }
    }
  }
  for (let k = 0; k < when.length; k++) if (when[k] === Infinity) when[k] = when[src[k]];
  return when;
}

function setupTitleQuill() {
  window.quillWrite = true;
  const root = document.documentElement;
  const title = document.getElementById("hero-title");
  const pen = document.getElementById("quill-pen");
  const reveal = () => root.classList.remove("quill-writing", "quill-awaiting");
  if (!title || !pen || !root.classList.contains("quill-writing")) {
    pen?.remove();
    reveal();
    return;
  }
  const cs = getComputedStyle(title);
  const font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
  // On a slow connection, show the headline rather than keep it hidden for
  // more than about three seconds from the start of the page load.
  let late = false;
  const wait = setTimeout(() => {
    late = true;
    pen.remove();
    reveal();
  }, Math.max(0, 3000 - performance.now()));
  Promise.all([
    fetch("data/headline-strokes.json").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    document.fonts.load(font),
  ])
    .then(([data]) => {
      clearTimeout(wait);
      if (late) return;
      if (!document.fonts.check(font)) throw new Error("headline font missing");
      write(title, pen, data);
    })
    .catch(() => {
      clearTimeout(wait);
      pen.remove();
      for (const c of title.querySelectorAll(".quill-ink")) c.remove();
      reveal();
    });
}

function write(title, pen, data) {
  const root = document.documentElement;
  const cs = getComputedStyle(title);
  const size = parseFloat(cs.fontSize);
  const dpr = Math.min(3, Math.max(1, window.devicePixelRatio || 1));
  const px = size * dpr;
  const fontAt = (s) => `${cs.fontWeight} ${s}px ${cs.fontFamily}`;

  // Where each letter sits, from the laid-out text itself, so wrapping,
  // kerning and letter spacing all match.
  const probe = document.createElement("canvas").getContext("2d");
  probe.font = fontAt(size);
  const m = probe.measureText("Hg");
  const asc = m.fontBoundingBoxAscent;
  const desc = m.fontBoundingBoxDescent;
  const frame = title.getBoundingClientRect();
  const pad = Math.ceil(size * 0.45);
  const letters = [];
  const walker = document.createTreeWalker(title, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  let word = 0;
  let lastWasSpace = true;
  while (walker.nextNode()) {
    const node = walker.currentNode;
    const text = node.textContent;
    for (let i = 0; i < text.length; i++) {
      const ch = text[i];
      if (/\s/.test(ch)) {
        lastWasSpace = true;
        continue;
      }
      if (lastWasSpace) word++;
      lastWasSpace = false;
      range.setStart(node, i);
      range.setEnd(node, i + 1);
      const r = range.getClientRects()[0];
      if (!r) continue;
      letters.push({
        ch,
        word,
        // The glyph's origin on the baseline, in the canvas's CSS pixels.
        x: r.left - frame.left + pad,
        y: r.top - frame.top + pad + (r.height * asc) / (asc + desc),
        line: Math.round(r.top),
        color: getComputedStyle(node.parentElement).color,
      });
    }
  }
  if (!letters.length) throw new Error("no headline text");

  // The ink layers: one for letters that have dried, one redrawn each frame
  // for the wet ones.
  const layer = () => {
    const c = document.createElement("canvas");
    c.className = "quill-ink";
    c.width = Math.ceil((frame.width + 2 * pad) * dpr);
    c.height = Math.ceil((frame.height + 2 * pad) * dpr);
    c.style.width = `${c.width / dpr}px`;
    c.style.height = `${c.height / dpr}px`;
    c.style.left = c.style.top = `${-pad}px`;
    c.setAttribute("aria-hidden", "true");
    title.append(c);
    return c;
  };
  const dry = layer().getContext("2d");
  const wet = layer().getContext("2d");

  // Per character: strokes at this size and the time each pixel inks.
  const shapes = new Map();
  const shapeOf = (ch) => {
    if (shapes.has(ch)) return shapes.get(ch);
    probe.font = fontAt(px);
    const tm = probe.measureText(ch);
    let glyph = data.glyphs[ch];
    if (!glyph) {
      // A character the plans don't cover: one sweep along the x-height,
      // wide enough to ink the whole letter.
      const w = Math.max(tm.width / px, 0.2) * 1000;
      glyph = { ms: 90, strokes: [[0, -260, 900, 0, w, -260, 900, 90]] };
    }
    const strokes = scaleStrokes(glyph, px);
    let x0 = -tm.actualBoundingBoxLeft;
    let x1 = tm.actualBoundingBoxRight;
    let y0 = -tm.actualBoundingBoxAscent;
    let y1 = tm.actualBoundingBoxDescent;
    for (const s of strokes) {
      for (let i = 0; i < s.x.length; i++) {
        x0 = Math.min(x0, s.x[i] - s.r[i]);
        x1 = Math.max(x1, s.x[i] + s.r[i]);
        y0 = Math.min(y0, s.y[i] - s.r[i]);
        y1 = Math.max(y1, s.y[i] + s.r[i]);
      }
    }
    const box = { x: Math.floor(x0) - 3, y: Math.floor(y0) - 3 };
    box.w = Math.ceil(x1) + 3 - box.x;
    box.h = Math.ceil(y1) + 3 - box.y;
    const shape = { glyph, strokes, box, when: null };
    shapes.set(ch, shape);
    return shape;
  };

  // Each letter's coverage, drawn by the browser in the headline font at its
  // exact position, so the finished ink matches the text it hands over to.
  const isDark = (() => {
    const c = cs.color.match(/\d+(\.\d+)?/g).map(Number);
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] > 128;
  })();
  for (const l of letters) {
    const shape = shapeOf(l.ch);
    const { box } = shape;
    const ox = l.x * dpr;
    // Text sits on whole device pixels vertically; match it.
    const oy = Math.round(l.y * dpr);
    l.shape = shape;
    l.px = Math.floor(ox) + box.x;
    l.py = Math.floor(oy) + box.y;
    const c = document.createElement("canvas");
    c.width = box.w;
    c.height = box.h;
    const g = c.getContext("2d", { willReadFrequently: true });
    g.font = fontAt(px);
    g.fillStyle = "#fff";
    g.fillText(l.ch, ox - l.px, oy - l.py);
    const img = g.getImageData(0, 0, box.w, box.h);
    l.cover = new Uint8Array(box.w * box.h);
    for (let k = 0; k < l.cover.length; k++) l.cover[k] = img.data[4 * k + 3];
    l.canvas = c;
    l.ctx = g;
    l.img = img;
    l.rgb = l.color.match(/\d+(\.\d+)?/g).map(Number);
    l.state = "waiting";
  }

  // The timeline. Letters follow one another; between them the pen lifts
  // and travels, a little longer between words, after a comma and to the
  // next line. Every stroke and hop goes into `moves`, so the nib's place at
  // any moment is a lookup.
  const [HOP_BASE, HOP_ROOT] = data.hop;
  const hopMs = (d) => HOP_BASE + HOP_ROOT * Math.sqrt(d / px);
  const INTRO = 700;
  const moves = [];
  const first = letters[0];
  const startOf = (l) => {
    const s = l.shape.strokes[0];
    return [l.x * dpr + s.x[0], l.y * dpr + s.y[0]];
  };
  const endOf = (l) => {
    const s = l.shape.strokes[l.shape.strokes.length - 1];
    const n = s.x.length - 1;
    return [l.x * dpr + s.x[n], l.y * dpr + s.y[n]];
  };
  let t = INTRO;
  letters.forEach((l, i) => {
    const prev = letters[i - 1];
    if (prev) {
      const from = endOf(prev);
      const to = startOf(l);
      const d = Math.hypot(to[0] - from[0], to[1] - from[1]);
      let dur = hopMs(d);
      let lift = 0.25 + 0.35 * clamp01(d / px);
      if (l.line !== prev.line) {
        dur = 364;
        lift = 1;
      } else if (l.word !== prev.word) dur += 33;
      if (prev.ch === ",") dur += 117;
      moves.push({ kind: "hop", t0: t, t1: t + dur, from, to, lift });
      t += dur;
    }
    l.t0 = t;
    const strokes = l.shape.strokes;
    strokes.forEach((s, k) => {
      const n = s.x.length - 1;
      if (k) {
        const p = strokes[k - 1];
        const pn = p.x.length - 1;
        const from = [l.x * dpr + p.x[pn], l.y * dpr + p.y[pn]];
        const to = [l.x * dpr + s.x[0], l.y * dpr + s.y[0]];
        const d = Math.hypot(to[0] - from[0], to[1] - from[1]);
        moves.push({ kind: "hop", t0: t + p.t[pn], t1: t + s.t[0], from, to, lift: 0.15 + 0.3 * clamp01(d / px) });
      }
      moves.push({ kind: "stroke", t0: t + s.t[0], t1: t + s.t[n], s, l });
    });
    t += l.shape.glyph.ms;
    l.t1 = t;
  });
  const WRITTEN = t;
  const DRY = 650; // ms for fresh ink to dry
  const SOFT = 1.5; // ms of soft edge at the ink's leading front
  const RISE = 320;
  const FLIGHT = 1250;
  const END = WRITTEN + RISE + FLIGHT;

  // The nib in canvas device pixels, and how high it rides.
  let cursor = 0;
  function nibAt(now) {
    if (now < INTRO) {
      // Come down onto the first stroke, pause a moment, touch the paper.
      const e = 1 - Math.pow(1 - clamp01(now / (INTRO - 150)), 3);
      const [x, y] = startOf(first);
      return { x: x - 0.5 * px * (1 - e), y: y - 0.1 * px * (1 - e), lift: 2.4 * (1 - e) };
    }
    while (cursor < moves.length - 1 && now >= moves[cursor].t1) cursor++;
    const mv = moves[cursor];
    if (mv.kind === "hop") {
      const p = clamp01((now - mv.t0) / (mv.t1 - mv.t0 || 1));
      const e = minJerk(p);
      return { x: mix(mv.from[0], mv.to[0], e), y: mix(mv.from[1], mv.to[1], e), lift: mv.lift * Math.sin(Math.PI * p) };
    }
    const { s, l } = mv;
    const local = now - l.t0;
    const n = s.t.length - 1;
    let i = 0;
    while (i < n - 1 && s.t[i + 1] <= local) i++;
    const p = n ? clamp01((local - s.t[i]) / (s.t[i + 1] - s.t[i] || 1)) : 0;
    const j = Math.min(i + 1, n);
    return { x: l.x * dpr + mix(s.x[i], s.x[j], p), y: l.y * dpr + mix(s.y[i], s.y[j], p), lift: 0 };
  }

  // Paint one letter's ink as of `now`: present where the nib has been,
  // with a soft leading edge, and fresh ink glossy until it dries.
  function paint(l, now) {
    const shape = l.shape;
    if (!shape.when) shape.when = inkTimes(shape.strokes, shape.box, shape.glyph.ms);
    const when = shape.when;
    const local = now - l.t0;
    const out = l.img.data;
    const [r, g, b] = l.rgb;
    // Fresh ink sits a touch brighter on a dark page and darker on a light one.
    const toward = isDark ? 255 : 0;
    const gloss = isDark ? 0.4 : 0.3;
    for (let k = 0; k < when.length; k++) {
      const a = l.cover[k];
      const o = 4 * k;
      if (!a) {
        out[o + 3] = 0;
        continue;
      }
      const age = local - when[k];
      if (age < -SOFT) {
        out[o + 3] = 0;
        continue;
      }
      const f = gloss * (age <= 0 ? 1 : Math.exp(-age / (DRY / 3)));
      out[o] = mix(r, toward, f);
      out[o + 1] = mix(g, toward, f);
      out[o + 2] = mix(b, toward, f);
      out[o + 3] = age >= 0 ? a : (a * (age + SOFT)) / SOFT;
    }
    l.ctx.putImageData(l.img, 0, 0);
  }

  // The logo shows just its ring until the feather lands in it.
  const brand = document.querySelector(".brand");
  const logo = brand?.querySelector(".brand-full");
  document.body.append(pen);
  const WRITE_BOW = 3;
  const LOGO_BOW = 9;
  drawFeather(pen, WRITE_BOW);

  // The drawing's viewBox is -32 -132 64 136 with the nib at (0, 0), the
  // feather pointing straight up and 128 units long.
  const penBox = pen.getBoundingClientRect();
  const unit = penBox.width / 64;
  const ox = 32 * unit;
  const oy = 132 * unit;
  pen.style.transformOrigin = `${ox}px ${oy}px`;
  const writeScale = (size * 2.3) / (128 * unit);
  const WRITE_ANGLE = 45;
  // In the icon the feather runs from its nib at (712, 850) to (400, 515)
  // of 1024: 458 long, leaning 43° left of upright.
  const LOGO_NIB = [712 / 1024, 850 / 1024];
  const LOGO_LEN = 458 / 1024;
  const LOGO_ANGLE = -42.97;

  const place = (x, y, angle, scale, lift) => {
    pen.style.transform = `translate(${(x - ox).toFixed(2)}px, ${(y - oy).toFixed(2)}px) rotate(${angle.toFixed(2)}deg) scale(${scale.toFixed(4)})`;
    const l = Math.max(0, lift);
    pen.style.filter = `drop-shadow(${(2 + l * 7).toFixed(1)}px ${(3 + l * 12).toFixed(1)}px ${(2 + l * 6).toFixed(1)}px rgba(4, 20, 40, ${(0.32 - l * 0.07).toFixed(3)}))`;
  };

  let done = false;
  let landed = false;
  const start = performance.now() + 150;
  const handOver = () => {
    // The text takes over from the ink, which fades off the top of it.
    root.classList.remove("quill-writing");
    for (const c of title.querySelectorAll(".quill-ink")) {
      c.classList.add("gone");
      c.addEventListener("transitionend", () => c.remove(), { once: true });
    }
  };
  const finish = () => {
    if (done) return;
    done = true;
    window.removeEventListener("resize", resized);
    pen.remove();
    root.classList.remove("quill-awaiting");
    handOver();
  };
  // A new width reflows the headline, so stop and show it. A new height
  // alone, like a phone's address bar folding away on scroll, changes
  // nothing the quill depends on.
  const width = window.innerWidth;
  const resized = () => {
    if (window.innerWidth !== width) finish();
  };
  window.addEventListener("resize", resized);

  // The feather settles into the ring and the full logo takes over.
  const land = () => {
    landed = true;
    root.classList.remove("quill-awaiting");
    brand?.querySelector(".brand-mark")?.animate(
      [{ transform: "scale(1)" }, { transform: "scale(1.12)" }, { transform: "scale(1)" }],
      { duration: 520, easing: "cubic-bezier(.3, .7, .3, 1)" },
    );
  };

  // The hand turns the pen a little with its sideways motion.
  let tilt = 0;
  let last = null;
  let handedOver = false;

  // A mistake in a frame must never leave the headline hidden.
  function tick(clock) {
    try {
      step(clock);
    } catch {
      finish();
    }
  }

  function step(clock) {
    if (done) return;
    const now = clock - start;
    if (now < 0) return requestAnimationFrame(tick);
    if (now >= END + 200) return finish();
    pen.style.opacity = String(Math.min(1, now / 260));

    // Work out one letter's ink timing per frame ahead of the pen, so no
    // frame has to do several.
    for (const shape of shapes.values()) {
      if (!shape.when) {
        shape.when = inkTimes(shape.strokes, shape.box, shape.glyph.ms);
        break;
      }
    }

    // Ink: letters that have dried go onto the dry layer once; the wet ones
    // are repainted every frame.
    wet.clearRect(0, 0, wet.canvas.width, wet.canvas.height);
    for (const l of letters) {
      if (l.state === "dry" || now < l.t0 - SOFT) continue;
      paint(l, now);
      if (now > l.t1 + DRY * 1.5) {
        dry.drawImage(l.canvas, l.px, l.py);
        l.state = "dry";
      } else {
        wet.drawImage(l.canvas, l.px, l.py);
      }
    }

    const box = title.getBoundingClientRect();
    const toScreen = (x, y) => [box.left - pad + x / dpr, box.top - pad + y / dpr];
    if (now < WRITTEN) {
      const nib = nibAt(now);
      const [x, y] = toScreen(nib.x, nib.y);
      if (last) tilt = mix(tilt, Math.max(-6, Math.min(6, (x - last) * 0.5)), 0.12);
      last = x;
      const lift = nib.lift;
      place(x, y - lift * 0.18 * size, WRITE_ANGLE - lift * 5 - tilt, writeScale * (1 + lift * 0.04), lift);
      return requestAnimationFrame(tick);
    }
    if (!handedOver && now > WRITTEN + DRY * 1.5) {
      handedOver = true;
      handOver();
    }

    const [ex, ey] = endOf(letters[letters.length - 1]);
    const [fx, fy] = toScreen(ex, ey);
    if (now < WRITTEN + RISE) {
      // A small flick off the last letter, then the pen rises.
      const p = ease((now - WRITTEN) / RISE);
      place(fx + p * 18, fy - p * 34, WRITE_ANGLE - tilt * (1 - p) - p * 8, writeScale * (1 + p * 0.06), p * 1.6);
      return requestAnimationFrame(tick);
    }

    // Fly to the logo, turning and shrinking to sit exactly on its feather.
    const p = Math.min(1, (now - WRITTEN - RISE) / FLIGHT);
    const e = ease(p);
    const r = logo?.getBoundingClientRect();
    const aloft = { x: fx + 18, y: fy - 34 };
    if (!r || !r.width) {
      pen.style.opacity = String(1 - p);
      place(aloft.x, aloft.y - p * 40, WRITE_ANGLE - 8, writeScale * 1.06, 1.6);
      return requestAnimationFrame(tick);
    }
    const to = { x: r.left + LOGO_NIB[0] * r.width, y: r.top + LOGO_NIB[1] * r.height };
    const ctrl = { x: mix(aloft.x, to.x, 0.35), y: Math.min(aloft.y, to.y) - 90 };
    const x = (1 - e) * (1 - e) * aloft.x + 2 * (1 - e) * e * ctrl.x + e * e * to.x;
    const y = (1 - e) * (1 - e) * aloft.y + 2 * (1 - e) * e * ctrl.y + e * e * to.y;
    const logoScale = (LOGO_LEN * r.width) / (128 * unit);
    const scale = writeScale * 1.06 * Math.pow(logoScale / (writeScale * 1.06), e);
    // The shaft curls past the logo's bend on the way in, then eases back to it.
    drawFeather(pen, mix(WRITE_BOW, LOGO_BOW, e) + 7 * Math.sin(Math.PI * Math.min(1, e * 1.08)));
    place(x, y, mix(WRITE_ANGLE - 8, LOGO_ANGLE, e), scale, 1.6 * (1 - e));
    if (p > 0.9 && !landed) land();
    if (p > 0.9) pen.style.opacity = String(Math.max(0, 1 - (p - 0.9) / 0.1));
    if (p >= 1) return finish();
    requestAnimationFrame(tick);
  }
  requestAnimationFrame(tick);
}

setupTitleQuill();

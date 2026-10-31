"use strict";

// On load, a quill writes the headline by hand in a thin cursive. The nib
// follows each letter's strokes in the order a hand forms them, and the ink
// appears under it, glossy at first and drying behind the pen. As the pen
// lifts off, a wave runs through the handwriting and each letter's ink
// flows into the headline's own letter. Meanwhile the quill flies up to the
// header, curling as it goes, and becomes the feather that completes the
// logo's Q. Until then the logo shows only its ring.
//
// The headline stays real text the whole time. While the quill writes, the
// text is transparent (still selectable and read by screen readers) and
// canvases over it show the ink. The strokes come from
// data/headline-strokes.json, which scripts/site-headline-strokes.py traces
// from the handwriting font. A small script in the page's head hides the
// text before first paint; reduced motion skips all of this.

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

// A glyph's distance field, for the morph: the signed distance
// in pixels from each pixel's center to the edge of a glyph drawn with
// coverage `a` (w by h), negative inside. Exact Euclidean distances between
// pixel centers (Felzenszwalb and Huttenlocher), with the edge pixels'
// coverage placing the edge between them.
function signedDistance(a, w, h) {
  const BIG = 1e20;
  const n = Math.max(w, h);
  const f = new Float64Array(n);
  const d = new Float64Array(n);
  const v = new Int32Array(n);
  const z = new Float64Array(n + 1);
  const line = (len) => {
    let k = 0;
    v[0] = 0;
    z[0] = -BIG;
    z[1] = BIG;
    for (let q = 1; q < len; q++) {
      let s;
      for (;;) {
        const r = v[k];
        s = (f[q] + q * q - (f[r] + r * r)) / (2 * (q - r));
        if (s > z[k]) break;
        k--;
      }
      k++;
      v[k] = q;
      z[k] = s;
      z[k + 1] = BIG;
    }
    k = 0;
    for (let q = 0; q < len; q++) {
      while (z[k + 1] < q) k++;
      const r = v[k];
      d[q] = (q - r) * (q - r) + f[r];
    }
  };
  // Squared distance from each pixel to the nearest one where `site` holds.
  const field = (site) => {
    const g = new Float64Array(w * h);
    for (let k = 0; k < g.length; k++) g[k] = site(a[k]) ? 0 : BIG;
    for (let x = 0; x < w; x++) {
      for (let y = 0; y < h; y++) f[y] = g[y * w + x];
      line(h);
      for (let y = 0; y < h; y++) g[y * w + x] = d[y];
    }
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) f[x] = g[y * w + x];
      line(w);
      for (let x = 0; x < w; x++) g[y * w + x] = d[x];
    }
    return g;
  };
  const toInk = field((c) => c >= 128);
  const toPaper = field((c) => c < 128);
  const out = new Float32Array(w * h);
  for (let k = 0; k < out.length; k++) {
    const c = a[k];
    if (c > 0 && c < 255) out[k] = 0.5 - c / 255;
    else if (c >= 128) out[k] = 0.5 - Math.sqrt(toPaper[k]);
    else out[k] = Math.sqrt(toInk[k]) - 0.5;
  }
  return out;
}

// One glyph drawn into a cell of its own with room around it: its coverage,
// distance field, ink box and where its origin sits in the cell.
function glyphCell(font, ch) {
  const probe = document.createElement("canvas").getContext("2d");
  probe.font = font;
  const tm = probe.measureText(ch);
  const P = 3;
  const ox = Math.ceil(tm.actualBoundingBoxLeft) + P;
  const oy = Math.ceil(tm.actualBoundingBoxAscent) + P;
  const w = Math.max(1, ox + Math.ceil(tm.actualBoundingBoxRight) + P);
  const h = Math.max(1, oy + Math.ceil(tm.actualBoundingBoxDescent) + P);
  return { font, ch, ox, oy, w, h, sdf: null, ink: null };
}

function fillCell(cell) {
  const c = document.createElement("canvas");
  c.width = cell.w;
  c.height = cell.h;
  const g = c.getContext("2d", { willReadFrequently: true });
  g.font = cell.font;
  g.fillStyle = "#fff";
  g.fillText(cell.ch, cell.ox, cell.oy);
  const px = g.getImageData(0, 0, cell.w, cell.h).data;
  const a = new Uint8Array(cell.w * cell.h);
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (let k = 0; k < a.length; k++) {
    a[k] = px[4 * k + 3];
    if (a[k] >= 128) {
      const x = k % cell.w;
      const y = (k / cell.w) | 0;
      x0 = Math.min(x0, x);
      x1 = Math.max(x1, x + 1);
      y0 = Math.min(y0, y);
      y1 = Math.max(y1, y + 1);
    }
  }
  cell.alpha = a;
  cell.sdf = signedDistance(a, cell.w, cell.h);
  // The ink's box around the glyph's origin.
  if (x0 === Infinity) cell.ink = { cx: 0, cy: 0, w: 1, h: 1 };
  else cell.ink = { cx: (x0 + x1) / 2 - cell.ox, cy: (y0 + y1) / 2 - cell.oy, w: x1 - x0, h: y1 - y0 };
}

// A cell's distance at a point given from the glyph's origin. Past the
// cell's edge, the distance from the nearest point inside it is added on.
function sampleCell(cell, x, y) {
  const tx = cell.ox + x - 0.5;
  const ty = cell.oy + y - 0.5;
  const cx = Math.max(0, Math.min(cell.w - 1, tx));
  const cy = Math.max(0, Math.min(cell.h - 1, ty));
  const i = Math.min(cell.w - 2, Math.floor(cx));
  const j = Math.min(cell.h - 2, Math.floor(cy));
  const fx = cx - i;
  const fy = cy - j;
  const s = cell.sdf;
  const k = j * cell.w + i;
  const top = mix(s[k], s[k + 1], fx);
  const bottom = mix(s[k + cell.w], s[k + cell.w + 1], fx);
  return mix(top, bottom, fy) + Math.hypot(tx - cx, ty - cy);
}

// The handwriting turns into the headline's letters on the GPU. Each
// letter's handwritten shape slides and stretches onto its headline letter
// while two distance fields trade places: the headline letter swells out
// from the handwriting's line, and the handwriting outside it is cut back,
// so the ink looks to flow from one shape into the other. Returns null
// without WebGL 2.
function makeMorph(canvas) {
  const gl = canvas.getContext("webgl2", { premultipliedAlpha: true, antialias: false });
  if (!gl) return null;
  const shader = (type, src) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    return s;
  };
  const prog = gl.createProgram();
  gl.attachShader(
    prog,
    shader(
      gl.VERTEX_SHADER,
      `#version 300 es
      in vec2 aUnit;
      uniform vec4 uRect;
      uniform vec2 uSize;
      out vec2 vP;
      void main() {
        vP = uRect.xy + aUnit * uRect.zw;
        vec2 c = vP / uSize * 2.0 - 1.0;
        gl_Position = vec4(c.x, -c.y, 0.0, 1.0);
      }`,
    ),
  );
  gl.attachShader(
    prog,
    shader(
      gl.FRAGMENT_SHADER,
      `#version 300 es
      precision highp float;
      in vec2 vP;
      uniform sampler2D uAtlas;
      uniform vec2 uAtlasSize;
      uniform vec4 uHandCell;
      uniform vec2 uHandOrigin;
      uniform vec4 uTypeCell;
      uniform vec2 uTypeOrigin;
      uniform vec2 uCenter;
      uniform vec2 uStretch;
      uniform vec2 uHandMid;
      uniform vec2 uTypeAt;
      uniform float uCut;
      uniform float uSwell;
      uniform float uK;
      uniform vec3 uColor;
      out vec4 outColor;
      float field(vec4 cell, vec2 t) {
        vec2 c = clamp(t, cell.xy + 0.5, cell.xy + cell.zw - 0.5);
        return texture(uAtlas, c / uAtlasSize).r + length(t - c);
      }
      void main() {
        vec2 q = (vP - uCenter) / uStretch + uHandMid;
        float hand = field(uHandCell, uHandCell.xy + uHandOrigin + q) * min(uStretch.x, uStretch.y);
        float type = field(uTypeCell, uTypeCell.xy + uTypeOrigin + vP - uTypeAt);
        float a = type - uCut;
        float b = hand - uSwell;
        float h = clamp(0.5 - 0.5 * (b - a) / uK, 0.0, 1.0);
        float d = mix(b, a, h) + uK * h * (1.0 - h);
        float cover = clamp(0.5 - d, 0.0, 1.0);
        outColor = vec4(uColor * cover, cover);
      }`,
    ),
  );
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog));
  const u = {};
  for (let i = 0, n = gl.getProgramParameter(prog, gl.ACTIVE_UNIFORMS); i < n; i++) {
    const name = gl.getActiveUniform(prog, i).name;
    u[name] = gl.getUniformLocation(prog, name);
  }
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1]), gl.STATIC_DRAW);
  const at = gl.getAttribLocation(prog, "aUnit");
  gl.enableVertexAttribArray(at);
  gl.vertexAttribPointer(at, 2, gl.FLOAT, false, 0, 0);
  gl.useProgram(prog);
  gl.enable(gl.BLEND);
  // Letters of one colour overlap where the handwriting joins up; the
  // brighter of two coverages, not their sum, keeps the joins even.
  gl.blendEquation(gl.MAX);
  gl.viewport(0, 0, canvas.width, canvas.height);
  gl.uniform2f(u.uSize, canvas.width, canvas.height);

  let atlas = null;
  return {
    // Pack the cells into one texture, a shelf at a time.
    pack(cells) {
      const W = Math.min(gl.getParameter(gl.MAX_TEXTURE_SIZE), 2048);
      let x = 0;
      let y = 0;
      let row = 0;
      for (const c of cells) {
        if (x + c.w > W) {
          x = 0;
          y += row + 1;
          row = 0;
        }
        c.ax = x;
        c.ay = y;
        x += c.w + 1;
        row = Math.max(row, c.h);
      }
      const H = y + row + 1;
      atlas = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, atlas);
      gl.texStorage2D(gl.TEXTURE_2D, 1, gl.R16F, W, H);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.uniform2f(u.uAtlasSize, W, H);
    },
    upload(c) {
      gl.texSubImage2D(gl.TEXTURE_2D, 0, c.ax, c.ay, c.w, c.h, gl.RED, gl.FLOAT, c.sdf);
    },
    clear() {
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    },
    // One letter: `rect` bounds what it can cover, in canvas pixels.
    draw(p) {
      gl.uniform4f(u.uRect, p.rect[0], p.rect[1], p.rect[2], p.rect[3]);
      gl.uniform4f(u.uHandCell, p.hand.ax, p.hand.ay, p.hand.w, p.hand.h);
      gl.uniform2f(u.uHandOrigin, p.hand.ox, p.hand.oy);
      gl.uniform4f(u.uTypeCell, p.type.ax, p.type.ay, p.type.w, p.type.h);
      gl.uniform2f(u.uTypeOrigin, p.type.ox, p.type.oy);
      gl.uniform2f(u.uCenter, p.center[0], p.center[1]);
      gl.uniform2f(u.uStretch, p.stretch[0], p.stretch[1]);
      gl.uniform2f(u.uHandMid, p.hand.ink.cx, p.hand.ink.cy);
      gl.uniform2f(u.uTypeAt, p.typeAt[0], p.typeAt[1]);
      gl.uniform1f(u.uCut, p.cut);
      gl.uniform1f(u.uSwell, p.swell);
      gl.uniform1f(u.uK, p.k);
      gl.uniform3f(u.uColor, p.color[0], p.color[1], p.color[2]);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
    },
  };
}

// The handwriting is set this many times the headline's size, which puts
// its small letters a little under the headline's in height and keeps its
// loops clear of the text below.
const HAND_SCALE = 1.25;
const HAND_FONT = '"Quill Hand"';

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
  const hand = new FontFace("Quill Hand", 'url("fonts/hand.woff2")');
  document.fonts.add(hand);
  Promise.all([
    fetch("data/headline-strokes.json").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    document.fonts.load(font),
    hand.load(),
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
  const handAt = (s) => `400 ${s}px ${HAND_FONT}`;

  // Where each letter of the headline sits, from the laid-out text itself,
  // so wrapping, kerning and letter spacing all match.
  const probe = document.createElement("canvas").getContext("2d");
  probe.font = fontAt(size);
  const m = probe.measureText("Hg");
  const asc = m.fontBoundingBoxAscent;
  const desc = m.fontBoundingBoxDescent;
  const frame = title.getBoundingClientRect();
  // The handwriting's capitals and loops reach well past the headline's
  // lines, so its canvases do too.
  const pad = Math.ceil(size * 0.9);
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
        right: r.right - frame.left + pad,
        line: Math.round(r.top),
        color: getComputedStyle(node.parentElement).color,
      });
    }
  }
  if (!letters.length) throw new Error("no headline text");

  // The handwriting: each line of the headline written out in the hand's
  // own spacing, centred on that line and on its baseline, and small enough
  // that no line runs wider than the headline's box.
  const lines = [];
  for (const l of letters) {
    let line = lines[lines.length - 1];
    if (!line || line.key !== l.line) lines.push((line = { key: l.line, letters: [], text: "", at: [] }));
    const prev = line.letters[line.letters.length - 1];
    if (prev && prev.word !== l.word) line.text += " ";
    line.at.push(line.text.length);
    line.text += l.ch;
    line.letters.push(l);
  }
  probe.font = handAt(size);
  let hs = size * HAND_SCALE;
  for (const line of lines) hs = Math.min(hs, (size * frame.width) / probe.measureText(line.text).width);
  probe.font = handAt(hs);
  for (const line of lines) {
    const first = line.letters[0];
    const last = line.letters[line.letters.length - 1];
    const x0 = (first.x + last.right) / 2 - probe.measureText(line.text).width / 2;
    line.letters.forEach((l, i) => {
      l.hx = x0 + probe.measureText(line.text.slice(0, line.at[i])).width;
      l.hy = l.y;
    });
  }
  const hpx = hs * dpr;

  // The ink layers: one for letters that have dried, one redrawn each frame
  // for the wet ones, and one where the handwriting turns into type.
  const layer = (kind) => {
    const c = document.createElement("canvas");
    c.className = "quill-ink";
    c.width = Math.ceil((frame.width + 2 * pad) * dpr);
    c.height = Math.ceil((frame.height + 2 * pad) * dpr);
    c.style.width = `${c.width / dpr}px`;
    c.style.height = `${c.height / dpr}px`;
    c.style.left = c.style.top = `${-pad}px`;
    c.setAttribute("aria-hidden", "true");
    title.append(c);
    return kind ? c : c.getContext("2d");
  };
  const dry = layer();
  const wet = layer();
  const morphCanvas = layer("gl");
  morphCanvas.style.visibility = "hidden";
  let morph = null;
  try {
    morph = makeMorph(morphCanvas);
  } catch {
    morph = null;
  }

  // Per character: the handwriting's strokes at this size and the time each
  // pixel inks.
  const shapes = new Map();
  const shapeOf = (ch) => {
    if (shapes.has(ch)) return shapes.get(ch);
    probe.font = handAt(hpx);
    const tm = probe.measureText(ch);
    let glyph = data.glyphs[ch];
    if (!glyph) {
      // A character the plans don't cover: one sweep along the x-height,
      // wide enough to ink the whole letter.
      const w = Math.max(tm.width / hpx, 0.2) * 1000;
      glyph = { ms: 90, strokes: [[0, -160, 900, 0, w, -160, 900, 90]] };
    }
    const strokes = scaleStrokes(glyph, hpx);
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

  const isDark = (() => {
    const c = cs.color.match(/\d+(\.\d+)?/g).map(Number);
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] > 128;
  })();
  for (const l of letters) {
    const shape = shapeOf(l.ch);
    const { box } = shape;
    // The handwritten glyph's origin in canvas pixels. Text sits on whole
    // device pixels vertically; so does the handwriting.
    l.ox = l.hx * dpr;
    l.oy = Math.round(l.hy * dpr);
    l.shape = shape;
    l.px = Math.floor(l.ox) + box.x;
    l.py = l.oy + box.y;
    const c = document.createElement("canvas");
    c.width = box.w;
    c.height = box.h;
    const g = c.getContext("2d", { willReadFrequently: true });
    g.font = handAt(hpx);
    g.fillStyle = "#fff";
    g.fillText(l.ch, l.ox - l.px, l.oy - l.py);
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
  const hopMs = (d) => HOP_BASE + HOP_ROOT * Math.sqrt(d / hpx);
  const INTRO = 700;
  const moves = [];
  const first = letters[0];
  const startOf = (l) => {
    const s = l.shape.strokes[0];
    return [l.ox + s.x[0], l.oy + s.y[0]];
  };
  const endOf = (l) => {
    const s = l.shape.strokes[l.shape.strokes.length - 1];
    const n = s.x.length - 1;
    return [l.ox + s.x[n], l.oy + s.y[n]];
  };
  let t = INTRO;
  letters.forEach((l, i) => {
    const prev = letters[i - 1];
    if (prev) {
      const from = endOf(prev);
      const to = startOf(l);
      const d = Math.hypot(to[0] - from[0], to[1] - from[1]);
      let dur = hopMs(d);
      let lift = 0.25 + 0.35 * clamp01(d / hpx);
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
        const from = [l.ox + p.x[pn], l.oy + p.y[pn]];
        const to = [l.ox + s.x[0], l.oy + s.y[0]];
        const d = Math.hypot(to[0] - from[0], to[1] - from[1]);
        moves.push({ kind: "hop", t0: t + p.t[pn], t1: t + s.t[0], from, to, lift: 0.15 + 0.3 * clamp01(d / hpx) });
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
  // The handwriting turns into type in a wave from the first letter to the
  // last, starting as the pen lifts off. Each letter takes MORPH ms.
  const MORPH_AT = WRITTEN + 420;
  const MORPH = 760;
  const WAVE = Math.min(1100, 26 * letters.length);
  letters.forEach((l, i) => {
    l.m0 = MORPH_AT + (letters.length > 1 ? (WAVE * i) / (letters.length - 1) : 0);
  });
  const MORPHED = MORPH_AT + WAVE + MORPH;
  const END = Math.max(WRITTEN + RISE + FLIGHT, MORPHED);

  // The morph's glyph cells: each character once in the hand and once in
  // the headline font. They're drawn and measured a cell per frame while the
  // pen writes.
  const cells = [];
  const pairs = new Map();
  if (morph) {
    for (const ch of new Set(letters.map((l) => l.ch))) {
      const hand = glyphCell(handAt(hpx), ch);
      const type = glyphCell(fontAt(px), ch);
      cells.push(hand, type);
      pairs.set(ch, { hand, type, ready: false });
    }
    morph.pack(cells);
  }
  // Fit the handwritten glyph's ink box onto the typed one's, and how far
  // apart the two shapes are once fitted: the distances the morph has to
  // cut back and swell out by.
  const SMOOTH = Math.max(2, px * 0.035);
  const finishPair = (pair) => {
    const { hand, type } = pair;
    const sx = Math.max(0.35, Math.min(2.5, type.ink.w / hand.ink.w));
    const sy = Math.max(0.35, Math.min(2.5, type.ink.h / hand.ink.h));
    pair.stretch = [sx, sy];
    let cut = 0;
    for (let k = 0; k < hand.alpha.length; k++) {
      if (hand.alpha[k] < 128) continue;
      const x = (k % hand.w) + 0.5 - hand.ox;
      const y = ((k / hand.w) | 0) + 0.5 - hand.oy;
      cut = Math.max(cut, sampleCell(type, (x - hand.ink.cx) * sx + type.ink.cx, (y - hand.ink.cy) * sy + type.ink.cy));
    }
    let swell = 0;
    const s = Math.min(sx, sy);
    for (let k = 0; k < type.alpha.length; k++) {
      if (type.sdf[k] > 1) continue;
      const x = (k % type.w) + 0.5 - type.ox;
      const y = ((k / type.w) | 0) + 0.5 - type.oy;
      swell = Math.max(swell, sampleCell(hand, (x - type.ink.cx) / sx + hand.ink.cx, (y - type.ink.cy) / sy + hand.ink.cy) * s);
    }
    pair.cut = cut + SMOOTH + 1;
    pair.swell = swell + SMOOTH + 1;
    pair.ready = true;
  };
  // Where a letter's handwriting starts out, how far it lies from its typed
  // letter: the cut has to begin past that, or it would clip the
  // handwriting before it has moved.
  const startCut = (l) => {
    const { hand, type } = pairs.get(l.ch);
    const dx = l.ox - l.x * dpr;
    const dy = l.oy - Math.round(l.y * dpr);
    let cut = 0;
    for (let k = 0; k < hand.alpha.length; k++) {
      if (hand.alpha[k] < 128) continue;
      const x = (k % hand.w) + 0.5 - hand.ox + dx;
      const y = ((k / hand.w) | 0) + 0.5 - hand.oy + dy;
      cut = Math.max(cut, sampleCell(type, x, y));
    }
    return cut + SMOOTH + 1;
  };
  const prepareOne = () => {
    for (const c of cells) {
      if (!c.sdf) {
        fillCell(c);
        morph.upload(c);
        return true;
      }
    }
    for (const pair of pairs.values()) {
      if (!pair.ready) {
        finishPair(pair);
        return true;
      }
    }
    for (const l of letters) {
      if (l.cut === undefined) {
        l.cut = startCut(l);
        return true;
      }
    }
    return false;
  };

  // Draw every letter part way to type: `k` runs from 0 (handwriting) to 1.
  function drawMorph(now) {
    morph.clear();
    for (const l of letters) {
      const pair = pairs.get(l.ch);
      const { hand, type } = pair;
      const k = clamp01((now - l.m0) / MORPH);
      // First the handwriting slides and stretches onto the letter...
      const fit = ease(clamp01(k / 0.6));
      // ...while the line swells out to fill the letter and whatever lies
      // outside the letter is cut back.
      const pk = clamp01((k - 0.08) / 0.84);
      const cut = mix(l.cut, pair.cut, fit) * (1 - pk) * (1 - pk);
      const swell = pair.swell * pk * pk * (3 - 2 * pk);
      const typeAt = [l.x * dpr, Math.round(l.y * dpr)];
      const handMid = [l.ox + hand.ink.cx, l.oy + hand.ink.cy];
      const typeMid = [typeAt[0] + type.ink.cx, typeAt[1] + type.ink.cy];
      const center = [mix(handMid[0], typeMid[0], fit), mix(handMid[1], typeMid[1], fit)];
      const stretch = [Math.pow(pair.stretch[0], fit), Math.pow(pair.stretch[1], fit)];
      // Only where both shapes reach can there be ink.
      const hw = (hand.w / 2 + 2) * stretch[0] + swell;
      const hh = (hand.h / 2 + 2) * stretch[1] + swell;
      const hcx = center[0] + (hand.w / 2 - hand.ox - hand.ink.cx) * stretch[0];
      const hcy = center[1] + (hand.h / 2 - hand.oy - hand.ink.cy) * stretch[1];
      const x0 = Math.max(hcx - hw, typeAt[0] - type.ox - cut - 2);
      const x1 = Math.min(hcx + hw, typeAt[0] - type.ox + type.w + cut + 2);
      const y0 = Math.max(hcy - hh, typeAt[1] - type.oy - cut - 2);
      const y1 = Math.min(hcy + hh, typeAt[1] - type.oy + type.h + cut + 2);
      if (x1 <= x0 || y1 <= y0) continue;
      const c = l.rgb;
      morph.draw({
        rect: [x0, y0, x1 - x0, y1 - y0],
        hand,
        type,
        center,
        stretch,
        typeAt,
        cut,
        swell,
        k: SMOOTH,
        color: [c[0] / 255, c[1] / 255, c[2] / 255],
      });
    }
  }

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
    return { x: l.ox + mix(s.x[i], s.x[j], p), y: l.oy + mix(s.y[i], s.y[j], p), lift: 0 };
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
  let morphing = false;
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

    // Work out one letter's ink timing per frame ahead of the pen, then the
    // morph's glyphs, so no frame has to do several.
    let busy = false;
    for (const shape of shapes.values()) {
      if (!shape.when) {
        shape.when = inkTimes(shape.strokes, shape.box, shape.glyph.ms);
        busy = true;
        break;
      }
    }
    if (!busy && morph && !morphing) prepareOne();

    if (now >= MORPH_AT && !morphing) {
      morphing = true;
      if (morph) {
        while (prepareOne());
        // The handwriting moves over to the morph's layer in one frame.
        drawMorph(now);
        morphCanvas.style.visibility = "";
        dry.canvas.remove();
        wet.canvas.remove();
      } else {
        // Without WebGL 2 the handwriting fades into the text instead.
        for (const c of title.querySelectorAll(".quill-ink")) c.style.transitionDuration = "0.6s";
        handedOver = true;
        handOver();
      }
    }
    if (morphing && morph && !handedOver) {
      drawMorph(now);
      if (now >= MORPHED) {
        handedOver = true;
        handOver();
      }
    }

    // Ink: letters that have dried go onto the dry layer once; the wet ones
    // are repainted every frame.
    if (!morphing) {
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
    if (p >= 1) {
      if (!landed) land();
      pen.style.opacity = "0";
      return requestAnimationFrame(tick);
    }
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
    requestAnimationFrame(tick);
  }
  requestAnimationFrame(tick);
}

setupTitleQuill();

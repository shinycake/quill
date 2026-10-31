"use strict";

// Release assets the pipeline publishes (see .github/workflows/release.yml).
// Each platform's card and the hero button open its install instructions.
const PLATFORMS = [
  { id: "mac", label: "macOS", detail: "Apple Silicon · macOS 14+", match: /^quill-macos-aarch64\.zip$/, panel: "mac-install" },
  { id: "win", label: "Windows", detail: "x64 · Windows 10+", match: /^quill-windows-x86_64\.zip$/, panel: "windows-install" },
  { id: "linux", label: "Linux", detail: "x86_64 or arm64 · glibc", match: /^quill-linux-x86_64-bundle\.tar\.gz$/, panel: "linux-install" },
];

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else node.setAttribute(k, v);
  }
  for (const child of children) {
    if (child != null) node.append(child);
  }
  return node;
}

function detectPlatform() {
  const ua = navigator.userAgent;
  const platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
  // Phones and tablets can't run Quill; iPadOS and iOS also report "Mac OS X".
  if (/iPhone|iPad|iPod|Android/i.test(ua) || navigator.maxTouchPoints > 1 && /Mac/i.test(platform)) return null;
  if (/Mac/i.test(platform) || /Mac OS X/.test(ua)) return "mac";
  if (/Win/i.test(platform) || /Windows/.test(ua)) return "win";
  if (/Linux|X11/i.test(platform) || /Linux/.test(ua)) return "linux";
  return null;
}

function formatSize(bytes) {
  if (!bytes) return "";
  return bytes >= 1e6 ? `${(bytes / 1e6).toFixed(1)} MB` : `${Math.round(bytes / 1e3)} KB`;
}

function formatDate(iso) {
  try {
    return new Date(iso).toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
  } catch {
    return iso;
  }
}

async function loadJSON(path) {
  const res = await fetch(path, { cache: "no-cache" });
  if (!res.ok) throw new Error(`${path}: ${res.status}`);
  return res.json();
}

async function renderRelease() {
  let release = null;
  try {
    release = (await loadJSON("data/release.json")).release;
  } catch {
    release = null;
  }
  if (!release || !Array.isArray(release.assets)) return;

  const current = detectPlatform();
  const grid = document.getElementById("platforms");
  let currentAsset = null;
  for (const p of PLATFORMS) {
    const asset = release.assets.find((a) => p.match.test(a.name));
    if (!asset) continue;
    if (p.id === current) currentAsset = { p, asset };
    const card = el(
      "a",
      { class: `plat${p.id === current ? " current" : ""}`, href: `#${p.panel}` },
      el("span", { class: "name" }, p.label),
      el("span", { class: "meta" }, `${p.detail} · ${formatSize(asset.size)}`),
      asset.sha256 ? el("span", { class: "sha", title: "SHA-256" }, `SHA-256 ${asset.sha256}`) : null,
      el("span", { class: "go" }, `Install ${release.tag}`),
    );
    grid.append(card);
  }
  if (!grid.children.length) return;

  document.getElementById("no-release").hidden = true;
  grid.hidden = false;

  const label = document.getElementById("hero-download-label");
  const hero = document.getElementById("hero-download");
  const meta = document.getElementById("hero-meta");
  if (currentAsset) {
    label.textContent = `Install for ${currentAsset.p.label}`;
    hero.href = `#${currentAsset.p.panel}`;
  } else {
    label.textContent = "Download";
  }
  meta.textContent = `${release.tag} · ${formatDate(release.published_at)} · Unofficial. Not affiliated with Telegram.`;

  if (release.body_html) {
    const box = document.getElementById("whatsnew");
    document.getElementById("whatsnew-title").textContent = `What's new in ${release.tag}`;
    // body_html is rendered and sanitized by GitHub's Markdown API at build time.
    document.getElementById("whatsnew-body").innerHTML = release.body_html;
    document.getElementById("whatsnew-link").href = release.html_url;
    box.hidden = false;
  }
}

function setupGallery() {
  const tabs = [...document.querySelectorAll(".tabs [role=tab]")];
  const img = document.getElementById("gallery-img");
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  // Warm the cache so switching tabs doesn't flash.
  const preload = () => tabs.forEach((t) => { new Image().src = `img/${t.dataset.shot}.webp`; });
  if ("requestIdleCallback" in window) requestIdleCallback(preload);
  else setTimeout(preload, 1500);

  function select(tab, focus) {
    for (const t of tabs) {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
    }
    if (focus) tab.focus();
    const swap = () => {
      img.src = `img/${tab.dataset.shot}.webp`;
      img.alt = tab.dataset.alt;
      img.classList.remove("fading");
    };
    if (reduce) swap();
    else {
      img.classList.add("fading");
      setTimeout(swap, 180);
    }
  }
  tabs.forEach((tab, i) => {
    tab.tabIndex = i === 0 ? 0 : -1;
    tab.addEventListener("click", () => select(tab, false));
    tab.addEventListener("keydown", (e) => {
      const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
      if (!step) return;
      e.preventDefault();
      select(tabs[(i + step + tabs.length) % tabs.length], true);
    });
  });
}

// The hero clip only plays while it's on screen, and not at all for people
// who asked for reduced motion (they see the poster frame instead).
function setupHeroVideo() {
  const video = document.getElementById("hero-video");
  if (!video) return;
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)");
  const play = () => { if (!reduce.matches) video.play().catch(() => {}); };
  if (reduce.matches) {
    video.removeAttribute("autoplay");
    video.pause();
  }
  if ("IntersectionObserver" in window) {
    new IntersectionObserver((entries) => {
      for (const entry of entries) entry.isIntersecting ? play() : video.pause();
    }).observe(video);
  }
  reduce.addEventListener("change", () => (reduce.matches ? video.pause() : play()));
}

function setupCopy() {
  for (const button of document.querySelectorAll(".cmd .copy")) {
    const code = button.parentElement.querySelector("code");
    if (!code || !navigator.clipboard) {
      button.hidden = true;
      continue;
    }
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(code.textContent);
        button.textContent = "Copied";
      } catch {
        button.textContent = "Select and copy";
      }
      setTimeout(() => { button.textContent = "Copy"; }, 2000);
    });
  }
}

// One panel per system at a time, the visitor's own first. `root` holds an
// .os-tabs list whose tabs name their panels. Without script every panel
// shows, each under its own heading.
function setupOsTabs(root) {
  const list = root && root.querySelector(".os-tabs");
  if (!list) return null;
  const tabs = [...list.querySelectorAll("[role=tab]")];
  const panelOf = (tab) => document.getElementById(tab.getAttribute("aria-controls"));
  function select(tab, focus) {
    for (const t of tabs) {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      panelOf(t).setAttribute("role", "tabpanel");
      panelOf(t).hidden = !on;
    }
    if (focus) tab.focus();
  }
  tabs.forEach((tab, i) => {
    tab.addEventListener("click", () => select(tab, false));
    tab.addEventListener("keydown", (e) => {
      const to = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: tabs.length - 1 }[e.key];
      if (to === undefined) return;
      e.preventDefault();
      select(tabs[(to + tabs.length) % tabs.length], true);
    });
  });
  root.classList.add("js-tabs");
  list.hidden = false;
  const own = detectPlatform();
  select(tabs.find((t) => t.dataset.os === own) || tabs[0], false);
  return { tabs, select };
}

// The install tabs also follow links that name a panel (#windows-install, a
// platform card, the hero button).
function setupInstallTabs() {
  const install = document.getElementById("install");
  const group = setupOsTabs(install);
  if (!group) return null;
  function fromHash(instant) {
    const tab = group.tabs.find((t) => t.getAttribute("aria-controls") === location.hash.slice(1));
    if (!tab) return false;
    group.select(tab, false);
    install.scrollIntoView({ behavior: instant === true ? "instant" : "auto" });
    return true;
  }
  fromHash();
  window.addEventListener("hashchange", fromHash);
  return fromHash;
}

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

// On load, a quill writes the headline. The nib runs across each letter and
// inks it in as it goes, lifts between words and lines, then flies up to the
// header, curling as it goes, and becomes the feather that completes the
// logo's Q. Until then the logo shows only its ring. Reduced motion, or no
// script, shows the headline and the whole logo as they are.
function setupTitleQuill() {
  const title = document.getElementById("hero-title");
  const pen = document.getElementById("quill-pen");
  if (!title || !pen) return;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    pen.remove();
    return;
  }
  title.setAttribute("aria-label", title.textContent.replace(/\s+/g, " ").trim());
  const letters = [];
  const walker = document.createTreeWalker(title, NodeFilter.SHOW_TEXT);
  const nodes = [];
  while (walker.nextNode()) nodes.push(walker.currentNode);
  for (const node of nodes) {
    const frag = document.createDocumentFragment();
    for (const part of node.textContent.split(/(\s+)/)) {
      if (!part) continue;
      if (/^\s+$/.test(part)) {
        frag.append(" ");
        continue;
      }
      const word = document.createElement("span");
      word.className = "word";
      for (const ch of part) {
        const ink = document.createElement("span");
        ink.className = "ink";
        ink.setAttribute("aria-hidden", "true");
        ink.textContent = ch;
        word.append(ink);
        letters.push(ink);
      }
      frag.append(word);
    }
    node.replaceWith(frag);
  }
  if (!letters.length) return;

  // The logo shows just its ring until the feather lands in it.
  const brand = document.querySelector(".brand");
  const logo = brand?.querySelector(".brand-full");
  brand?.classList.add("awaiting");
  document.body.append(pen);
  const WRITE_BOW = 3;
  const LOGO_BOW = 9;
  drawFeather(pen, WRITE_BOW);

  // The drawing's viewBox is -32 -132 64 136 with the nib at (0, 0), the
  // feather pointing straight up and 128 units long.
  const box = pen.getBoundingClientRect();
  const unit = box.width / 64;
  const ox = 32 * unit;
  const oy = 132 * unit;
  pen.style.transformOrigin = `${ox}px ${oy}px`;
  const fontSize = parseFloat(getComputedStyle(title).fontSize);
  const writeScale = (fontSize * 2.3) / (128 * unit);
  const WRITE_ANGLE = 45;
  // In the icon the feather runs from its nib at (712, 850) to (400, 515)
  // of 1024: 458 long, leaning 43° left of upright.
  const LOGO_NIB = [712 / 1024, 850 / 1024];
  const LOGO_LEN = 458 / 1024;
  const LOGO_ANGLE = -42.97;

  // A seeded wobble, so each visit writes the same hand.
  let seed = 7;
  const rand = () => {
    seed = (seed * 16807) % 2147483647;
    return seed / 2147483647;
  };

  const spots = letters.map((ink) => {
    const r = ink.getBoundingClientRect();
    return {
      l: r.left + scrollX,
      r: r.right + scrollX,
      base: r.bottom + scrollY - r.height * 0.24,
      top: r.top + scrollY,
      ch: ink.textContent,
    };
  });
  const xh = fontSize * 0.42;

  // The timeline: write each letter in turn; between words and lines the
  // pen lifts and travels.
  const INTRO = 750;
  const legs = [];
  let t = INTRO;
  spots.forEach((s, i) => {
    const prev = spots[i - 1];
    if (prev) {
      const newLine = Math.abs(s.top - prev.top) > fontSize * 0.5;
      const newWord = letters[i].parentElement !== letters[i - 1].parentElement;
      if (/[,.]/.test(prev.ch)) t += 140;
      if (newLine || newWord) {
        const d = newLine ? 420 : 70 + rand() * 40;
        legs.push({ kind: "move", t0: t, t1: t + d, from: [prev.r, prev.base], to: [s.l, s.base], lift: newLine ? 1 : 0.3 });
        t += d;
      }
    }
    const w = s.r - s.l;
    const d = (24 + w * 0.95) * (0.85 + rand() * 0.3);
    legs.push({ kind: "ink", t0: t, t1: t + d, i, strokes: Math.max(1, Math.round(w / (fontSize * 0.3))) });
    t += d;
  });
  const WRITTEN = t;
  const RISE = 320;
  const FLIGHT = 1250;
  const END = WRITTEN + RISE + FLIGHT;

  const ease = (p) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2);
  const mix = (a, b, p) => a + (b - a) * p;

  // Where the nib is in page coordinates, and how high it rides.
  let legIndex = 0;
  function nibAt(now) {
    const first = spots[0];
    if (now < INTRO) {
      // Come down onto the first letter, pause a moment, touch the paper.
      const p = Math.min(1, now / (INTRO - 150));
      const e = 1 - Math.pow(1 - p, 3);
      return { x: first.l - 30 * (1 - e), y: first.base - xh * 0.2, lift: 2.4 * (1 - e) };
    }
    while (legIndex < legs.length - 1 && now >= legs[legIndex].t1) legIndex++;
    const leg = legs[legIndex];
    const p = Math.max(0, Math.min(1, (now - leg.t0) / (leg.t1 - leg.t0)));
    if (leg.kind === "move") {
      const e = ease(p);
      return {
        x: mix(leg.from[0], leg.to[0], e),
        y: mix(leg.from[1], leg.to[1], e) - xh * 0.2,
        lift: leg.lift * Math.sin(Math.PI * p),
      };
    }
    const s = spots[leg.i];
    // Up and down strokes across the letter, the way a hand forms it.
    const stroke = 0.5 - 0.5 * Math.cos(2 * Math.PI * leg.strokes * p);
    return { x: mix(s.l, s.r, p), y: s.base - xh * stroke * 0.8, lift: 0, leg, p };
  }

  let inking = -1;
  const inkTo = (i, p) => {
    while (inking < i) {
      if (inking >= 0) dry(letters[inking]);
      inking++;
      letters[inking].classList.add("wet");
    }
    const ink = letters[i];
    const w = ink.getBoundingClientRect().width;
    ink.style.clipPath = `inset(-0.4em ${((1 - p) * w).toFixed(1)}px -0.4em -0.4em)`;
  };
  const dry = (ink) => {
    ink.style.clipPath = "";
    ink.classList.add("on");
    requestAnimationFrame(() => ink.classList.remove("wet"));
  };

  const place = (x, y, angle, scale, lift) => {
    pen.style.transform = `translate(${(x - ox).toFixed(2)}px, ${(y - oy).toFixed(2)}px) rotate(${angle.toFixed(2)}deg) scale(${scale.toFixed(4)})`;
    const l = Math.max(0, lift);
    pen.style.filter = `drop-shadow(${(2 + l * 7).toFixed(1)}px ${(3 + l * 12).toFixed(1)}px ${(2 + l * 6).toFixed(1)}px rgba(4, 20, 40, ${(0.32 - l * 0.07).toFixed(3)}))`;
  };

  let done = false;
  let landed = false;
  const start = performance.now() + 250;
  const finish = () => {
    if (done) return;
    done = true;
    letters.forEach((ink) => {
      ink.style.clipPath = "";
      ink.classList.remove("wet");
      ink.classList.add("on");
    });
    pen.remove();
    brand?.classList.remove("awaiting");
  };
  window.addEventListener("resize", finish, { once: true });

  // The feather settles into the ring and the full logo takes over.
  const land = () => {
    landed = true;
    brand?.classList.remove("awaiting");
    brand?.querySelector(".brand-mark")?.animate(
      [{ transform: "scale(1)" }, { transform: "scale(1.12)" }, { transform: "scale(1)" }],
      { duration: 520, easing: "cubic-bezier(.3, .7, .3, 1)" },
    );
  };

  function frame(clock) {
    if (done) return;
    const now = clock - start;
    if (now < 0) return requestAnimationFrame(frame);
    if (now >= END + 200) return finish();
    pen.style.opacity = String(Math.min(1, now / 280));

    if (now < WRITTEN) {
      const nib = nibAt(now);
      if (nib.leg) inkTo(nib.leg.i, nib.p);
      const wobble = Math.sin(now / 70) * 2.2 + (nib.leg ? Math.sin(nib.p * Math.PI * 2 * nib.leg.strokes) * 2 : 0);
      place(nib.x - scrollX, nib.y - scrollY - nib.lift * 12, WRITE_ANGLE - nib.lift * 5 + wobble, writeScale * (1 + nib.lift * 0.04), nib.lift);
      return requestAnimationFrame(frame);
    }
    if (inking < letters.length) {
      if (inking >= 0) dry(letters[inking]);
      inking = letters.length;
    }

    const last = spots[spots.length - 1];
    const from = { x: last.r - scrollX + 6, y: last.base - scrollY - xh * 0.2 };
    if (now < WRITTEN + RISE) {
      // A small flick off the last letter, then the pen rises.
      const p = ease((now - WRITTEN) / RISE);
      place(from.x + p * 18, from.y - p * 34, WRITE_ANGLE - p * 8, writeScale * (1 + p * 0.06), p * 1.6);
      return requestAnimationFrame(frame);
    }

    // Fly to the logo, turning and shrinking to sit exactly on its feather.
    const p = Math.min(1, (now - WRITTEN - RISE) / FLIGHT);
    const e = ease(p);
    const r = logo?.getBoundingClientRect();
    const aloft = { x: from.x + 18, y: from.y - 34 };
    if (!r || !r.width) {
      pen.style.opacity = String(1 - p);
      place(aloft.x, aloft.y - p * 40, WRITE_ANGLE - 8, writeScale * 1.06, 1.6);
      return requestAnimationFrame(frame);
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
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

setupTitleQuill();
setupCopy();
const showInstallFromHash = setupInstallTabs();
setupOsTabs(document.getElementById("agents"));
setupHeroVideo();
setupGallery();
// The release cards and notes arrive late and push the install
// panels down, so a link straight to one scrolls again once they're in,
// unless the visitor has started scrolling on their own.
let scrolledByHand = false;
for (const type of ["wheel", "touchstart", "keydown"]) {
  window.addEventListener(type, () => { scrolledByHand = true; }, { once: true, passive: true });
}
renderRelease().then(() => {
  if (showInstallFromHash && !scrolledByHand) showInstallFromHash(true);
});

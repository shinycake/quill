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

// One install panel at a time: the visitor's own system first, or the one a
// link (#windows-install, a platform card, the hero button) names. Without
// script every panel shows, each under its own heading.
function setupInstallTabs() {
  const list = document.querySelector(".os-tabs");
  if (!list) return;
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
  const byPanel = (id) => tabs.find((t) => t.getAttribute("aria-controls") === id);
  function fromHash(instant) {
    const tab = byPanel(location.hash.slice(1));
    if (!tab) return false;
    select(tab, false);
    document.getElementById("install").scrollIntoView({ behavior: instant === true ? "instant" : "auto" });
    return true;
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
  document.getElementById("install").classList.add("js-tabs");
  list.hidden = false;
  if (!fromHash()) {
    const own = PLATFORMS.find((p) => p.id === detectPlatform());
    select(byPanel(own ? own.panel : "mac-install"), false);
  }
  window.addEventListener("hashchange", fromHash);
  return fromHash;
}

// On load, a quill writes the headline: each letter inks in as the nib
// passes it. Reduced motion, or no script, shows the headline as it is.
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
    const parts = node.textContent.split(/(\s+)/);
    const frag = document.createDocumentFragment();
    for (const part of parts) {
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

  const box = pen.parentElement;
  const size = pen.getBoundingClientRect().width;
  // The nib's tip sits at (3, 61) in the 64-unit drawing.
  const tipX = (3 / 64) * size;
  const tipY = (61 / 64) * size;
  let spots = [];
  const measure = () => {
    const origin = box.getBoundingClientRect();
    spots = letters.map((ink) => {
      const r = ink.getBoundingClientRect();
      return { x: r.right - origin.left, y: r.bottom - origin.top - r.height * 0.22 };
    });
  };
  measure();

  const step = 30;
  const start = performance.now() + 300;
  let shown = 0;
  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    letters.forEach((ink) => ink.classList.add("on"));
    pen.classList.remove("writing");
    setTimeout(() => pen.remove(), 500);
  };
  window.addEventListener("resize", finish, { once: true });

  function frame(now) {
    if (done) return;
    const t = now - start;
    if (t < 0) return requestAnimationFrame(frame);
    pen.classList.add("writing");
    const target = Math.min(letters.length, Math.floor(t / step) + 1);
    while (shown < target) letters[shown++].classList.add("on");
    // Glide the nib between letters, with a small writing wobble.
    const i = Math.min(letters.length - 1, Math.floor(t / step));
    const next = spots[Math.min(letters.length - 1, i + 1)];
    const here = spots[i];
    const f = (t % step) / step;
    const sameLine = Math.abs(next.y - here.y) < 4;
    const x = sameLine ? here.x + (next.x - here.x) * f : here.x;
    const y = (sameLine ? here.y : here.y) - Math.abs(Math.sin(t / 55)) * 4;
    const tilt = Math.sin(t / 90) * 5;
    pen.style.transform = `translate(${x - tipX}px, ${y - tipY}px) rotate(${tilt}deg)`;
    pen.style.transformOrigin = `${tipX}px ${tipY}px`;
    if (shown >= letters.length) {
      setTimeout(finish, 350);
      return;
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

setupTitleQuill();
setupCopy();
const showInstallFromHash = setupInstallTabs();
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

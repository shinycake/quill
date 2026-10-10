// The trusted shell around a Telegram mini app.
//
// Quill drives this page through `window.__quillShell.command(cmd)` (one of
// the `HostCommand`s of quill-webview-protocol); the page talks back through
// `window.ipc.postMessage` with `{t: "shell", event}` for its own controls and
// `{t: "webapp", event, data}` for everything the bot page posts.
//
// The bot page runs in a sandboxed cross-origin <iframe>. telegram-web-app.js
// detects the frame and uses `window.parent.postMessage(JSON.stringify(
// {eventType, eventData}), '*')`; answers go back as the same JSON through
// `iframe.contentWindow.postMessage`. The shell answers only the events that
// describe itself (viewport, safe area, theme) and relays the bottom, back and
// settings button presses; every other event is Quill's to validate.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);
  const frameWrap = $('frame-wrap');
  const loading = $('loading');
  const header = $('header');
  const titleEl = $('title');
  const subtitleEl = $('subtitle');
  const backButton = $('back');
  const menuButton = $('menu-button');
  const closeButton = $('close');
  const footer = $('footer');
  const buttonsEl = $('buttons');
  const menu = $('menu');
  const popupBackdrop = $('popup-backdrop');
  const popupTitle = $('popup-title');
  const popupMessage = $('popup-message');
  const popupButtons = $('popup-buttons');
  const toastEl = $('toast');

  const FRAME_SANDBOX = 'allow-scripts allow-same-origin allow-forms allow-popups allow-modals allow-storage-access-by-user-activation';
  const FRAME_ALLOW = "camera 'none'; microphone 'none'; geolocation 'none'; payment 'none'; display-capture 'none'; usb 'none'";

  const state = {
    frame: null,
    frameLoaded: false,
    theme: null,
    headerColor: '',
    backgroundColor: '',
    bottomBarColor: '',
    menu: [],
    popup: null,
    main: null,
    secondary: null,
    toastTimer: null,
    pending: [],
  };

  function ipc(message) {
    if (window.ipc && typeof window.ipc.postMessage === 'function') {
      window.ipc.postMessage(JSON.stringify(message));
    }
  }

  function shellEvent(event) {
    ipc({ t: 'shell', event: event });
  }

  function forward(eventType, eventData) {
    let data = '';
    if (eventData !== undefined && eventData !== null) {
      try {
        data = typeof eventData === 'string' ? eventData : JSON.stringify(eventData);
      } catch (e) {
        data = '';
      }
    }
    ipc({ t: 'webapp', event: String(eventType), data: data });
  }

  // Deliver a bridge event to the bot page.
  function emit(eventType, eventData) {
    const frame = state.frame;
    if (!frame || !frame.contentWindow) {
      return;
    }
    if (!state.frameLoaded) {
      if (state.pending.length < 64) {
        state.pending.push([eventType, eventData]);
      }
      return;
    }
    try {
      frame.contentWindow.postMessage(JSON.stringify({ eventType: eventType, eventData: eventData || {} }), '*');
    } catch (e) {
      // The frame is navigating; nothing to do.
    }
  }

  function viewportData() {
    const rect = frameWrap.getBoundingClientRect();
    return { height: Math.round(rect.height), width: Math.round(rect.width), is_state_stable: true, is_expanded: true };
  }

  function safeArea() {
    return { top: 0, bottom: 0, left: 0, right: 0 };
  }

  // Events the shell answers itself: they describe this page, not the account.
  function handleLocally(eventType, eventData) {
    switch (eventType) {
      case 'iframe_ready':
      case 'iframe_will_reload':
        return true;
      case 'web_app_request_viewport':
      case 'web_app_expand':
        emit('viewport_changed', viewportData());
        return true;
      case 'web_app_request_safe_area':
        emit('safe_area_changed', safeArea());
        return true;
      case 'web_app_request_content_safe_area':
        emit('content_safe_area_changed', safeArea());
        return true;
      case 'web_app_request_theme':
        if (state.theme) {
          emit('theme_changed', { theme_params: themeParams() });
        }
        return true;
      default:
        return false;
    }
  }

  window.addEventListener('message', function (event) {
    const frame = state.frame;
    if (!frame || event.source !== frame.contentWindow) {
      return;
    }
    let parsed = event.data;
    if (typeof parsed === 'string') {
      try {
        parsed = JSON.parse(parsed);
      } catch (e) {
        return;
      }
    }
    if (!parsed || typeof parsed !== 'object' || typeof parsed.eventType !== 'string') {
      return;
    }
    const eventType = parsed.eventType;
    const eventData = parsed.eventData;
    if (eventType === 'web_app_ready') {
      hideLoading();
    }
    if (handleLocally(eventType, eventData)) {
      return;
    }
    forward(eventType, eventData);
  });

  function hideLoading() {
    loading.hidden = true;
  }

  // Theme -------------------------------------------------------------------

  function themeParams() {
    const t = state.theme;
    if (!t) {
      return {};
    }
    return {
      bg_color: t.bg_color,
      secondary_bg_color: t.secondary_bg_color,
      header_bg_color: t.header_bg_color,
      bottom_bar_bg_color: t.bottom_bar_bg_color,
      section_bg_color: t.section_bg_color,
      section_separator_color: t.section_separator_color,
      text_color: t.text_color,
      accent_text_color: t.accent_text_color,
      section_header_text_color: t.section_header_text_color,
      subtitle_text_color: t.subtitle_text_color,
      destructive_text_color: t.destructive_text_color,
      hint_color: t.hint_color,
      link_color: t.link_color,
      button_color: t.button_color,
      button_text_color: t.button_text_color,
    };
  }

  function isHex(color) {
    return typeof color === 'string' && /^#[0-9a-fA-F]{6}$/.test(color);
  }

  function applyTheme() {
    const t = state.theme;
    if (!t) {
      return;
    }
    const root = document.documentElement.style;
    const set = (name, value, fallback) => root.setProperty(name, isHex(value) ? value : fallback);
    set('--bg', state.backgroundColor || t.bg_color, '#ffffff');
    set('--secondary-bg', t.secondary_bg_color, '#f1f3f5');
    set('--header-bg', state.headerColor || t.header_bg_color, '#ffffff');
    set('--bottom-bar-bg', state.bottomBarColor || t.bottom_bar_bg_color, '#ffffff');
    set('--section-separator', t.section_separator_color, '#e1e4e8');
    set('--text', t.text_color, '#0f1419');
    set('--hint', t.hint_color, '#6b7280');
    set('--link', t.link_color, '#2563eb');
    set('--accent', t.accent_text_color, '#2563eb');
    set('--button', t.button_color, '#2563eb');
    set('--button-text', t.button_text_color, '#ffffff');
    set('--destructive', t.destructive_text_color, '#dc2626');
    document.documentElement.style.colorScheme = t.color_scheme === 'dark' ? 'dark' : 'light';
  }

  // Frame -------------------------------------------------------------------

  function load(url) {
    if (state.frame) {
      state.frame.remove();
      state.frame = null;
    }
    state.frameLoaded = false;
    state.pending = [];
    loading.hidden = false;
    const frame = document.createElement('iframe');
    frame.setAttribute('sandbox', FRAME_SANDBOX);
    frame.setAttribute('allow', FRAME_ALLOW);
    frame.setAttribute('referrerpolicy', 'strict-origin-when-cross-origin');
    frame.setAttribute('title', 'Mini app');
    frame.addEventListener('load', function () {
      state.frameLoaded = true;
      const queued = state.pending;
      state.pending = [];
      queued.forEach((item) => emit(item[0], item[1]));
      shellEvent({ kind: 'frame_loaded' });
      // Pages that never call web_app_ready still get to show.
      setTimeout(hideLoading, 1500);
    });
    frame.src = url;
    frameWrap.appendChild(frame);
    state.frame = frame;
  }

  function reload() {
    if (state.frame) {
      load(state.frame.src);
    }
  }

  // Chrome ------------------------------------------------------------------

  function setTitle(title) {
    titleEl.textContent = title || '';
    document.title = title || 'Mini app';
  }

  function renderMenu() {
    menu.replaceChildren();
    state.menu.forEach((item, index) => {
      if (item.id === '-') {
        const sep = document.createElement('div');
        sep.className = 'menu-separator';
        menu.appendChild(sep);
        return;
      }
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'menu-item' + (item.attention ? ' attention' : '');
      button.setAttribute('role', 'menuitem');
      button.textContent = item.label;
      button.dataset.index = String(index);
      button.addEventListener('click', function () {
        closeMenu();
        if (item.id === 'settings') {
          emit('settings_button_pressed', {});
          return;
        }
        if (item.id === 'reload') {
          reload();
        }
        shellEvent({ kind: 'menu', id: item.id });
      });
      menu.appendChild(button);
    });
  }

  function openMenu() {
    if (!state.menu.length) {
      return;
    }
    renderMenu();
    menu.hidden = false;
    menuButton.setAttribute('aria-expanded', 'true');
    const first = menu.querySelector('.menu-item');
    if (first) {
      first.focus();
    }
  }

  function closeMenu() {
    menu.hidden = true;
    menuButton.setAttribute('aria-expanded', 'false');
  }

  menuButton.addEventListener('click', function () {
    if (menu.hidden) {
      openMenu();
    } else {
      closeMenu();
    }
  });

  document.addEventListener('mousedown', function (event) {
    if (!menu.hidden && !menu.contains(event.target) && event.target !== menuButton && !menuButton.contains(event.target)) {
      closeMenu();
    }
  });

  closeButton.addEventListener('click', function () {
    shellEvent({ kind: 'close', force: false });
  });

  backButton.addEventListener('click', function () {
    emit('back_button_pressed', {});
  });

  // Bottom buttons ----------------------------------------------------------

  function makeBottomButton(spec, id, eventName) {
    const button = document.createElement('button');
    button.type = 'button';
    button.id = id;
    button.className = 'bottom-button' + (spec.progress ? ' progress' : '') + (spec.shine ? ' shine' : '');
    button.disabled = !spec.active || !!spec.progress;
    if (isHex(spec.color)) {
      button.style.background = spec.color;
    }
    if (isHex(spec.text_color)) {
      button.style.color = spec.text_color;
    }
    const label = document.createElement('span');
    label.className = 'label';
    label.textContent = spec.text || '';
    button.appendChild(label);
    button.addEventListener('click', function () {
      if (!button.disabled) {
        emit(eventName, {});
      }
    });
    return button;
  }

  function renderButtons() {
    buttonsEl.replaceChildren();
    const main = state.main && state.main.visible ? state.main : null;
    const secondary = state.secondary && state.secondary.visible ? state.secondary : null;
    if (!main && !secondary) {
      footer.hidden = true;
      requestAnimationFrame(() => emit('viewport_changed', viewportData()));
      return;
    }
    const position = secondary ? (secondary.position || 'left') : 'left';
    buttonsEl.className = position === 'top' || position === 'bottom' ? 'vertical' : '';
    const mainButton = main ? makeBottomButton(main, 'main-button', 'main_button_pressed') : null;
    const secondaryButton = secondary ? makeBottomButton(secondary, 'secondary-button', 'secondary_button_pressed') : null;
    const order = position === 'left' || position === 'top'
      ? [secondaryButton, mainButton]
      : [mainButton, secondaryButton];
    order.forEach((button) => button && buttonsEl.appendChild(button));
    footer.hidden = false;
    requestAnimationFrame(() => emit('viewport_changed', viewportData()));
  }

  // Popups ------------------------------------------------------------------

  function showPopup(popup) {
    state.popup = popup;
    popupTitle.textContent = popup.title || '';
    popupMessage.textContent = popup.message || '';
    popupButtons.replaceChildren();
    (popup.buttons || []).forEach((spec) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'popup-button' + (spec.kind === 'destructive' ? ' destructive' : '');
      button.textContent = spec.text;
      button.addEventListener('click', () => closePopup(spec.id));
      popupButtons.appendChild(button);
    });
    popupBackdrop.hidden = false;
    const first = popupButtons.querySelector('.popup-button:last-child');
    if (first) {
      first.focus();
    }
  }

  function closePopup(buttonId) {
    const popup = state.popup;
    if (!popup) {
      return;
    }
    state.popup = null;
    popupBackdrop.hidden = true;
    shellEvent({ kind: 'popup_closed', id: popup.id, button: buttonId || '' });
  }

  popupBackdrop.addEventListener('mousedown', function (event) {
    if (event.target === popupBackdrop) {
      closePopup('');
    }
  });

  document.addEventListener('keydown', function (event) {
    if (event.key !== 'Escape') {
      return;
    }
    if (state.popup) {
      closePopup('');
    } else if (!menu.hidden) {
      closeMenu();
      menuButton.focus();
    } else {
      shellEvent({ kind: 'close', force: false });
    }
  });

  // Toast -------------------------------------------------------------------

  function toast(text) {
    toastEl.textContent = text;
    toastEl.hidden = false;
    clearTimeout(state.toastTimer);
    state.toastTimer = setTimeout(() => { toastEl.hidden = true; }, 2800);
  }

  // Commands from Quill -------------------------------------------------------

  function command(cmd) {
    if (!cmd || typeof cmd !== 'object') {
      return;
    }
    switch (cmd.kind) {
      case 'load':
        state.theme = cmd.theme || state.theme;
        state.headerColor = '';
        state.backgroundColor = '';
        state.bottomBarColor = '';
        applyTheme();
        setTitle(cmd.title);
        state.menu = Array.isArray(cmd.menu) ? cmd.menu : [];
        load(cmd.url);
        break;
      case 'set_title':
        setTitle(cmd.title);
        break;
      case 'set_theme':
        state.theme = cmd.theme;
        applyTheme();
        emit('theme_changed', { theme_params: themeParams() });
        break;
      case 'set_menu':
        state.menu = Array.isArray(cmd.menu) ? cmd.menu : [];
        if (!menu.hidden) {
          renderMenu();
        }
        break;
      case 'main_button':
        state.main = cmd.button;
        renderButtons();
        break;
      case 'secondary_button':
        state.secondary = cmd.button;
        renderButtons();
        break;
      case 'back_button':
        backButton.hidden = !cmd.visible;
        break;
      case 'header_color':
        state.headerColor = isHex(cmd.color) ? cmd.color : '';
        applyTheme();
        break;
      case 'background_color':
        state.backgroundColor = isHex(cmd.color) ? cmd.color : '';
        applyTheme();
        break;
      case 'bottom_bar_color':
        state.bottomBarColor = isHex(cmd.color) ? cmd.color : '';
        applyTheme();
        break;
      case 'closing_confirmation':
        // Quill keeps the flag; the close path asks it first.
        break;
      case 'show_popup':
        showPopup(cmd.popup);
        break;
      case 'emit':
        emit(cmd.event, cmd.data);
        break;
      case 'toast':
        toast(cmd.text);
        break;
      case 'reload':
        reload();
        break;
      default:
        break;
    }
  }

  let resizeTimer = null;
  window.addEventListener('resize', function () {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => emit('viewport_changed', viewportData()), 60);
  });

  window.__quillShell = { command: command };
  subtitleEl.textContent = 'Mini app';
  header.hidden = false;
  ipc({ t: 'shell_ready' });
})();

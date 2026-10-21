// The bridge object of a web proxy carrier (TDLib `webProxyCarrier`),
// injected at document start into every frame the engine lets it reach.
// Its one argument, at the very end, is a JSON object filled in by
// script.rs.
//
// Every frame first loses what a relay page has no use for and an
// untrusted page could abuse: native dialogs, printing, WebRTC (which a
// content policy does not cover), WebTransport and WebAssembly (as
// Telegram Desktop's web proxy view does). The bridge itself exists only
// in the top-level document of the carrier's origin, and every message to
// the helper starts with a token only this closure knows.
(function (config) {
  'use strict';
  var lock = function (name, value) {
    try {
      Object.defineProperty(window, name, {
        value: value,
        writable: false,
        configurable: false,
        enumerable: false,
      });
    } catch (error) {
      // Already locked in this frame.
    }
  };
  lock('alert', function () {});
  lock('confirm', function () {
    return false;
  });
  lock('prompt', function () {
    return null;
  });
  lock('print', function () {});
  lock('onbeforeunload', null);
  [
    'RTCPeerConnection',
    'webkitRTCPeerConnection',
    'RTCDataChannel',
    'RTCRtpSender',
    'RTCRtpReceiver',
    'WebTransport',
    'WebAssembly',
  ].forEach(function (name) {
    lock(name, undefined);
  });

  if (window !== window.top || window.location.origin !== config.origin) {
    return;
  }
  var token = config.token;
  var send = function (body) {
    window.ipc.postMessage(token + body);
  };

  // The page may load from and talk to its path prefix only. A policy in a
  // <meta> only counts inside <head> and only for what loads after it, so
  // it goes in before the parser gets to the page's own markup. The helper
  // also blocks what leaves the prefix natively where the engine allows.
  var addPolicy = function (head) {
    var meta = document.createElement('meta');
    meta.httpEquiv = 'Content-Security-Policy';
    meta.content = config.policy;
    head.insertBefore(meta, head.firstChild);
  };
  try {
    if (document.documentElement) {
      var head = document.head;
      if (!head) {
        head = document.createElement('head');
        document.documentElement.insertBefore(head, document.documentElement.firstChild);
      }
      addPolicy(head);
    } else {
      // WebView2 runs document-created scripts before the root element
      // exists: add the policy the moment the parser makes <head>.
      var observer = new MutationObserver(function () {
        if (document.head) {
          observer.disconnect();
          addPolicy(document.head);
        }
      });
      observer.observe(document, { childList: true, subtree: true });
    }
  } catch (error) {
    send('f');
    return;
  }

  var encode = function (bytes) {
    var parts = [];
    for (var i = 0; i < bytes.length; i += 32768) {
      parts.push(String.fromCharCode.apply(null, bytes.subarray(i, i + 32768)));
    }
    return btoa(parts.join(''));
  };
  var decode = function (text) {
    var binary = atob(text);
    var bytes = new Uint8Array(binary.length);
    for (var i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes.buffer;
  };

  var receiver = null;
  var announced = false;
  var bridge = {};
  Object.defineProperty(bridge, 'postMessage', {
    enumerable: true,
    value: function (value) {
      if (value instanceof ArrayBuffer) {
        send('b' + encode(new Uint8Array(value)));
      } else if (ArrayBuffer.isView(value)) {
        send('b' + encode(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)));
      } else if (typeof value === 'string') {
        send('t' + value);
      } else {
        throw new TypeError('postMessage takes an ArrayBuffer, a typed array or a string');
      }
    },
  });
  Object.defineProperty(bridge, 'onmessage', {
    enumerable: true,
    get: function () {
      return receiver;
    },
    set: function (value) {
      receiver = typeof value === 'function' ? value : null;
      // The page's own script is running: deliveries may start.
      if (receiver && !announced) {
        announced = true;
        send('o');
      }
    },
  });
  Object.defineProperty(window, config.name, {
    value: Object.freeze(bridge),
    writable: false,
    configurable: false,
    enumerable: false,
  });

  // What the helper calls. Its name is random per carrier, and every call
  // carries a key the page never sees, so the page cannot fake an
  // acknowledgement or a sign of life by calling it.
  var hook = {
    deliver: function (key, sequence, kind, value) {
      if (key !== config.key) {
        return;
      }
      try {
        if (typeof receiver !== 'function') {
          throw new Error('no onmessage');
        }
        receiver({ data: kind === 'b' ? decode(value) : value });
        send('a' + sequence);
      } catch (error) {
        send('f');
      }
    },
    probe: function (key) {
      if (key === config.key) {
        send('h');
      }
    },
  };
  Object.defineProperty(window, config.hook, {
    value: Object.freeze(hook),
    writable: false,
    configurable: false,
    enumerable: false,
  });
  send('h');
})(__CONFIG__);

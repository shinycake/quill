// The bridge object of a web proxy carrier (TDLib `webProxyCarrier`),
// injected at document start. The placeholders are filled by carrier.rs
// with validated values. Defined only in the top-level document of the
// carrier's origin; every message to the helper starts with a token that
// only this closure knows, so no other frame can speak for the page.
(function () {
  'use strict';
  if (window !== window.top || window.location.origin !== __ORIGIN__) {
    return;
  }
  var token = __TOKEN__;
  var send = function (body) {
    window.ipc.postMessage(token + body);
  };

  // The page may load from and talk to its path prefix only. A policy in a
  // <meta> only counts inside <head> and only for what loads after it, so
  // it goes in before the parser gets to the page's own markup.
  var policy = __POLICY__;
  var addPolicy = function (head) {
    var meta = document.createElement('meta');
    meta.httpEquiv = 'Content-Security-Policy';
    meta.content = policy;
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
    },
  });
  Object.defineProperty(window, __NAME__, {
    value: Object.freeze(bridge),
    writable: false,
    configurable: false,
    enumerable: false,
  });

  // What the helper calls; its name is random per carrier.
  var deliver = function (sequence, kind, value) {
    try {
      if (typeof receiver !== 'function') {
        throw new Error('no onmessage');
      }
      receiver({ data: kind === 'b' ? decode(value) : value });
      send('a' + sequence);
    } catch (error) {
      send('f');
    }
  };
  Object.defineProperty(window, __DELIVER__, {
    value: Object.freeze({
      deliver: deliver,
      probe: function () {
        send('h');
      },
    }),
    writable: false,
    configurable: false,
    enumerable: false,
  });
  send('h');
})();

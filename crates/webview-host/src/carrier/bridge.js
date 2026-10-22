// The bridge object of a web proxy carrier (TDLib `webProxyCarrier`),
// injected at document start into every frame the engine lets it reach.
// Its one argument, at the very end, is a JSON object filled in by
// script.rs.
//
// Every frame, and every worker the page starts, first loses what a relay
// page has no use for and an untrusted page could abuse: native dialogs,
// printing, WebRTC (which a content policy does not cover), WebTransport
// and WebAssembly (as Telegram Desktop's web proxy view does), and service
// workers. The bridge itself exists only
// in the top-level document of the carrier's origin, and every message to
// the helper starts with a token only this closure knows.
(function (config) {
  'use strict';
  var REMOVED = [
    'RTCPeerConnection',
    'webkitRTCPeerConnection',
    'RTCDataChannel',
    'RTCRtpSender',
    'RTCRtpReceiver',
    'WebTransport',
    'WebAssembly',
  ];
  var lockIn = function (scope, name, value) {
    try {
      Object.defineProperty(scope, name, {
        value: value,
        writable: false,
        configurable: false,
        enumerable: false,
      });
    } catch (error) {
      // Already locked there, or another origin's window.
    }
  };

  // What runs first in every worker this page starts: the same removals,
  // and no workers of its own. Engines run the bridge script in documents
  // only, so a worker is started from a small script of ours that then
  // loads the page's own.
  var workerPrelude =
    '(function(){var l=function(n){try{Object.defineProperty(self,n,{value:undefined,' +
    'writable:false,configurable:false,enumerable:false})}catch(e){}};' +
    JSON.stringify(REMOVED.concat(['Worker', 'SharedWorker'])) +
    '.forEach(l)})();';
  var wrapWorker = function (scope, name) {
    var Original = scope[name];
    if (typeof Original !== 'function') {
      return;
    }
    var Wrapped = function (url, options) {
      if (!new.target) {
        throw new TypeError(name + ' must be called with new');
      }
      var target = new scope.URL(String(url), scope.document.baseURI).href;
      var load =
        options && options.type === 'module'
          ? 'await import(' + JSON.stringify(target) + ');'
          : 'importScripts(' + JSON.stringify(target) + ');';
      var blob = new scope.Blob([workerPrelude + load], { type: 'text/javascript' });
      return Reflect.construct(Original, [scope.URL.createObjectURL(blob), options], new.target);
    };
    Wrapped.prototype = Original.prototype;
    lockIn(scope, name, Wrapped);
  };

  // Strip one window: no native dialogs or printing, none of the removed
  // interfaces, workers only through the prelude, no service workers.
  var stripped = new WeakSet();
  var strip = function (scope) {
    try {
      if (stripped.has(scope)) {
        return scope;
      }
      stripped.add(scope);
      lockIn(scope, 'alert', function () {});
      lockIn(scope, 'confirm', function () {
        return false;
      });
      lockIn(scope, 'prompt', function () {
        return null;
      });
      lockIn(scope, 'print', function () {});
      lockIn(scope, 'onbeforeunload', null);
      REMOVED.forEach(function (name) {
        lockIn(scope, name, undefined);
      });
      wrapWorker(scope, 'Worker');
      wrapWorker(scope, 'SharedWorker');
      if (scope.navigator) {
        lockIn(scope.navigator, 'serviceWorker', undefined);
      }
      guardFrames(scope);
    } catch (error) {
      // Another origin's window: the engine keeps it apart anyway.
    }
    return scope;
  };

  // A frame's window reached through its element is stripped before the
  // page gets it. Engines inject this script into frames too, but not
  // always into the empty document a new frame starts with.
  var guardFrames = function (scope) {
    ['HTMLIFrameElement', 'HTMLFrameElement', 'HTMLObjectElement'].forEach(function (type) {
      var proto = scope[type] && scope[type].prototype;
      if (!proto) {
        return;
      }
      try {
        guardFrame(proto);
      } catch (error) {
        // Already guarded by this window's own copy of the script.
      }
    });
  };
  var guardFrame = function (proto) {
    var windowOf = Object.getOwnPropertyDescriptor(proto, 'contentWindow');
    var documentOf = Object.getOwnPropertyDescriptor(proto, 'contentDocument');
    if (windowOf && windowOf.get) {
      Object.defineProperty(proto, 'contentWindow', {
        configurable: false,
        enumerable: windowOf.enumerable,
        get: function () {
          var child = windowOf.get.call(this);
          return child ? strip(child) : child;
        },
      });
    }
    if (documentOf && documentOf.get) {
      Object.defineProperty(proto, 'contentDocument', {
        configurable: false,
        enumerable: documentOf.enumerable,
        get: function () {
          var child = documentOf.get.call(this);
          if (child && child.defaultView) {
            strip(child.defaultView);
          }
          return child;
        },
      });
    }
  };

  strip(window);

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

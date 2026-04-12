// Tauri bridge — synchronous, zero-network, zero-import.
//
// Previously this file did `await import('https://esm.sh/@tauri-apps/api@2/...')`,
// which (a) requires network access inside the Tauri webview and (b) is
// async — so sibling modules like app.mjs could start rendering and
// dispatch `api()` calls before `window.__mn` was populated, causing
// every request to silently fall through to `fetch()` and fail. The
// symptom was an empty sidebar with "No sessions yet" on first launch
// and the `h1` title still showing because `isTauri()` returned false.
//
// The fix: don't import from esm.sh at all. Tauri 2 injects
// `window.__TAURI_INTERNALS__` synchronously at webview startup, before
// any of our scripts run. That object exposes `invoke`, `transformCallback`,
// and the plumbing we need to reimplement `Channel`, `listen`, and `emit`
// in ~30 lines here with no external dependencies.
//
// Tauri-2.x-specific: if a future major version renames the internals,
// update this file to match. The public `@tauri-apps/api/core` surface
// we mimic (invoke/Channel/listen/emit/convertFileSrc) should stay the
// same.

if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
  const internals = window.__TAURI_INTERNALS__;

  // Minimal re-implementation of `@tauri-apps/api/core` Channel.
  // The Rust side receives `__CHANNEL__:<id>` (because of toJSON) and
  // wires it to a `tauri::ipc::Channel<T>` parameter; every time the
  // command does `channel.send(event)` on the Rust side, the JS callback
  // stored in `onmessage` fires with the deserialized payload.
  class Channel {
    constructor() {
      this.__onmessage = () => {};
      this.id = internals.transformCallback((msg) => this.__onmessage(msg));
    }
    set onmessage(cb) { this.__onmessage = typeof cb === 'function' ? cb : () => {}; }
    get onmessage() { return this.__onmessage; }
    toJSON() { return `__CHANNEL__:${this.id}`; }
  }

  // Subscribe to a Tauri app event (emitted from Rust via
  // `app.emit(event, payload)`). Returns an async `unlisten` function.
  async function listen(event, handler) {
    const handlerId = internals.transformCallback((e) => handler(e));
    const eventId = await internals.invoke('plugin:event|listen', {
      event,
      target: { kind: 'Any' },
      handler: handlerId,
    });
    return async () => {
      try {
        await internals.invoke('plugin:event|unlisten', { event, eventId });
      } catch {}
    };
  }

  async function emit(event, payload) {
    return internals.invoke('plugin:event|emit', { event, payload });
  }

  // Build a URL the webview can load directly (img/audio/video src,
  // <link href>) for a local filesystem path. Uses Tauri's asset
  // protocol so the webview bypasses CORS / same-origin for files that
  // the Rust side has already validated.
  function convertFileSrc(filePath, protocol = 'asset') {
    const path = encodeURIComponent(filePath);
    // Windows uses a pseudo-HTTPS scheme; macOS/Linux use the custom scheme.
    return navigator.userAgent.includes('Windows')
      ? `https://${protocol}.localhost/${path}`
      : `${protocol}://localhost/${path}`;
  }

  window.__mn = {
    invoke: (cmd, args) => internals.invoke(cmd, args),
    Channel,
    listen,
    emit,
    convertFileSrc,
  };

  // Signal readiness — anyone listening for the bridge to come up can
  // hook this. utils.mjs doesn't need it (it imports this file directly,
  // so `window.__mn` is guaranteed live before `isTauri()` is ever called),
  // but third-party scripts that want to wait for the bridge can use it.
  try {
    window.dispatchEvent(new CustomEvent('mn:tauri-ready'));
  } catch {}
}

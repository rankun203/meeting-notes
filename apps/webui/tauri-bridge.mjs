// Tauri bridge — loaded only when running inside the VoiceRecords desktop
// webview. Exposes `invoke`, `listen`, `emit`, `convertFileSrc`, and
// `Channel` on `window.__mn` so the rest of the webui (which is plain ES
// modules, no bundler) can pick them up via `utils.mjs` without caring
// which transport it's on.
//
// When this file is loaded inside a regular browser (daemon-served mode),
// `window.__TAURI_INTERNALS__` is undefined and we no-op — the utils.mjs
// `api()` helper falls back to `fetch('/api/...')`.

if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
  try {
    const core = await import('https://esm.sh/@tauri-apps/api@2/core');
    const event = await import('https://esm.sh/@tauri-apps/api@2/event');
    window.__mn = {
      invoke: core.invoke,
      convertFileSrc: core.convertFileSrc,
      Channel: core.Channel,
      listen: event.listen,
      emit: event.emit,
    };
    // Broadcast readiness so utils.mjs can wait on it if loaded first.
    window.dispatchEvent(new CustomEvent('mn:tauri-ready'));
  } catch (err) {
    console.error('Failed to load Tauri API bindings:', err);
  }
}

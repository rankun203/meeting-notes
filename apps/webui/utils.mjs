import { useState, useEffect, useRef, useCallback } from 'react';
import { jsx as _jsx, jsxs as _jsxs, Fragment } from 'react/jsx-runtime';
// Import tauri-bridge as a module dependency so `window.__mn` is guaranteed
// populated (in Tauri webview) before any component that uses `api()` or
// `isTauri()` runs. The bridge module is a no-op in a regular browser.
import './tauri-bridge.mjs';

// ── JSX wrappers ──
// Extract 'key' from props to avoid React 19 warning,
// and filter falsy children (jsx-runtime doesn't do this unlike JSX transpilation).

export function jsx(type, props, key) {
  const { key: k, ...rest } = props;
  return _jsx(type, rest, key ?? k);
}

export function jsxs(type, props, key) {
  const { key: k, ...rest } = props;
  const finalKey = key ?? k;
  if (rest.children && Array.isArray(rest.children)) {
    const filtered = rest.children.filter(c => c != null && c !== false);
    if (filtered.length === 0) {
      const { children, ...noChildren } = rest;
      return _jsx(type, noChildren, finalKey);
    }
    if (filtered.length === 1) {
      return _jsx(type, { ...rest, children: filtered[0] }, finalKey);
    }
    return _jsxs(type, { ...rest, children: filtered }, finalKey);
  }
  return _jsxs(type, rest, finalKey);
}

export { Fragment };

// ── Constants ──

export const API = window.location.origin + '/api';
export const PAGE_SIZE = 50;

export const INPUT_CLS = 'w-full rounded-md border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 px-2.5 py-1.5 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-blue-500 transition-colors';
export const LABEL_CLS = 'block text-[11px] font-medium text-gray-500 dark:text-gray-400 mb-0.5 uppercase tracking-wider';

export const SPEAKER_COLORS = [
  'bg-violet-100 text-violet-700 dark:bg-violet-900/30 dark:text-violet-300',
  'bg-cyan-100 text-cyan-700 dark:bg-cyan-900/30 dark:text-cyan-300',
  'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300',
  'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300',
  'bg-rose-100 text-rose-700 dark:bg-rose-900/30 dark:text-rose-300',
  'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300',
  'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-300',
  'bg-teal-100 text-teal-700 dark:bg-teal-900/30 dark:text-teal-300',
];

export const PROCESSING_LABELS = {
  starting: 'Starting...',
  uploading: 'Uploading audio...',
  extracting: 'Transcribing...',
  matching: 'Matching speakers...',
  summarizing: 'Generating summary...',
};

// ── API helper ──
//
// Works in two modes:
//   - Browser / daemon-served: calls `fetch(${origin}/api${path})`.
//   - Inside the VoiceRecords Tauri webview: routes the REST shape to the
//     matching `mn_*` Tauri command. The Tauri bridge sets up
//     `window.__mn.invoke`; detection is feature-based, not UA-based.
//
// Every existing component keeps calling `api('/sessions', {...})` with no
// changes. The router lives in `./api-router.mjs`.

import { routeToCommand } from './api-router.mjs';

export function isTauri() {
  return typeof window !== 'undefined' && !!window.__mn && typeof window.__mn.invoke === 'function';
}

export async function api(path, opts = {}) {
  if (isTauri()) {
    const method = (opts.method || 'GET').toUpperCase();
    let body;
    if (opts.body != null) {
      try { body = JSON.parse(opts.body); } catch { body = opts.body; }
    }
    const route = routeToCommand(method, path, body);
    if (route) {
      try {
        return await window.__mn.invoke(route.name, route.args);
      } catch (e) {
        // Tauri serializes ServiceError as {kind, message}; surface the message.
        const msg = (e && typeof e === 'object' && 'message' in e) ? e.message : (e?.toString?.() || 'command failed');
        throw new Error(msg);
      }
    }
    // Fall through to fetch for unrouted paths (should not happen once the
    // table is complete — logged to help diagnose any misses).
    console.warn(`[api] no Tauri route for ${method} ${path}, falling back to fetch`);
  }
  const res = await fetch(`${API}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...opts,
  });
  if (!res.ok && res.status !== 204) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error || `HTTP ${res.status}`);
  }
  if (res.status === 204) return null;
  return res.json();
}

// ── Streaming helpers ──
//
// Two streaming endpoints need special handling because they return SSE
// events in daemon mode and typed ChatEvent/ClaudeStreamEvent via a
// `tauri::ipc::Channel` in desktop mode.

/** Open `POST /conversations/{id}/messages` as a streaming source.
 *  `onEvent(event)` fires for every typed event (`{type, ...}`).
 *  Returns an `abort()` function the caller can call to stop listening. */
export function apiSendMessage(conversationId, { content, mentions }, onEvent) {
  if (isTauri()) {
    const channel = new window.__mn.Channel();
    channel.onmessage = onEvent;
    window.__mn
      .invoke('mn_send_message', {
        id: conversationId,
        input: { content, mentions: mentions || [] },
        onEvent: channel,
      })
      .catch((e) => {
        const msg = (e && typeof e === 'object' && 'message' in e) ? e.message : String(e);
        onEvent({ type: 'error', error: msg });
      });
    return () => { try { channel.onmessage = () => {}; } catch {} };
  }

  // Browser mode — SSE via fetch.
  const ctrl = new AbortController();
  (async () => {
    try {
      const res = await fetch(`${API}/conversations/${conversationId}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content, mentions: mentions || [] }),
        signal: ctrl.signal,
      });
      if (!res.ok || !res.body) {
        const body = await res.json().catch(() => ({}));
        onEvent({ type: 'error', error: body.error || `HTTP ${res.status}` });
        return;
      }
      await consumeSse(res.body, onEvent);
    } catch (e) {
      if (e?.name !== 'AbortError') {
        onEvent({ type: 'error', error: String(e?.message || e) });
      }
    }
  })();
  return () => ctrl.abort();
}

/** Open `POST /claude/send` as a streaming source. */
export function apiClaudeSend({ prompt, session_id, mentions }, onEvent) {
  if (isTauri()) {
    const channel = new window.__mn.Channel();
    channel.onmessage = onEvent;
    window.__mn
      .invoke('mn_claude_send', {
        input: { prompt, session_id, mentions: mentions || [] },
        onEvent: channel,
      })
      .catch((e) => {
        const msg = (e && typeof e === 'object' && 'message' in e) ? e.message : String(e);
        onEvent({ type: 'error', error: msg });
      });
    return () => { try { channel.onmessage = () => {}; } catch {} };
  }

  const ctrl = new AbortController();
  (async () => {
    try {
      const res = await fetch(`${API}/claude/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt, session_id, mentions: mentions || [] }),
        signal: ctrl.signal,
      });
      if (!res.ok || !res.body) {
        const body = await res.json().catch(() => ({}));
        onEvent({ type: 'error', error: body.error || `HTTP ${res.status}` });
        return;
      }
      await consumeSse(res.body, onEvent);
    } catch (e) {
      if (e?.name !== 'AbortError') {
        onEvent({ type: 'error', error: String(e?.message || e) });
      }
    }
  })();
  return () => ctrl.abort();
}

// Minimal SSE parser — the existing app code already tolerates the shape
// `{type, ...payload}` for each event, so we project from the wire format
// (which has `event: name\ndata: {json}`) into that shape.
async function consumeSse(stream, onEvent) {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let idx;
    while ((idx = buffer.indexOf('\n\n')) >= 0) {
      const frame = buffer.slice(0, idx);
      buffer = buffer.slice(idx + 2);
      let name = 'message';
      let data = '';
      for (const line of frame.split('\n')) {
        if (line.startsWith('event:')) name = line.slice(6).trim();
        else if (line.startsWith('data:')) data += line.slice(5).trim();
      }
      if (!data) continue;
      try {
        const parsed = JSON.parse(data);
        onEvent({ type: name, ...parsed });
      } catch {
        onEvent({ type: name, data });
      }
    }
  }
}

// ── Hooks ──

/// Auto-resize a textarea to fit content, up to maxRows lines.
/// Call on the input event: onInput={autoResize} or wrap existing handler.
///
/// Subtleties we handle:
///   - After `style.height = 'auto'`, scrollHeight can lag one frame
///     unless we force a layout flush; reading `offsetHeight` does that.
///   - `getComputedStyle(el).lineHeight` is often `"normal"`, which
///     parseInt() turns into NaN. Derive a real px value from font-size
///     in that case so an empty textarea has a sensible minimum height.
///   - Minimum = one line + vertical padding, so empty textareas still
///     render as a proper input rather than collapsing to 0 px tall.
///   - Maximum = maxRows lines + padding, with scrollbar above that.
export function autoResize(e, maxRows = 8) {
  const el = e.target || e;
  if (!el) return;
  el.style.height = 'auto';
  // Force a reflow so scrollHeight reflects the fresh 'auto' layout.
  // eslint-disable-next-line no-unused-expressions
  void el.offsetHeight;

  const cs = getComputedStyle(el);
  let lineHeight = parseFloat(cs.lineHeight);
  if (!Number.isFinite(lineHeight)) {
    const fontSize = parseFloat(cs.fontSize) || 14;
    lineHeight = fontSize * 1.4;
  }
  const padY = (parseFloat(cs.paddingTop) || 0) + (parseFloat(cs.paddingBottom) || 0);
  const border = (parseFloat(cs.borderTopWidth) || 0) + (parseFloat(cs.borderBottomWidth) || 0);

  const minHeight = lineHeight + padY + border;
  const maxHeight = lineHeight * maxRows + padY + border;
  const target = Math.max(minHeight, Math.min(el.scrollHeight + border, maxHeight));

  el.style.height = `${Math.round(target)}px`;
  el.style.overflowY = el.scrollHeight + border > maxHeight ? 'auto' : 'hidden';
}

/// Deferred variant of autoResize() for ref callbacks on first mount.
///
/// On the initial app render the element's font metrics may not be
/// final yet (webfont still loading, CSS just applied, Tauri webview
/// in particular takes an extra tick to settle), which makes
/// scrollHeight return a stale "one collapsed line" value. We see the
/// symptom on the notes textarea in the first session that auto-mounts
/// and nowhere else (switching sessions later works because fonts are
/// already resolved by then).
///
/// Fix: wait for document.fonts.ready AND the next animation frame
/// before measuring, so layout is guaranteed to be stable. Also run a
/// second pass on the NEXT frame after that as a belt-and-suspenders
/// against any residual layout churn (cheap — one extra measure).
export function autoResizeDeferred(el, maxRows = 8) {
  if (!el) return;
  const run = () => autoResize({ target: el }, maxRows);
  const schedule = () => {
    requestAnimationFrame(() => {
      run();
      requestAnimationFrame(run);
    });
  };
  if (typeof document !== 'undefined' && document.fonts && document.fonts.status !== 'loaded') {
    document.fonts.ready.then(schedule);
  } else {
    schedule();
  }
}

/// Trigger auto-resize on a textarea ref (for initial sizing).
export function autoResizeRef(ref, maxRows = 8) {
  if (ref.current) autoResize(ref.current, maxRows);
}

/// Strip basic markdown formatting (bold, italic, code) for plain-text display.
export function stripMd(s) {
  return s.replace(/\*\*(.+?)\*\*/g, '$1').replace(/__(.+?)__/g, '$1')
    .replace(/\*(.+?)\*/g, '$1').replace(/_(.+?)_/g, '$1')
    .replace(/`(.+?)`/g, '$1');
}

export function useIsMobile() {
  const [mobile, setMobile] = useState(window.innerWidth < 768);
  useEffect(() => {
    const mq = window.matchMedia('(max-width: 767px)');
    const handler = (e) => setMobile(e.matches);
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, []);
  return mobile;
}

export function useWebSocket(onEvent) {
  const wsRef = useRef(null);
  const reconnectRef = useRef(null);
  const unlistenRef = useRef(null);
  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  useEffect(() => {
    let cancelled = false;

    async function subscribeTauri() {
      // Ask the backend for an initial snapshot (same shape the daemon's
      // WS sends in its first `init` message), then subscribe to the
      // live event stream that the Tauri backend re-emits from the
      // SessionManager broadcast bus.
      try {
        const page = await window.__mn.invoke('mn_list_sessions', { limit: 1000, offset: 0 });
        if (!cancelled) {
          onEventRef.current({
            type: 'init',
            data: { sessions: page.sessions || [], total: page.total || 0 },
          });
        }
      } catch (e) {
        console.warn('initial session snapshot failed', e);
      }
      const unlisten = await window.__mn.listen('mn:server-event', (ev) => {
        onEventRef.current(ev.payload);
      });
      if (cancelled) { unlisten(); return; }
      unlistenRef.current = unlisten;
    }

    function connectWs() {
      const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${protocol}//${location.host}/api/ws`);
      wsRef.current = ws;
      ws.onmessage = (e) => {
        try { onEventRef.current(JSON.parse(e.data)); } catch {}
      };
      ws.onclose = () => {
        reconnectRef.current = setTimeout(connectWs, 2000);
      };
      ws.onerror = () => ws.close();
    }

    if (isTauri()) {
      subscribeTauri();
    } else {
      connectWs();
    }

    return () => {
      cancelled = true;
      if (unlistenRef.current) { try { unlistenRef.current(); } catch {} unlistenRef.current = null; }
      if (wsRef.current) wsRef.current.close();
      if (reconnectRef.current) clearTimeout(reconnectRef.current);
    };
  }, []);
}

// ── Formatters ──

export function formatFileSize(bytes) {
  if (bytes == null) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatDuration(secs) {
  if (secs == null || secs < 0) return '';
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

export function formatTime(iso) {
  const d = new Date(iso);
  const now = new Date();
  const diff = now - d;
  if (diff < 60000) return 'just now';
  if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
  if (d.getFullYear() === now.getFullYear()) {
    return d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
  }
  return d.toLocaleDateString(undefined, { weekday: 'short', year: 'numeric', month: 'short', day: 'numeric' });
}

export function fmtTime(s) {
  if (!s || !isFinite(s)) return '0:00';
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, '0')}`;
}

export function fmtTimestamp(secs) {
  if (secs == null || !isFinite(secs)) return '0:00';
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
}

export function typeBadgeColor(type) {
  const c = {
    mic: 'bg-violet-100 text-violet-600 dark:bg-violet-900/30 dark:text-violet-400',
    system_mix: 'bg-cyan-100 text-cyan-600 dark:bg-cyan-900/30 dark:text-cyan-400',
    app: 'bg-orange-100 text-orange-600 dark:bg-orange-900/30 dark:text-orange-400',
  };
  return c[type] || 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400';
}

export function speakerColor(speaker) {
  if (!speaker) return 'bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400';
  let hash = 0;
  for (let i = 0; i < speaker.length; i++) hash = ((hash << 5) - hash + speaker.charCodeAt(i)) | 0;
  return SPEAKER_COLORS[Math.abs(hash) % SPEAKER_COLORS.length];
}

export function tagColor(name) {
  if (!name) return SPEAKER_COLORS[0];
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = ((hash << 5) - hash + name.charCodeAt(i)) | 0;
  return SPEAKER_COLORS[Math.abs(hash) % SPEAKER_COLORS.length];
}

export function normalizeTagName(input) {
  return input.toLowerCase().replace(/[^a-z0-9_]/g, '_').replace(/_+/g, '_').replace(/^_|_$/g, '');
}

// ── Icons (re-exported from icons.mjs) ──
export { ChevronIcon, PlusIcon, CloseIcon, MenuIcon, BackIcon, PlayIcon, StopIcon, MicIcon, SpeakerIcon, SourceIcon, RecordIcon, PauseIcon, StopSquareIcon, FastForwardIcon, TranscriptIcon, TagIcon, ChatIcon, SendIcon, MinimizeIcon, SparkleIcon, NewChatIcon, ContextIcon, SpinnerIcon, ExportIcon } from './icons.mjs';

// ── Shared components ──

export function StateBadge({ state, small }) {
  const styles = {
    created: 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300',
    recording: 'bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-300 animate-pulse-recording',
    stopped: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300',
  };
  const size = small ? 'px-1.5 py-0 text-[10px]' : 'px-2 py-0.5 text-xs';
  return jsx('span', {
    className: `inline-flex items-center rounded-full font-medium ${size} ${styles[state] || ''}`,
    children: state,
  });
}

import { useState, useEffect, useRef } from 'react';
import { jsx as _jsx, jsxs as _jsxs, Fragment } from 'react/jsx-runtime';

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

export const API = window.location.origin;
export const PAGE_SIZE = 50;

export const INPUT_CLS = 'w-full rounded-md border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 px-2.5 py-1.5 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-blue-500 transition-colors';
export const LABEL_CLS = 'block text-[11px] font-medium text-gray-400 dark:text-gray-500 mb-0.5 uppercase tracking-wider';

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
};

// ── API helper ──

export async function api(path, opts = {}) {
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

// ── Hooks ──

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
  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  useEffect(() => {
    function connect() {
      const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${protocol}//${location.host}/ws`);
      wsRef.current = ws;
      ws.onmessage = (e) => {
        try { onEventRef.current(JSON.parse(e.data)); } catch {}
      };
      ws.onclose = () => {
        reconnectRef.current = setTimeout(connect, 2000);
      };
      ws.onerror = () => ws.close();
    }
    connect();
    return () => {
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
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
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

// ── Icons ──

export function ChevronIcon({ open }) {
  return jsx('svg', {
    xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
    className: `w-3.5 h-3.5 transition-transform ${open ? 'rotate-90' : ''}`,
    children: jsx('path', {
      fillRule: 'evenodd', clipRule: 'evenodd',
      d: 'M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z',
    }),
  });
}

export const PlusIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { d: 'M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z' }),
});

export const CloseIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { d: 'M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z' }),
});

export const MenuIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-5 h-5',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zm0 5A.75.75 0 012.75 9h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 9.75zm0 5a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75z',
  }),
});

export const BackIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-5 h-5',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M17 10a.75.75 0 01-.75.75H5.612l4.158 3.96a.75.75 0 11-1.04 1.08l-5.5-5.25a.75.75 0 010-1.08l5.5-5.25a.75.75 0 011.04 1.08L5.612 9.25H16.25A.75.75 0 0117 10z',
  }),
});

export const PlayIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { d: 'M6.3 2.841A1.5 1.5 0 004 4.11V15.89a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z' }),
});

export const StopIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M2 10a8 8 0 1116 0 8 8 0 01-16 0zm5-2.25A.75.75 0 017.75 7h4.5a.75.75 0 01.75.75v4.5a.75.75 0 01-.75.75h-4.5a.75.75 0 01-.75-.75v-4.5z',
  }),
});

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

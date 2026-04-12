// Minimal i18n layer — no build step, no external deps.
//
// Usage:
//   import { LocaleProvider, useLocale, t } from './i18n.mjs';
//
//   // At the top of the app tree, wrap everything:
//   <LocaleProvider><App /></LocaleProvider>
//
//   // Inside any component:
//   const { locale, setLocale, t } = useLocale();
//   return <span>{t('sidebar.new_session')}</span>;
//
//   // Outside React (rare — prefer the hook), access the latest bundle:
//   import { t } from './i18n.mjs';
//   t('common.error')
//
// Locale resolution order:
//   1. `localStorage.mn_locale` if the user has explicitly picked one.
//   2. The `locale` field from `mn_get_app_info` / `/api/app-info` (which
//      the backend derives from `LANG` / macOS system locale).
//   3. `navigator.language`.
//   4. Fallback to `en`.
//
// Missing keys return the key itself so it's immediately obvious in the
// UI which strings need translation entries.

import { useState, useEffect, createContext, useContext, useCallback } from 'react';
import { jsx } from './utils.mjs';

const BUNDLES = new Map();
let currentBundle = {};
let currentLocale = 'en';

async function fetchBundle(code) {
  if (BUNDLES.has(code)) return BUNDLES.get(code);
  try {
    const res = await fetch(`/i18n/${code}.json`);
    if (!res.ok) throw new Error(`no bundle for ${code}`);
    const data = await res.json();
    BUNDLES.set(code, data);
    return data;
  } catch {
    if (code !== 'en') return fetchBundle('en');
    return {};
  }
}

/** Shorten e.g. "en-US" → "en", "zh-CN" → "zh-CN". Keeps the region for
 *  Chinese variants because Simplified vs Traditional matters; strips it
 *  for everything else. */
function canonicalizeLocale(code) {
  if (!code) return 'en';
  const lower = String(code).toLowerCase().replace('_', '-');
  if (lower.startsWith('zh')) {
    if (lower.includes('tw') || lower.includes('hk')) return 'zh-TW';
    return 'zh-CN';
  }
  return lower.split('-')[0];
}

export async function loadLocale(code) {
  const canon = canonicalizeLocale(code);
  const bundle = await fetchBundle(canon);
  currentBundle = bundle;
  currentLocale = canon;
  return { locale: canon, bundle };
}

/** Module-level `t` — useful outside the React tree. Prefer the hook. */
export function t(key, vars) {
  let s = currentBundle[key];
  if (s == null) return key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      s = s.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return s;
}

const LocaleContext = createContext({
  locale: 'en',
  setLocale: () => {},
  t: (k) => k,
});

export function useLocale() {
  return useContext(LocaleContext);
}

export function LocaleProvider({ children }) {
  const [locale, setLocaleState] = useState(currentLocale);
  const [, setRev] = useState(0); // force re-render when bundle swaps

  useEffect(() => {
    let cancelled = false;
    async function init() {
      // 1. explicit user pick wins
      const stored = typeof localStorage !== 'undefined' && localStorage.getItem('mn_locale');
      if (stored) {
        await loadLocale(stored);
        if (!cancelled) { setLocaleState(currentLocale); setRev((r) => r + 1); }
        return;
      }
      // 2. backend-detected system locale
      let detected = null;
      try {
        const info = await (await import('./utils.mjs')).api('/app-info');
        detected = info?.locale;
      } catch { /* backend not reachable yet — fall through */ }
      // 3. navigator fallback
      if (!detected && typeof navigator !== 'undefined') detected = navigator.language;
      await loadLocale(detected || 'en');
      if (!cancelled) { setLocaleState(currentLocale); setRev((r) => r + 1); }
    }
    init();
    return () => { cancelled = true; };
  }, []);

  const setLocale = useCallback(async (code) => {
    await loadLocale(code);
    try { localStorage.setItem('mn_locale', currentLocale); } catch {}
    setLocaleState(currentLocale);
    setRev((r) => r + 1);
  }, []);

  const boundT = useCallback((key, vars) => t(key, vars), [locale]);

  return jsx(LocaleContext.Provider, {
    value: { locale, setLocale, t: boundT },
    children,
  });
}

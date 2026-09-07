// Explicit usage tracking only. Never send DOM text, URLs, meeting IDs, or content.
let enabled = false;
let distinctId;
let sessionId;
let lastActivity = 0;
let refreshPromise;

function uuid() { return globalThis.crypto?.randomUUID?.(); }

export async function refreshAnalytics() {
  if (refreshPromise) return refreshPromise;
  refreshPromise = (async () => {
    try {
      const response = await fetch('/api/analytics/config');
      enabled = response.ok && (await response.json()).enabled === true;
    } catch { enabled = false; }
    finally { refreshPromise = null; }
  })();
  return refreshPromise;
}

export async function initAnalytics() {
  await refreshAnalytics();
  track('app_opened');
  setInterval(refreshAnalytics, 60_000);
}

export function track(event, properties = {}) {
  if (!enabled || globalThis.navigator?.doNotTrack === '1' || globalThis.navigator?.globalPrivacyControl) return;
  try {
    if (!distinctId) {
      try {
        distinctId = localStorage.getItem('meeting-notes-analytics-id');
        if (!/^[0-9a-f-]{36}$/i.test(distinctId || '')) {
          distinctId = uuid();
          if (distinctId) localStorage.setItem('meeting-notes-analytics-id', distinctId);
        }
      } catch { distinctId = uuid(); }
    }
    if (!sessionId || Date.now() - lastActivity > 30 * 60 * 1000) sessionId = uuid();
    lastActivity = Date.now();
    if (!distinctId || !sessionId) return;
    void fetch('/api/analytics/events', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, keepalive: true,
      body: JSON.stringify({ event, distinct_id: distinctId, session_id: sessionId, properties }),
    }).catch(() => {});
  } catch { /* Analytics must never interrupt the app. */ }
}

// Allowlisted API actions: reads/polling never inflate feature usage.
export function apiFeature(path, method = 'GET') {
  const rules = [
    ['POST', /^\/sessions$/, 'meeting_create'],
    ['PATCH', /^\/sessions\/[^/]+$/, 'meeting_edit'],
    ['DELETE', /^\/sessions\/[^/]+$/, 'meeting_delete'],
    ['POST', /^\/sessions\/[^/]+\/recording\/start$/, 'recording_start'],
    ['POST', /^\/sessions\/[^/]+\/recording\/stop$/, 'recording_stop'],
    ['POST', /^\/sessions\/[^/]+\/recording\/upload$/, 'recording_upload'],
    ['POST', /^\/sessions\/[^/]+\/transcribe$/, 'transcription_request'],
    ['DELETE', /^\/sessions\/[^/]+\/transcript$/, 'transcript_delete'],
    ['POST', /^\/sessions\/[^/]+\/summarize$/, 'summary_request'],
    ['PATCH', /^\/sessions\/[^/]+\/summary$/, 'summary_edit'],
    ['DELETE', /^\/sessions\/[^/]+\/summary$/, 'summary_delete'],
    ['PATCH', /^\/sessions\/[^/]+\/todos\/\d+$/, 'todo_toggle'],
    ['POST', /^\/sessions\/[^/]+\/attribution$/, 'speaker_attribution'],
    ['PUT', /^\/sessions\/[^/]+\/tags$/, 'tags_manage'],
    ['PUT', /^\/settings$/, 'settings_save'],
  ];
  const exact = rules.find(([m, pattern]) => method === m && pattern.test(path));
  if (exact) return exact[2];
  if (['POST', 'PATCH', 'PUT', 'DELETE'].includes(method)) {
    if (/^\/people(?:\/[^/]+)?$/.test(path)) return 'people_manage';
    if (/^\/tags(?:\/[^/]+)?$/.test(path)) return 'tags_manage';
    if (/^\/conversations(?:\/[^/]+)?$/.test(path)) return 'conversation_manage';
  }
  return null;
}

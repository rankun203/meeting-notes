// Router that translates the existing REST-style `api(path, {method, body})`
// calls into Tauri `invoke('mn_*', args)` calls when running inside the
// VoiceRecords desktop webview. Keeps the webui bundler-free — every
// existing component keeps calling `api()` and doesn't need to know which
// transport it's on.
//
// Shape: `routeToCommand({method, path})` returns `{name, args}` if the
// request maps to a Tauri command, or `null` if it should fall through to
// plain fetch(). `null` means "the frontend is running in browser /
// daemon-served mode; let the fetch() path handle it."
//
// Path patterns are matched against a small table. Each entry is a tuple:
//     [method, path-regex, (match, body) => ({ name, args })]
//
// `match` is the RegExp match array. `body` is the parsed JSON body (or
// undefined if none).

// ── Helpers ────────────────────────────────────────────────────────────────

function decodeSeg(s) {
  return decodeURIComponent(s);
}

function parseQuery(qs) {
  const out = {};
  if (!qs) return out;
  for (const pair of qs.split('&')) {
    if (!pair) continue;
    const [k, v] = pair.split('=');
    out[decodeURIComponent(k)] = v == null ? '' : decodeURIComponent(v);
  }
  return out;
}

// Parse a path (possibly with `?query`) into (pathname, query-object).
function splitPath(path) {
  const q = path.indexOf('?');
  if (q < 0) return [path, {}];
  return [path.slice(0, q), parseQuery(path.slice(q + 1))];
}

// Pass every scalar query arg through as a Number when it looks numeric,
// otherwise as a string. Tauri commands that take Option<usize> accept both
// JS `undefined` (field absent) and a number.
function coerceQuery(obj) {
  const out = {};
  for (const [k, v] of Object.entries(obj)) {
    if (v === '' || v == null) continue;
    const n = Number(v);
    out[k] = Number.isFinite(n) && String(n) === v ? n : v;
  }
  return out;
}

// ── Route table ────────────────────────────────────────────────────────────

/** @type {Array<[string, RegExp, (m: RegExpMatchArray, body: any, query: Record<string,any>) => { name: string, args: object }]>} */
const ROUTES = [
  // ---- Sessions ----
  ['POST',   /^\/sessions$/,
    (_, body) => ({ name: 'mn_create_session', args: { config: body } })],
  ['GET',    /^\/sessions$/,
    (_, __, q) => ({ name: 'mn_list_sessions', args: coerceQuery(q) })],
  ['GET',    /^\/sessions\/([^/]+)$/,
    (m) => ({ name: 'mn_get_session', args: { id: decodeSeg(m[1]) } })],
  ['PATCH',  /^\/sessions\/([^/]+)$/,
    (m, body) => ({ name: 'mn_update_session', args: { id: decodeSeg(m[1]), input: body || {} } })],
  ['DELETE', /^\/sessions\/([^/]+)$/,
    (m) => ({ name: 'mn_delete_session', args: { id: decodeSeg(m[1]) } })],
  ['POST',   /^\/sessions\/([^/]+)\/recording\/start$/,
    (m) => ({ name: 'mn_start_recording', args: { id: decodeSeg(m[1]) } })],
  ['POST',   /^\/sessions\/([^/]+)\/recording\/stop$/,
    (m) => ({ name: 'mn_stop_recording', args: { id: decodeSeg(m[1]) } })],

  // ---- Files / waveform ----
  ['GET',    /^\/sessions\/([^/]+)\/files$/,
    (m) => ({ name: 'mn_list_files', args: { id: decodeSeg(m[1]) } })],
  ['GET',    /^\/sessions\/([^/]+)\/waveform\/(.+)$/,
    (m) => ({ name: 'mn_get_waveform', args: { id: decodeSeg(m[1]), filename: decodeSeg(m[2]) } })],

  // ---- Transcripts + attribution ----
  ['GET',    /^\/sessions\/([^/]+)\/transcript$/,
    (m) => ({ name: 'mn_get_transcript', args: { id: decodeSeg(m[1]) } })],
  ['DELETE', /^\/sessions\/([^/]+)\/transcript$/,
    (m) => ({ name: 'mn_delete_transcript', args: { id: decodeSeg(m[1]) } })],
  ['GET',    /^\/sessions\/([^/]+)\/attribution$/,
    (m) => ({ name: 'mn_get_attribution', args: { id: decodeSeg(m[1]) } })],
  ['POST',   /^\/sessions\/([^/]+)\/attribution$/,
    (m, body) => ({ name: 'mn_update_attribution', args: { id: decodeSeg(m[1]), body } })],
  ['POST',   /^\/sessions\/([^/]+)\/transcribe$/,
    (m) => ({ name: 'mn_transcribe_session', args: { id: decodeSeg(m[1]) } })],

  // ---- Summary + todos ----
  ['GET',    /^\/sessions\/([^/]+)\/summary$/,
    (m) => ({ name: 'mn_get_summary', args: { id: decodeSeg(m[1]) } })],
  ['PATCH',  /^\/sessions\/([^/]+)\/summary$/,
    (m, body) => ({ name: 'mn_update_summary', args: { id: decodeSeg(m[1]), input: body || {} } })],
  ['DELETE', /^\/sessions\/([^/]+)\/summary$/,
    (m) => ({ name: 'mn_delete_summary', args: { id: decodeSeg(m[1]) } })],
  ['POST',   /^\/sessions\/([^/]+)\/summarize$/,
    (m, body) => ({ name: 'mn_summarize_session', args: { id: decodeSeg(m[1]), input: body || null } })],
  ['GET',    /^\/sessions\/([^/]+)\/todos$/,
    (m) => ({ name: 'mn_get_session_todos', args: { id: decodeSeg(m[1]) } })],
  ['PATCH',  /^\/sessions\/([^/]+)\/todos\/(\d+)$/,
    (m) => ({ name: 'mn_toggle_todo', args: { id: decodeSeg(m[1]), idx: parseInt(m[2], 10) } })],

  // ---- People ----
  ['GET',    /^\/people$/,                      () => ({ name: 'mn_list_people',        args: {} })],
  ['POST',   /^\/people$/,                      (_, body) => ({ name: 'mn_create_person',      args: { input: body } })],
  ['GET',    /^\/people\/([^/]+)$/,             (m) => ({ name: 'mn_get_person',         args: { id: decodeSeg(m[1]) } })],
  ['PATCH',  /^\/people\/([^/]+)$/,             (m, body) => ({ name: 'mn_update_person',      args: { id: decodeSeg(m[1]), input: body || {} } })],
  ['DELETE', /^\/people\/([^/]+)$/,             (m) => ({ name: 'mn_delete_person',      args: { id: decodeSeg(m[1]) } })],
  ['GET',    /^\/people\/([^/]+)\/sessions$/,   (m) => ({ name: 'mn_get_person_sessions',args: { personId: decodeSeg(m[1]) } })],
  ['GET',    /^\/people\/([^/]+)\/todos$/,      (m) => ({ name: 'mn_get_person_todos',   args: { personId: decodeSeg(m[1]) } })],

  // ---- Tags ----
  ['GET',    /^\/tags$/,                        () => ({ name: 'mn_list_tags', args: {} })],
  ['POST',   /^\/tags$/,                        (_, body) => ({ name: 'mn_create_tag', args: { input: body || {} } })],
  ['GET',    /^\/tags\/([^/]+)$/,               (m) => ({ name: 'mn_get_tag_sessions', args: { name: decodeSeg(m[1]) } })],
  ['PATCH',  /^\/tags\/([^/]+)$/,               (m, body) => ({ name: 'mn_update_tag', args: { name: decodeSeg(m[1]), input: body || {} } })],
  ['DELETE', /^\/tags\/([^/]+)$/,               (m) => ({ name: 'mn_delete_tag', args: { name: decodeSeg(m[1]) } })],
  ['PUT',    /^\/sessions\/([^/]+)\/tags$/,     (m, body) => ({ name: 'mn_set_session_tags', args: { id: decodeSeg(m[1]), input: body || { tags: [] } } })],

  // ---- Settings + config + diagnostics ----
  ['GET',    /^\/settings$/,     () => ({ name: 'mn_get_settings', args: {} })],
  ['PUT',    /^\/settings$/,     (_, body) => ({ name: 'mn_update_settings', args: { body: body || {} } })],
  ['GET',    /^\/config$/,       () => ({ name: 'mn_get_config', args: {} })],
  ['GET',    /^\/app-info$/,     () => ({ name: 'mn_get_app_info', args: {} })],
  ['GET',    /^\/diagnostics$/,  () => ({ name: 'mn_get_diagnostics', args: {} })],
  ['GET',    /^\/diagnostics\/logs$/,
    (_, __, q) => ({
      name: 'mn_tail_logs',
      args: {
        lines: q.lines != null ? Number(q.lines) : undefined,
        file: q.file,
        after: q.after != null ? Number(q.after) : undefined,
      },
    })],

  // ---- Chat (non-streaming) ----
  ['GET',    /^\/conversations$/,                           () => ({ name: 'mn_list_conversations', args: {} })],
  ['POST',   /^\/conversations$/,                           (_, body) => ({ name: 'mn_create_conversation', args: { input: body || {} } })],
  ['GET',    /^\/conversations\/([^/]+)$/,                  (m) => ({ name: 'mn_get_conversation', args: { id: decodeSeg(m[1]) } })],
  ['DELETE', /^\/conversations\/([^/]+)$/,                  (m) => ({ name: 'mn_delete_conversation', args: { id: decodeSeg(m[1]) } })],
  ['DELETE', /^\/conversations\/([^/]+)\/messages\/([^/]+)$/, (m) => ({ name: 'mn_delete_message', args: { id: decodeSeg(m[1]), msgId: decodeSeg(m[2]) } })],
  ['POST',   /^\/conversations\/([^/]+)\/claude-sync$/,     (m, body) => ({ name: 'mn_sync_claude_messages', args: { id: decodeSeg(m[1]), body: body || {} } })],
  ['GET',    /^\/conversations\/([^/]+)\/export-prompt$/,   (m) => ({ name: 'mn_export_prompt', args: { id: decodeSeg(m[1]) } })],
  ['GET',    /^\/llm\/models$/,                             () => ({ name: 'mn_list_models', args: {} })],

  // ---- Claude (non-streaming) ----
  ['GET',    /^\/claude\/status$/,        () => ({ name: 'mn_claude_status', args: {} })],
  ['POST',   /^\/claude\/stop$/,          () => ({ name: 'mn_claude_stop', args: {} })],
  ['POST',   /^\/claude\/approve-tools$/, (_, body) => ({ name: 'mn_claude_approve_tools', args: { input: body || {} } })],
  ['GET',    /^\/claude\/sessions$/,      () => ({ name: 'mn_claude_list_sessions', args: {} })],
  ['GET',    /^\/claude\/sessions\/([^/]+)$/, (m) => ({ name: 'mn_claude_get_session', args: { id: decodeSeg(m[1]) } })],
];

/**
 * Map a REST request to a Tauri command descriptor.
 * Returns `null` if no route matches — callers should fall back to `fetch()`.
 *
 * Streaming endpoints (`POST /conversations/{id}/messages` and
 * `POST /claude/send`) are intentionally NOT in this table. They need a
 * `Channel<T>` argument and are exposed through dedicated helpers in
 * `utils.mjs` (see `apiStream()`).
 */
export function routeToCommand(method, path, body) {
  const [pathname, query] = splitPath(path);
  for (const [m, re, make] of ROUTES) {
    if (m !== method) continue;
    const match = pathname.match(re);
    if (match) return make(match, body, query);
  }
  return null;
}

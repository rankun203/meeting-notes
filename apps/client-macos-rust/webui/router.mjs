/// Minimal URL router using the History API.

/**
 * Parse a URL pathname into navigation state.
 *
 *   /                        → { view: 'sessions' }
 *   /sessions                → { view: 'sessions' }
 *   /sessions/abc123         → { view: 'sessions', selectedId: 'abc123' }
 *   /people                  → { view: 'people' }
 *   /people/p_abc123         → { view: 'people', selectedPersonId: 'p_abc123' }
 *   /settings                → { view: 'settings', settingsCategory: 'services' }
 *   /settings/recognition    → { view: 'settings', settingsCategory: 'recognition' }
 */
export function parseRoute(pathname, search) {
  const parts = pathname.replace(/^\/+|\/+$/g, '').split('/').filter(Boolean);
  const first = parts[0] || 'sessions';
  const second = parts[1] || null;

  // Parse query params
  const params = new URLSearchParams(search || '');
  const query = {};
  if (params.get('content_panel')) query.contentPanel = params.get('content_panel');
  if (params.get('jump')) query.jump = parseFloat(params.get('jump'));

  switch (first) {
    case 'people':
      return { view: 'people', selectedPersonId: second, query };
    case 'settings':
      return { view: 'settings', settingsCategory: second || 'services', query };
    case 'sessions':
    default:
      return { view: 'sessions', selectedId: second, query };
  }
}

/**
 * Build a URL path from view + optional ID.
 */
export function buildPath(view, id) {
  switch (view) {
    case 'people':
      return id ? `/people/${id}` : '/people';
    case 'settings':
      return id && id !== 'services' ? `/settings/${id}` : '/settings';
    case 'sessions':
    default:
      return id ? `/sessions/${id}` : '/sessions';
  }
}

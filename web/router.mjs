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
export function parseRoute(pathname) {
  const parts = pathname.replace(/^\/+|\/+$/g, '').split('/').filter(Boolean);
  const first = parts[0] || 'sessions';
  const second = parts[1] || null;

  switch (first) {
    case 'people':
      return { view: 'people', selectedPersonId: second };
    case 'settings':
      return { view: 'settings', settingsCategory: second || 'services' };
    case 'sessions':
    default:
      return { view: 'sessions', selectedId: second };
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

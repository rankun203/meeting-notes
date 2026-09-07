import { useState, useEffect } from 'react';
import { jsx, jsxs, api, INPUT_CLS, LABEL_CLS } from './utils.mjs';
import { refreshAnalytics } from './analytics.mjs';

export function AnalyticsSettings() {
  const [config, setConfig] = useState(null);
  const [token, setToken] = useState('');
  const [clearToken, setClearToken] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState('');
  useEffect(() => { api('/analytics/config').then(setConfig).catch(() => setMessage('Unable to load analytics settings.')); }, []);
  async function save(e) {
    e.preventDefault();
    setSaving(true);
    setMessage('');
    try {
      const body = { posthog_enabled: config.posthog_enabled, posthog_host: config.posthog_host };
      if (clearToken) body.posthog_project_token = '';
      else if (token.trim()) body.posthog_project_token = token.trim();
      setConfig(await api('/analytics/config', { method: 'PUT', body: JSON.stringify(body) }));
      setToken(''); setClearToken(false);
      await refreshAnalytics();
      setMessage('Analytics settings saved. Open browser tabs refresh within a minute.');
    } catch (e) { setMessage(`Error: ${e.message}`); }
    finally { setSaving(false); }
  }
  if (!config) return jsx('p', { role: 'status', children: message || 'Loading analytics settings…' });
  return jsxs('form', { onSubmit: save, className: 'space-y-5', children: [
    jsx('h3', { className: 'text-base font-semibold', children: 'PostHog usage analytics' }),
    jsx('p', { className: 'text-sm text-gray-600 dark:text-gray-300', children: 'Understand which features people use and the steps they take. Records anonymous feature events, playback positions, and browser sessions. Meeting content, names, filenames, and chat messages are excluded. No session replay or automatic click capture.' }),
    jsxs('label', { className: 'flex items-center gap-2 text-sm', children: [
      jsx('input', { type: 'checkbox', checked: config.posthog_enabled, onChange: e => setConfig({ ...config, posthog_enabled: e.target.checked }) }),
      'Enable usage tracking when a token is configured',
    ]}),
    jsxs('div', { children: [
      jsx('label', { htmlFor: 'posthog-host', className: LABEL_CLS, children: 'PostHog domain' }),
      jsx('input', { id: 'posthog-host', type: 'url', required: true, value: config.posthog_host, onChange: e => setConfig({ ...config, posthog_host: e.target.value }), className: INPUT_CLS, placeholder: 'https://ph.dsync.net' }),
      jsx('p', { className: 'text-xs text-gray-500 mt-2', children: 'HTTPS address of the PostHog instance, without an API path.' }),
    ]}),
    jsxs('div', { children: [
      jsx('label', { htmlFor: 'posthog-token', className: LABEL_CLS, children: 'Project token' }),
      jsx('input', { id: 'posthog-token', type: 'password', autoComplete: 'new-password', value: token, disabled: clearToken, onChange: e => setToken(e.target.value), className: INPUT_CLS, placeholder: config.posthog_project_token_set ? 'Configured — leave blank to keep' : 'phc_…' }),
      jsx('p', { className: 'text-xs text-gray-500 mt-2', children: 'Stored as posthog_project_token in secrets.json. Use the project token, not a personal API key.' }),
    ]}),
    config.posthog_project_token_set && jsxs('label', { className: 'flex items-center gap-2 text-sm', children: [
      jsx('input', { type: 'checkbox', checked: clearToken, onChange: e => setClearToken(e.target.checked) }), 'Remove the saved token',
    ]}),
    jsx('p', { className: 'text-sm font-medium', children: config.enabled ? 'Status: enabled' : 'Status: disabled — enable tracking and configure a token to begin.' }),
    jsx('button', { type: 'submit', disabled: saving, className: 'px-4 py-2 rounded-lg text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50', children: saving ? 'Saving…' : 'Save analytics settings' }),
    message && jsx('p', { role: 'status', className: 'text-sm', children: message }),
  ]});
}

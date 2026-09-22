import { useState, useEffect } from 'react';
import { jsx, jsxs, api, INPUT_CLS, LABEL_CLS } from './utils.mjs';

export function GdaySettings() {
  const [account, setAccount] = useState(null);
  const [url, setUrl] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    api('/gday/auth/status').then(value => { setAccount(value); setUrl(value.url || ''); })
      .catch(error => setError(error.message));
  }, []);
  async function login() {
    setBusy(true); setError('');
    try {
      const result = await api('/gday/auth/login', { method: 'POST', body: JSON.stringify({ url }) });
      window.location.assign(result.authorization_url);
    } catch (error) { setError(error.message); setBusy(false); }
  }
  async function logout() {
    setBusy(true); setError('');
    try { setAccount(await api('/gday/auth/logout', { method: 'POST' })); }
    catch (error) { setError(error.message); }
    finally { setBusy(false); }
  }
  return jsxs('div', { className: 'space-y-3 border-b border-gray-200 dark:border-gray-700 pb-6', children: [
    jsx('p', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'Gday Meetings' }),
    jsx('p', { className: 'text-xs text-gray-500', children: 'Sign in to upload recordings and have Gday Meetings transcribe them. Results stay available when this app is closed.' }),
    account?.connected ? jsxs('div', { className: 'space-y-2', children: [
      jsx('p', { className: 'text-sm', children: `Signed in as ${account.email || account.subject}` }),
      jsx('p', { className: 'text-xs text-gray-500', children: account.url }),
      jsx('button', { type: 'button', disabled: busy, onClick: logout, className: 'text-sm text-blue-600 disabled:opacity-50', children: busy ? 'Signing out…' : 'Sign out' }),
    ] }) : jsxs('div', { className: 'space-y-2', children: [
      jsx('label', { className: LABEL_CLS, htmlFor: 'gday-url', children: 'Gday Meetings URL' }),
      jsx('input', { id: 'gday-url', className: INPUT_CLS, type: 'url', value: url, placeholder: 'https://meetings.example.com', onChange: event => setUrl(event.target.value) }),
      jsx('button', { type: 'button', disabled: busy || !url.trim(), onClick: login, className: 'rounded-lg bg-blue-600 px-3 py-2 text-sm text-white disabled:opacity-50', children: busy ? 'Opening sign-in…' : 'Login to Gday Meetings' }),
    ] }),
    error && jsx('p', { role: 'alert', className: 'text-sm text-red-600', children: error }),
  ] });
}

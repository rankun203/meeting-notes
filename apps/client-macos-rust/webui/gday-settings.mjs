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
    jsx('p', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'Gday Meetings Server' }),
    jsx('p', { className: 'text-xs text-gray-500', children: 'Sign in to upload recordings and have Gday Meetings Server transcribe them. Results stay available when this app is closed.' }),
    account?.connected ? jsxs('div', { className: 'space-y-2', children: [
      jsx('p', { className: 'text-sm', children: `Signed in as ${account.email || account.subject}` }),
      jsx('p', { className: 'text-xs text-gray-500', children: account.url }),
      jsx('button', { type: 'button', disabled: busy, onClick: logout, className: 'text-sm text-blue-600 disabled:opacity-50', children: busy ? 'Signing out…' : 'Sign out' }),
    ] }) : jsxs('div', { className: 'space-y-2', children: [
      jsx('label', { className: LABEL_CLS, htmlFor: 'gday-url', children: 'Gday Meetings Server URL' }),
      jsx('input', { id: 'gday-url', className: INPUT_CLS, type: 'url', value: url, placeholder: 'https://meetings.example.com', onChange: event => setUrl(event.target.value) }),
      jsx('button', { type: 'button', disabled: busy || !url.trim(), onClick: login, className: 'rounded-lg bg-blue-600 px-3 py-2 text-sm text-white disabled:opacity-50', children: busy ? 'Opening sign-in…' : 'Login to Gday Meetings Server' }),
    ] }),
    error && jsx('p', { role: 'alert', className: 'text-sm text-red-600', children: error }),
    account?.connected && jsx(GdayMigration, {}, account.url),
  ] });
}

function GdayMigration() {
  const [preview, setPreview] = useState(null);
  const [status, setStatus] = useState(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const buttonClass = 'rounded-lg bg-blue-600 px-3 py-2 text-sm text-white disabled:opacity-50';

  useEffect(() => {
    let active = true;
    Promise.all([api('/gday/migration/preview'), api('/gday/migration/status')])
      .then(([plan, progress]) => {
        if (active) { setPreview(plan); setStatus(progress); }
      })
      .catch(error => { if (active) setError(error.message); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!status?.running) return;
    let active = true;
    let timer;
    async function poll() {
      try {
        const progress = await api('/gday/migration/status');
        if (!active) return;
        setStatus(progress);
        setError('');
        if (!progress.running) return;
      } catch (error) {
        if (active) setError(`Progress update failed: ${error.message}. Retrying…`);
      }
      if (active) timer = setTimeout(poll, 1500);
    }
    timer = setTimeout(poll, 1500);
    return () => { active = false; clearTimeout(timer); };
  }, [status?.running]);

  async function refresh() {
    setBusy(true); setError('');
    try {
      const [plan, progress] = await Promise.all([
        api('/gday/migration/preview'), api('/gday/migration/status'),
      ]);
      setPreview(plan); setStatus(progress);
    } catch (error) { setError(error.message); }
    finally { setBusy(false); }
  }

  async function start() {
    setBusy(true); setError('');
    try { setStatus(await api('/gday/migration', { method: 'POST' })); }
    catch (error) { setError(error.message); }
    finally { setBusy(false); }
  }

  const running = Boolean(status?.running);
  const results = status?.results || [];
  return jsxs('div', { className: 'space-y-2 border-t border-gray-200 dark:border-gray-700 pt-3', children: [
    jsx('p', { className: 'text-sm font-medium', children: 'Copy existing meetings to Gday Meetings Server' }),
    jsx('p', { className: 'text-xs text-gray-500', children: 'Copy recordings and existing results to your signed-in Gday Meetings Server account. Local files stay as a backup; this does not delete files or start transcription.' }),
    preview ? jsx('p', { className: 'text-sm', children: `${preview.ready} of ${preview.total} meetings ready · ${(preview.audioBytes / (1024 * 1024)).toFixed(1)} MiB audio` })
      : jsx('p', { className: 'text-xs text-gray-500', children: error ? 'Preview unavailable.' : 'Checking local meetings…' }),
    preview?.blocked?.length > 0 && jsxs('div', { className: 'text-sm', children: [
      jsx('p', { children: 'These meetings will be skipped until their issues are resolved:' }),
      jsx('ul', { className: 'list-disc pl-5', children: preview.blocked.map(meeting => jsx('li', {
        children: `${meeting.title || meeting.id}: ${meeting.error}`,
      }, meeting.id)) }),
    ] }),
    jsxs('div', { className: 'flex flex-wrap gap-3 items-center', children: [
      jsx('button', { type: 'button', className: buttonClass,
        disabled: busy || running || !status || !preview?.ready, onClick: start,
        children: running ? 'Copying meetings…' : busy ? 'Please wait…' : results.length ? 'Retry copying meetings' : 'Copy existing meetings to Gday Meetings Server',
      }),
      jsx('button', { type: 'button', disabled: busy || running, onClick: refresh,
        className: 'text-sm text-blue-600 disabled:opacity-50', children: 'Refresh preview',
      }),
    ] }),
    status && (running || results.length > 0) && jsxs('div', { className: 'space-y-2', children: [
      jsx('p', { role: 'status', className: 'text-sm', children: `${running ? 'Copying' : 'Finished'}: ${status.completed} / ${status.total} processed${running && status.current ? ` · ${status.current}` : ''}` }),
      jsx('ul', { className: 'max-h-60 overflow-y-auto space-y-1 text-xs', children: results.map(meeting => jsx('li', {
        className: meeting.status === 'failed' ? 'text-red-600' : 'text-gray-600 dark:text-gray-400',
        children: `${meeting.title || meeting.id}: ${meeting.status === 'imported' ? 'Copied' : meeting.status === 'already_imported' ? 'Already copied' : `Failed — ${meeting.error || 'Unknown error'}`}`,
      }, meeting.id)) }),
    ] }),
    status?.error && jsx('p', { role: 'alert', className: 'text-sm text-red-600', children: status.error }),
    error && jsx('p', { role: 'alert', className: 'text-sm text-red-600', children: error }),
  ] });
}

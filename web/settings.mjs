import { useState, useEffect } from 'react';
import { jsx, jsxs, api, INPUT_CLS, LABEL_CLS } from './utils.mjs';

const SETTINGS_CATEGORIES = [
  { id: 'services', label: 'Services' },
  { id: 'recognition', label: 'Recognition' },
  { id: 'file_drop', label: 'File Transfer' },
];

export function SettingsSidebar({ selected, onSelect }) {
  return jsx('div', {
    className: 'space-y-0.5 px-2 py-2',
    children: SETTINGS_CATEGORIES.map(cat => jsx('button', {
      key: cat.id,
      onClick: () => onSelect(cat.id),
      className: [
        'w-full text-left px-3 py-2 rounded-lg text-xs font-medium transition-colors',
        selected === cat.id
          ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800'
          : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800/60 border border-transparent',
      ].join(' '),
      children: cat.label,
    })),
  });
}

export function SettingsPage({ category }) {
  const [settings, setSettings] = useState(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState({});
  const [message, setMessage] = useState(null);

  useEffect(() => {
    api('/settings').then(data => {
      setSettings(data);
      setForm({
        audio_extraction_url: data.audio_extraction_url || '',
        audio_extraction_api_key: '',
        file_drop_url: data.file_drop_url || '',
        file_drop_api_key: '',
        diarize: data.diarize ?? true,
        people_recognition: data.people_recognition ?? true,
        speaker_match_threshold: data.speaker_match_threshold ?? 0.75,
      });
      setLoading(false);
    }).catch(e => { setLoading(false); setMessage(`Error: ${e.message}`); });
  }, []);

  async function save() {
    setSaving(true);
    setMessage(null);
    try {
      const update = {};
      if (form.audio_extraction_url !== (settings.audio_extraction_url || ''))
        update.audio_extraction_url = form.audio_extraction_url || null;
      if (form.audio_extraction_api_key)
        update.audio_extraction_api_key = form.audio_extraction_api_key;
      if (form.file_drop_url !== (settings.file_drop_url || ''))
        update.file_drop_url = form.file_drop_url;
      if (form.file_drop_api_key)
        update.file_drop_api_key = form.file_drop_api_key;
      if (form.diarize !== settings.diarize)
        update.diarize = form.diarize;
      if (form.people_recognition !== settings.people_recognition)
        update.people_recognition = form.people_recognition;
      if (form.speaker_match_threshold !== settings.speaker_match_threshold)
        update.speaker_match_threshold = parseFloat(form.speaker_match_threshold);
      if (Object.keys(update).length === 0) {
        setMessage('No changes to save');
        setSaving(false);
        return;
      }
      const result = await api('/settings', { method: 'PUT', body: JSON.stringify(update) });
      setSettings(result);
      setForm(prev => ({ ...prev, audio_extraction_api_key: '', file_drop_api_key: '' }));
      setMessage('Settings saved');
    } catch (e) {
      setMessage(`Error: ${e.message}`);
    } finally {
      setSaving(false);
    }
  }

  if (loading) return jsx('div', { className: 'p-8 text-gray-400 text-sm', children: 'Loading settings...' });

  const cat = category || 'services';
  const title = SETTINGS_CATEGORIES.find(c => c.id === cat)?.label || 'Settings';

  const categoryContent = {
    services: jsxs('div', { className: 'space-y-4', children: [
      jsx('p', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'Audio Extraction (RunPod)' }),
      jsxs('div', { children: [
        jsx('label', { className: LABEL_CLS, children: 'Endpoint URL' }),
        jsx('input', {
          type: 'text', value: form.audio_extraction_url,
          onChange: e => setForm(prev => ({ ...prev, audio_extraction_url: e.target.value })),
          placeholder: 'https://api.runpod.ai/v2/ENDPOINT_ID',
          className: INPUT_CLS,
        }),
      ]}),
      jsxs('div', { children: [
        jsx('label', { className: LABEL_CLS, children: 'API Key' }),
        jsx('input', {
          type: 'password', value: form.audio_extraction_api_key,
          onChange: e => setForm(prev => ({ ...prev, audio_extraction_api_key: e.target.value })),
          placeholder: settings?.audio_extraction_api_key ? `Current: ${settings.audio_extraction_api_key}` : 'Enter RunPod API key',
          className: INPUT_CLS,
        }),
      ]}),
    ]}),

    recognition: jsxs('div', { className: 'space-y-4', children: [
      jsxs('label', { className: 'flex items-center gap-3 cursor-pointer', children: [
        jsx('input', {
          type: 'checkbox', checked: form.diarize,
          onChange: e => setForm(prev => ({ ...prev, diarize: e.target.checked })),
          className: 'w-4 h-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500',
        }),
        jsx('span', { className: 'text-sm text-gray-700 dark:text-gray-300', children: 'Enable speaker diarization (identify who spoke when)' }),
      ]}),
      jsxs('label', { className: 'flex items-center gap-3 cursor-pointer', children: [
        jsx('input', {
          type: 'checkbox', checked: form.people_recognition,
          onChange: e => setForm(prev => ({ ...prev, people_recognition: e.target.checked })),
          className: 'w-4 h-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500',
        }),
        jsx('span', { className: 'text-sm text-gray-700 dark:text-gray-300', children: 'Auto-match speakers to known people after diarization' }),
      ]}),
      jsxs('div', { children: [
        jsx('label', { className: LABEL_CLS, children: 'Match Threshold' }),
        jsxs('div', { className: 'flex items-center gap-2', children: [
          jsx('input', {
            type: 'range', min: 0.5, max: 0.95, step: 0.05, value: form.speaker_match_threshold,
            onChange: e => setForm(prev => ({ ...prev, speaker_match_threshold: parseFloat(e.target.value) })),
            className: 'flex-1',
          }),
          jsx('span', { className: 'text-sm font-mono text-gray-500 w-10', children: form.speaker_match_threshold }),
        ]}),
      ]}),
    ]}),

    file_drop: jsxs('div', { className: 'space-y-4', children: [
      jsx('p', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'File Transfer Server' }),
      jsx('p', { className: 'text-xs text-gray-400 dark:text-gray-500', children: 'Temporary file parking for audio uploads to GPU workers.' }),
      jsxs('div', { children: [
        jsx('label', { className: LABEL_CLS, children: 'Server URL' }),
        jsx('input', {
          type: 'text', value: form.file_drop_url,
          onChange: e => setForm(prev => ({ ...prev, file_drop_url: e.target.value })),
          placeholder: 'https://file-drop.dsync.net',
          className: INPUT_CLS,
        }),
      ]}),
      jsxs('div', { children: [
        jsx('label', { className: LABEL_CLS, children: 'API Key' }),
        jsx('input', {
          type: 'password', value: form.file_drop_api_key,
          onChange: e => setForm(prev => ({ ...prev, file_drop_api_key: e.target.value })),
          placeholder: settings?.file_drop_api_key ? `Current: ${settings.file_drop_api_key}` : 'Enter file-drop API key',
          className: INPUT_CLS,
        }),
      ]}),
    ]}),
  };

  return jsx('div', {
    className: 'h-full overflow-y-auto px-6 py-6',
    children: jsxs('div', { className: 'max-w-xl space-y-6', children: [
      jsx('h2', { className: 'text-lg font-semibold', children: title }),
      jsx('div', {
        className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-5',
        children: categoryContent[cat],
      }),
      jsxs('div', { className: 'flex items-center gap-3', children: [
        jsx('button', {
          onClick: save, disabled: saving,
          className: 'px-4 py-2 rounded-lg text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50 transition-colors',
          children: saving ? 'Saving...' : 'Save Settings',
        }),
        message && jsx('span', {
          className: `text-sm ${message.startsWith('Error') ? 'text-red-500' : 'text-emerald-500'}`,
          children: message,
        }),
      ]}),
    ]}),
  });
}

import { useState, useEffect } from 'react';
import { jsx, jsxs, Fragment, api, INPUT_CLS, LABEL_CLS, tagColor, normalizeTagName, TagIcon, ChevronIcon } from './utils.mjs';
import { ConversationsSettings } from './chat.mjs';

const SETTINGS_CATEGORIES = [
  { id: 'services', label: 'Services' },
  { id: 'pipeline', label: 'Pipeline' },
  { id: 'tags', label: 'Tags' },
  { id: 'conversations', label: 'Conversations' },
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

// ── Tags Settings (self-contained, no global save button) ──

function TagsSettings({ onSelectSession }) {
  const [tags, setTags] = useState([]);
  const [loading, setLoading] = useState(true);
  const [newName, setNewName] = useState('');
  const [expandedTag, setExpandedTag] = useState(null);
  const [tagSessions, setTagSessions] = useState([]);
  const [loadingSessions, setLoadingSessions] = useState(false);
  const [renamingTag, setRenamingTag] = useState(null); // tag name being renamed
  const [renameValue, setRenameValue] = useState('');

  async function fetchTags() {
    try {
      const data = await api('/tags');
      setTags(data.tags || []);
    } catch {}
    setLoading(false);
  }

  useEffect(() => { fetchTags(); }, []);

  async function createTag() {
    const normalized = normalizeTagName(newName);
    if (!normalized) return;
    try {
      await api('/tags', { method: 'POST', body: JSON.stringify({ name: normalized }) });
      setNewName('');
      fetchTags();
    } catch (e) { alert(e.message); }
  }

  async function toggleHidden(name, hidden) {
    try {
      await api(`/tags/${encodeURIComponent(name)}`, { method: 'PATCH', body: JSON.stringify({ hidden }) });
      fetchTags();
    } catch (e) { alert(e.message); }
  }

  async function deleteTag(name) {
    if (!confirm(`Delete tag "${name}"? It will be removed from all sessions.`)) return;
    try {
      await api(`/tags/${encodeURIComponent(name)}`, { method: 'DELETE' });
      if (expandedTag === name) { setExpandedTag(null); setTagSessions([]); }
      fetchTags();
    } catch (e) { alert(e.message); }
  }

  function startRename(name) {
    setRenamingTag(name);
    setRenameValue(name);
  }

  async function submitRename(oldName) {
    setRenamingTag(null);
    const normalized = normalizeTagName(renameValue);
    if (!normalized || normalized === oldName) return;
    try {
      await api(`/tags/${encodeURIComponent(oldName)}`, { method: 'PATCH', body: JSON.stringify({ name: normalized }) });
      if (expandedTag === oldName) setExpandedTag(normalized);
      fetchTags();
    } catch (e) { alert(e.message); }
  }

  async function toggleExpand(name) {
    if (expandedTag === name) {
      setExpandedTag(null);
      setTagSessions([]);
      return;
    }
    setExpandedTag(name);
    setLoadingSessions(true);
    try {
      const data = await api(`/tags/${encodeURIComponent(name)}`);
      setTagSessions(data.sessions || []);
    } catch { setTagSessions([]); }
    setLoadingSessions(false);
  }

  async function removeSessionFromTag(sessionId, tagName) {
    try {
      const session = await api(`/sessions/${sessionId}`);
      const newTags = (session.tags || []).filter(t => t !== tagName);
      await api(`/sessions/${sessionId}/tags`, { method: 'PUT', body: JSON.stringify({ tags: newTags }) });
      const data = await api(`/tags/${encodeURIComponent(tagName)}`);
      setTagSessions(data.sessions || []);
      fetchTags();
    } catch (e) { alert(e.message); }
  }

  if (loading) return jsx('div', { className: 'text-sm text-gray-400 py-4', children: 'Loading...' });

  return jsxs('div', { className: 'space-y-4', children: [
    // Create tag input
    jsxs('div', { className: 'flex gap-2', children: [
      jsx('input', {
        type: 'text', value: newName, placeholder: 'New tag name...',
        onChange: e => setNewName(e.target.value),
        onKeyDown: e => { if (e.key === 'Enter') createTag(); },
        className: INPUT_CLS + ' flex-1',
      }),
      jsx('button', {
        onClick: createTag, disabled: !normalizeTagName(newName),
        className: 'px-3 py-1.5 rounded-lg text-xs font-medium text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50 transition-colors',
        children: 'Add',
      }),
    ]}),
    newName && normalizeTagName(newName) !== newName && jsx('p', {
      className: 'text-[11px] text-gray-400',
      children: `Will be saved as: ${normalizeTagName(newName)}`,
    }),

    // Tags list
    tags.length === 0
      ? jsx('p', { className: 'text-xs text-gray-400 py-2', children: 'No tags yet. Create one above.' })
      : jsx('div', { className: 'space-y-1', children:
          tags.map(t => jsxs('div', {
            key: t.name,
            className: 'rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden',
            children: [
              // Tag row
              jsxs('div', {
                className: 'flex items-center gap-2 px-3 py-2 cursor-pointer',
                onClick: () => toggleExpand(t.name),
                children: [
                  jsx('span', {
                    className: 'flex-shrink-0',
                    children: jsx(ChevronIcon, { open: expandedTag === t.name }),
                  }),
                  renamingTag === t.name
                    ? jsx('input', {
                        type: 'text', value: renameValue, autoFocus: true,
                        onClick: e => e.stopPropagation(),
                        onChange: e => setRenameValue(e.target.value),
                        onBlur: () => submitRename(t.name),
                        onKeyDown: e => { if (e.key === 'Enter') e.target.blur(); if (e.key === 'Escape') setRenamingTag(null); },
                        className: 'text-[11px] px-1.5 py-0.5 rounded border border-blue-400 bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-300 w-28 focus:outline-none',
                      })
                    : jsx('span', {
                        onDoubleClick: () => startRename(t.name),
                        className: `inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium cursor-default select-none ${tagColor(t.name)}`,
                        title: 'Double-click to rename',
                        children: jsxs(Fragment, { children: [
                          jsx(TagIcon, { className: 'w-2.5 h-2.5' }),
                          t.name,
                        ]}),
                      }),
                  jsx('span', {
                    className: 'text-[10px] text-gray-400 ml-auto',
                    children: `${t.session_count} session${t.session_count !== 1 ? 's' : ''}`,
                  }),
                  jsxs('label', {
                    onClick: e => e.stopPropagation(),
                    className: 'flex items-center gap-1.5 cursor-pointer ml-2',
                    title: 'Hide sessions with this tag from the list',
                    children: [
                      jsx('input', {
                        type: 'checkbox', checked: t.hidden,
                        onChange: e => toggleHidden(t.name, e.target.checked),
                        className: 'w-3.5 h-3.5 rounded border-gray-300 text-blue-600 focus:ring-blue-500',
                      }),
                      jsx('span', { className: 'text-[10px] text-gray-400', children: 'Hide' }),
                    ],
                  }),
                  jsx('button', {
                    onClick: (e) => { e.stopPropagation(); deleteTag(t.name); },
                    className: 'text-[11px] text-red-400 hover:text-red-600 ml-1 transition-colors',
                    children: 'Delete',
                  }),
                ],
              }),
              // Expanded: sessions list
              expandedTag === t.name && jsx('div', {
                className: 'border-t border-gray-200 dark:border-gray-700 px-3 py-2 bg-gray-50 dark:bg-gray-800/50',
                children: loadingSessions
                  ? jsx('p', { className: 'text-[11px] text-gray-400 py-1', children: 'Loading...' })
                  : tagSessions.length === 0
                    ? jsx('p', { className: 'text-[11px] text-gray-400 py-1', children: 'No sessions with this tag' })
                    : jsx('div', { className: 'space-y-1', children:
                        tagSessions.map(s => jsxs('div', {
                          key: s.id,
                          className: 'flex items-center justify-between py-1',
                          children: [
                            jsx('button', {
                              onClick: () => onSelectSession && onSelectSession(s.id),
                              className: 'min-w-0 flex-1 text-left hover:underline',
                              children: jsxs('div', { children: [
                                jsx('p', { className: 'text-xs text-gray-700 dark:text-gray-300 truncate', children: s.name || s.id }),
                                jsx('p', { className: 'text-[10px] text-gray-400', children: new Date(s.created_at).toLocaleDateString() }),
                              ]}),
                            }),
                            jsx('button', {
                              onClick: () => removeSessionFromTag(s.id, t.name),
                              title: 'Remove this session from tag',
                              className: 'text-[11px] text-red-400 hover:text-red-600 px-1 transition-colors',
                              children: '\u00d7',
                            }),
                          ],
                        })),
                      }),
              }),
            ],
          })),
        }),
  ]});
}

export function SettingsPage({ category, onSelectSession }) {
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
        summarization_prompt: data.summarization_prompt || '',
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
      if (form.summarization_prompt !== (settings.summarization_prompt || ''))
        update.summarization_prompt = form.summarization_prompt || null;
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
    services: jsxs('div', { className: 'space-y-6', children: [
      jsxs('div', { className: 'space-y-4', children: [
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
      jsx('hr', { className: 'border-gray-200 dark:border-gray-700' }),
      jsxs('div', { className: 'space-y-4', children: [
        jsx('p', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'File Transfer' }),
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
    ]}),

    pipeline: jsxs('div', { className: 'space-y-6', children: [
      jsxs('div', { className: 'space-y-4', children: [
        jsx('p', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'Recognition' }),
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
      jsx('hr', { className: 'border-gray-200 dark:border-gray-700' }),
      jsxs('div', { className: 'space-y-4', children: [
        jsx('p', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'Summarization' }),
        jsx('p', { className: 'text-xs text-gray-400 dark:text-gray-500', children: 'Used when exporting transcripts as ChatGPT messages. Automatic meeting summarization with different styles coming soon.' }),
        jsxs('div', { children: [
          jsx('label', { className: LABEL_CLS, children: 'Prompt' }),
          jsx('textarea', {
            value: form.summarization_prompt,
            onChange: e => setForm(prev => ({ ...prev, summarization_prompt: e.target.value })),
            placeholder: 'e.g. Summarize this meeting transcript, highlighting key decisions and action items.',
            rows: 4,
            className: INPUT_CLS + ' resize-y',
          }),
        ]}),
      ]}),
    ]}),

    tags: jsx(TagsSettings, { onSelectSession }),
    conversations: jsx(ConversationsSettings, {}),
  };

  return jsx('div', {
    className: 'h-full overflow-y-auto px-6 py-6',
    children: jsxs('div', { className: 'max-w-xl space-y-6', children: [
      jsx('h2', { className: 'text-lg font-semibold', children: title }),
      jsx('div', {
        className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-5',
        children: categoryContent[cat],
      }),
      (cat !== 'tags' && cat !== 'conversations') && jsxs('div', { className: 'flex items-center gap-3', children: [
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

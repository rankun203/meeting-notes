import { useState, useEffect, useMemo, useRef } from 'react';
import { jsx, jsxs, Fragment, api, API, INPUT_CLS, LABEL_CLS, PROCESSING_LABELS,
         formatFileSize, formatDuration, formatTime, typeBadgeColor, tagColor, autoResize,
         ChevronIcon, PlayIcon, StopIcon, StateBadge, BackIcon,
         RecordIcon, TranscriptIcon, TagIcon, PlusIcon, CloseIcon } from './utils.mjs';
import { SyncedPlayer } from './player.mjs';
import { TranscriptViewer, SpeakerAttributionWrapper } from './transcript.mjs';
import { SearchableList } from './searchable-list.mjs';

// ── New Session Form ──

export function NewSessionPanel({ sources: availableSources, fields, onCreated, onSelect }) {
  const fieldEntries = useMemo(() => Object.entries(fields || {}), [fields]);
  const initVals = useMemo(() => {
    const v = {};
    for (const [k, f] of fieldEntries) v[k] = f.default ?? '';
    return v;
  }, [fieldEntries]);

  const [vals, setVals] = useState(initVals);
  const [selectedSources, setSelectedSources] = useState([]);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [error, setError] = useState(null);
  const [creating, setCreating] = useState(false);

  useEffect(() => { if (fieldEntries.length) setVals(initVals); }, [initVals]);

  useEffect(() => {
    if (availableSources.length > 0 && selectedSources.length === 0) {
      setSelectedSources(availableSources.filter(s => s.default_selected).map(s => s.id));
    }
  }, [availableSources]);

  function setVal(key, value) { setVals(prev => ({ ...prev, [key]: value })); }
  function toggleSource(id) {
    setSelectedSources(prev =>
      prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]
    );
  }

  function isVisible(f) {
    if (!f.show_when) return true;
    return vals[f.show_when.field] === f.show_when.value;
  }

  function buildBody() {
    const body = { sources: selectedSources };
    for (const [key, f] of fieldEntries) {
      const v = vals[key];
      if (v === '' && f.type !== 'text') continue;
      if (f.config_path) {
        const parts = f.config_path.split('.');
        let obj = body;
        for (let i = 0; i < parts.length - 1; i++) {
          if (!obj[parts[i]]) obj[parts[i]] = {};
          obj = obj[parts[i]];
        }
        obj[parts[parts.length - 1]] = typeof f.default === 'number' ? Number(v) : v;
      } else {
        body[key] = typeof f.default === 'number' ? Number(v) : v;
      }
    }
    return body;
  }

  async function create() {
    setError(null);
    setCreating(true);
    try {
      const session = await api('/sessions', { method: 'POST', body: JSON.stringify(buildBody()) });
      if (session && session.id) {
        try {
          await api(`/sessions/${session.id}/recording/start`, { method: 'POST' });
        } catch (e) {
          setError(`Created but failed to start recording: ${e.message}`);
        }
        await onCreated();
        onSelect(session.id);
      }
    } catch (e) { setError(e.message); }
    finally { setCreating(false); }
  }

  function renderField(key, f) {
    if (!isVisible(f)) return null;
    const label = jsx('label', { className: LABEL_CLS, title: f.description, children: f.label });
    let input;
    if (f.type === 'select') {
      input = jsxs('select', {
        value: vals[key],
        onChange: e => setVal(key, typeof f.default === 'number' ? Number(e.target.value) : e.target.value),
        className: INPUT_CLS, title: f.description,
        children: (f.options || []).map(o => jsx('option', { key: o.value, value: o.value, title: o.title || '', children: o.label })),
      });
    } else if (f.type === 'textarea') {
      input = jsx('textarea', {
        value: vals[key], onChange: e => setVal(key, e.target.value),
        placeholder: f.placeholder || '', className: INPUT_CLS + ' resize-y min-h-[48px]', rows: 2,
        title: f.description,
      });
    } else {
      input = jsx('input', {
        value: vals[key], onChange: e => setVal(key, e.target.value),
        placeholder: f.placeholder || f.default || '', className: INPUT_CLS,
        title: f.description,
      });
    }
    return jsxs('div', { key, children: [label, input] });
  }

  const basicFields = fieldEntries.filter(([, f]) => !f.advanced);
  const advancedFields = fieldEntries.filter(([, f]) => f.advanced);
  const visibleAdvanced = advancedFields.filter(([, f]) => isVisible(f));

  return jsxs('div', { className: 'space-y-3', children: [
    jsx('div', { className: `grid grid-cols-${Math.min(basicFields.length, 2)} gap-2`,
      children: basicFields.map(([k, f]) => renderField(k, f)),
    }),
    jsxs('div', { children: [
      jsx('label', { className: LABEL_CLS, children: 'Sources' }),
      jsx('div', { className: 'space-y-0.5', children:
        availableSources.map(s => jsx('label', {
          key: s.id,
          className: 'flex items-center gap-2 py-1 px-1.5 rounded hover:bg-gray-100 dark:hover:bg-gray-800/50 cursor-pointer transition-colors',
          children: jsxs(Fragment, { children: [
            jsx('input', {
              type: 'checkbox',
              checked: selectedSources.includes(s.id),
              onChange: () => toggleSource(s.id),
              className: 'rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500/40 h-3.5 w-3.5',
            }),
            jsx('span', { className: 'text-xs text-gray-700 dark:text-gray-300 flex-1 truncate', children: s.label }),
            jsx('span', {
              className: `text-[9px] font-medium px-1 py-0 rounded-full ${typeBadgeColor(s.source_type)}`,
              children: s.source_type === 'system_mix' ? 'sys' : s.source_type,
            }),
          ]}),
        })),
      }),
    ]}),
    visibleAdvanced.length > 0 && jsx('button', {
      className: 'flex items-center gap-1 text-[11px] text-gray-400 dark:text-gray-500 hover:text-gray-600 dark:hover:text-gray-300 transition-colors',
      onClick: () => setShowAdvanced(!showAdvanced),
      children: jsxs(Fragment, { children: [jsx(ChevronIcon, { open: showAdvanced }), 'Advanced'] }),
    }),
    showAdvanced && jsx('div', { className: 'space-y-2',
      children: advancedFields.map(([k, f]) => renderField(k, f)),
    }),
    jsx('button', {
      onClick: create, disabled: creating,
      className: 'w-full flex justify-center items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium text-white bg-red-500 hover:bg-red-600 disabled:opacity-50 transition-colors',
      children: creating ? 'Starting...' : jsxs(Fragment, { children: [
        jsx(RecordIcon, {}),
        'Start Recording',
      ]}),
    }),
    error && jsx('p', { className: 'text-xs text-red-500', children: error }),
  ]});
}

// ── Sidebar Session Item ──

export function SidebarItem({ session, selected, onClick }) {
  const s = session;
  return jsx('button', {
    onClick,
    className: [
      'w-full text-left px-3 py-2.5 rounded-lg transition-colors',
      selected
        ? 'bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800'
        : 'hover:bg-gray-100 dark:hover:bg-gray-800/60 border border-transparent',
    ].join(' '),
    children: jsxs('div', { className: 'flex items-center justify-between gap-2', children: [
      jsxs('div', { className: 'min-w-0 flex-1', children: [
        jsxs('div', { className: 'flex items-center gap-2', children: [
          s.name
            ? jsx('span', {
                className: `text-xs font-medium truncate ${selected ? 'text-blue-700 dark:text-blue-300' : 'text-gray-700 dark:text-gray-300'}`,
                children: s.name,
              })
            : jsx('code', {
                className: `text-xs font-mono ${selected ? 'text-blue-700 dark:text-blue-300' : 'text-gray-500 dark:text-gray-400'}`,
                children: s.id,
              }),
          jsx(StateBadge, { state: s.state, small: true }),
        ]}),
        jsx('p', {
          className: 'text-[11px] text-gray-400 dark:text-gray-500 mt-0.5',
          children: s.duration_secs != null
            ? `${formatDuration(s.duration_secs)} · ${formatTime(s.created_at)}`
            : formatTime(s.created_at),
        }),
      ]}),
      s.state === 'recording' && jsx('span', {
        className: 'w-2 h-2 rounded-full bg-red-500 animate-pulse-recording flex-shrink-0',
      }),
    ]}),
  });
}

// ── Session Detail ──

// Lazy-load marked for summary rendering
let markedModule = null;
let markedLoading = false;
const markedWaiters = [];

function ensureMarked(cb) {
  if (markedModule) { cb(); return; }
  markedWaiters.push(cb);
  if (markedLoading) return;
  markedLoading = true;
  import('marked').then(mod => {
    markedModule = mod.marked;
    markedModule.setOptions({ breaks: true, gfm: true });
    for (const fn of markedWaiters) fn();
    markedWaiters.length = 0;
  });
}

function renderMarkdown(content) {
  if (!content) return '';
  if (!markedModule) return content.replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, '<br>');
  return markedModule.parse(content);
}

// Convert [MM:SS] citation markers into clickable links for a given session
function convertCitations(content, sessionId) {
  if (!content) return content;
  // Convert [MM:SS-MM:SS] ranges (use start time), [MM:SS], and 【MM:SS】 to markdown links
  return content
    .replace(/[\[【](\d+):(\d{2})\s*[-–—‑\u2010-\u2015]\s*\d+:\d{2}[\]】](?!\()/g, (match, mins, secs) => {
      const total = parseInt(mins) * 60 + parseInt(secs);
      return `[${mins}:${secs}](/sessions/${sessionId}?content_panel=transcript&jump=${total})`;
    })
    .replace(/[\[【](\d+):(\d{2})[\]】](?!\()/g, (match, mins, secs) => {
      const total = parseInt(mins) * 60 + parseInt(secs);
      return `[${mins}:${secs}](/sessions/${sessionId}?content_panel=transcript&jump=${total})`;
    })
    .replace(/\)\[/g, ') [');
}

export function SessionDetail({ session, onRefresh, onDeleted, onBack, isMobile, fields, onSelectPerson, routeQuery }) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const renameRef = useRef(null);
  const playerRef = useRef(null);
  const [playbackTime, setPlaybackTime] = useState(0);
  const [exportOpen, setExportOpen] = useState(false);
  const exportRef = useRef(null);
  const [tagPicker, setTagPicker] = useState(null); // { anchorPoint, items } or null
  const [notes, setNotes] = useState(session?.notes || '');
  const [activeTab, setActiveTab] = useState('transcript');
  const [summary, setSummary] = useState(null);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState(null);
  const [regenPrompt, setRegenPrompt] = useState(null); // null = hidden, string = visible
  const regenRef = useRef(null);
  const [notesSaving, setNotesSaving] = useState(false);
  const notesTimer = useRef(null);
  const [summaryElapsed, setSummaryElapsed] = useState(0);
  const summaryTimerRef = useRef(null);

  // Summary generation timer — derived from server-provided summary_started_at
  useEffect(() => {
    const startedAt = session?.summary_started_at;
    if (startedAt) {
      const calcElapsed = () => Math.max(0, Math.floor((Date.now() - new Date(startedAt).getTime()) / 1000));
      setSummaryElapsed(calcElapsed());
      summaryTimerRef.current = setInterval(() => setSummaryElapsed(calcElapsed()), 1000);
    } else {
      clearInterval(summaryTimerRef.current);
      setSummaryElapsed(0);
    }
    return () => clearInterval(summaryTimerRef.current);
  }, [session?.summary_started_at]);

  // Update page title to reflect current session
  useEffect(() => {
    const prev = document.title;
    if (session?.name) document.title = `Meeting Notes - ${session.name}`;
    return () => { document.title = prev; };
  }, [session?.name]);

  // Reset state when switching sessions
  useEffect(() => {
    setNotes(session?.notes || '');
    setSummaryLoading(false);
    setSummary(null);
    setSummaryError(null);
    setRegenPrompt(null);
    setActiveTab('transcript');
  }, [session?.id]);

  // Sync notes when updated externally
  useEffect(() => { setNotes(session?.notes || ''); }, [session?.notes]);

  // Handle route query params (content_panel, jump)
  useEffect(() => {
    if (!routeQuery || !session) return;
    if (routeQuery.contentPanel) {
      setActiveTab(routeQuery.contentPanel);
    }
    if (routeQuery.jump != null && playerRef.current) {
      setTimeout(() => {
        if (playerRef.current) playerRef.current.seekAndPlay(routeQuery.jump);
      }, 300); // small delay for player to mount
    }
    // Clear query params from URL after handling
    if (routeQuery.contentPanel || routeQuery.jump != null) {
      const cleanUrl = window.location.pathname;
      history.replaceState(null, '', cleanUrl);
    }
  }, [session?.id, routeQuery]);

  // Load summary when tab switches or summary becomes available
  useEffect(() => {
    if (activeTab !== 'summary' || !session?.summary_available) {
      setSummary(null);
      return;
    }
    let cancelled = false;
    // Only show loading spinner if we don't already have streamed content
    if (!session?.summary_streaming) {
      setSummaryLoading(true);
    }
    setSummaryError(null);
    api(`/sessions/${session.id}/summary`).then(data => {
      if (!cancelled) { setSummary(data); setSummaryLoading(false); }
    }).catch(e => {
      if (!cancelled) { setSummaryError(e.message); setSummaryLoading(false); }
    });
    return () => { cancelled = true; };
  }, [activeTab, session?.id, session?.summary_available]);

  // Ensure marked is loaded when summary tab is shown
  const [markedReady, setMarkedReady] = useState(!!markedModule);
  useEffect(() => {
    if (activeTab === 'summary' && !markedModule) {
      ensureMarked(() => setMarkedReady(true));
    }
  }, [activeTab]);

  async function generateSummary(additionalInstructions) {
    setSummaryLoading(true);
    setSummaryError(null);
    setRegenPrompt(null);
    const opts = {
      method: 'POST',
      body: JSON.stringify({ additional_instructions: additionalInstructions?.trim() || null }),
    };
    try {
      await api(`/sessions/${session.id}/summarize`, opts);
    } catch (e) {
      setSummaryError(e.message);
      setSummaryLoading(false);
    }
  }

  function startRegenerate() {
    setRegenPrompt('');
    setTimeout(() => regenRef.current?.focus(), 0);
  }

  async function submitRegenerate() {
    const instructions = regenPrompt;
    if (session.summary_available) {
      await api(`/sessions/${session.id}/summary`, { method: 'DELETE' });
      setSummary(null);
    }
    generateSummary(instructions);
  }

  // Auto-stop countdown: tick down every second between backend updates.
  const [autoStopCountdown, setAutoStopCountdown] = useState(null);
  const autoStopDeadlineRef = useRef(null);

  useEffect(() => {
    const remaining = session?.auto_stop_remaining_secs;
    if (remaining != null) {
      autoStopDeadlineRef.current = Date.now() + remaining * 1000;
      setAutoStopCountdown(remaining);
    } else {
      autoStopDeadlineRef.current = null;
      setAutoStopCountdown(null);
    }
  }, [session?.auto_stop_remaining_secs]);

  useEffect(() => {
    if (autoStopDeadlineRef.current == null) return;
    const timer = setInterval(() => {
      const remaining = Math.max(0, Math.ceil((autoStopDeadlineRef.current - Date.now()) / 1000));
      setAutoStopCountdown(remaining);
    }, 1000);
    return () => clearInterval(timer);
  }, [session?.auto_stop_remaining_secs]);

  function handleNotesChange(e) {
    const val = e.target.value;
    setNotes(val);
    // Auto-save after 800ms of inactivity
    clearTimeout(notesTimer.current);
    notesTimer.current = setTimeout(async () => {
      setNotesSaving(true);
      try {
        await api(`/sessions/${session.id}`, {
          method: 'PATCH',
          body: JSON.stringify({ notes: val || null }),
        });
      } catch {}
      setNotesSaving(false);
    }, 800);
  }

  // Close export dropdown on outside click
  useEffect(() => {
    if (!exportOpen) return;
    const handler = (e) => { if (exportRef.current && !exportRef.current.contains(e.target)) setExportOpen(false); };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [exportOpen]);

  async function openTagPicker(e) {
    const rect = e.currentTarget.getBoundingClientRect();
    let items = [];
    try { const data = await api('/tags'); items = data.tags || []; } catch {}
    const currentTags = new Set(session.tags || []);
    setTagPicker({
      anchorPoint: { x: rect.left, y: rect.bottom },
      items: items.filter(t => !currentTags.has(t.name)).map(t => ({ id: t.name, label: t.name })),
    });
  }

  async function addTag(tagName) {
    setTagPicker(null);
    const newTags = [...(session.tags || []), tagName];
    try {
      await api(`/sessions/${session.id}/tags`, { method: 'PUT', body: JSON.stringify({ tags: newTags }) });
      await onRefresh();
    } catch (e) { setError(e.message); }
  }

  async function createAndAddTag(name) {
    setTagPicker(null);
    try {
      await api('/tags', { method: 'POST', body: JSON.stringify({ name }) });
      const newTags = [...(session.tags || []), name];
      await api(`/sessions/${session.id}/tags`, { method: 'PUT', body: JSON.stringify({ tags: newTags }) });
      await onRefresh();
    } catch (e) { setError(e.message); }
  }

  async function removeTag(tagName) {
    const newTags = (session.tags || []).filter(t => t !== tagName);
    try {
      await api(`/sessions/${session.id}/tags`, { method: 'PUT', body: JSON.stringify({ tags: newTags }) });
      await onRefresh();
    } catch (e) { setError(e.message); }
  }

  if (!session) {
    return jsx('div', {
      className: 'h-full flex items-center justify-center px-4',
      children: jsx('p', {
        className: 'text-gray-400 dark:text-gray-600 text-sm text-center',
        children: 'Select a session or create a new one',
      }),
    });
  }

  async function action(fn) {
    setLoading(true);
    setError(null);
    try { await fn(); await onRefresh(); }
    catch (e) { setError(e.message); }
    finally { setLoading(false); }
  }

  function downloadFile(filename, content, mime = 'text/plain') {
    const blob = new Blob([content], { type: mime + ';charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = filename; a.click();
    URL.revokeObjectURL(url);
  }

  function fmtLrcTime(secs) {
    const m = Math.floor(secs / 60);
    const s = (secs % 60).toFixed(2);
    return `${m.toString().padStart(2, '0')}:${s.padStart(5, '0')}`;
  }

  async function exportLrc() {
    setExportOpen(false);
    const t = await api(`/sessions/${session.id}/transcript`);
    const lines = (t.segments || []).map(seg => {
      const speaker = seg.person_name || seg.speaker || '';
      const time = fmtLrcTime(seg.start || 0);
      return `[${time}] ${speaker}: ${seg.text}`;
    });
    downloadFile(`${session.name || session.id}.lrc`, lines.join('\n'));
  }

  function langName(code) {
    const opts = fields?.language?.options || [];
    const match = opts.find(o => o.value === code);
    return match ? match.label : code;
  }

  async function exportChatGpt() {
    setExportOpen(false);
    const [t, settings] = await Promise.all([
      api(`/sessions/${session.id}/transcript`),
      api('/settings'),
    ]);
    const parts = [];
    const prompt = settings.summarization_prompt || session.summarization_instruction;
    if (prompt) { parts.push(prompt); parts.push('\n'); }
    const lang = session.language || t.language || 'en';
    parts.push(`Language: ${langName(lang)}\n---\n`);
    for (const seg of (t.segments || [])) {
      const speaker = seg.person_name || seg.speaker || 'Unknown Speaker';
      const m = Math.floor((seg.start || 0) / 60);
      const s = Math.floor((seg.start || 0) % 60);
      parts.push(`${speaker} [${m}:${s.toString().padStart(2, '0')}]: ${seg.text}`);
    }
    downloadFile(`${session.name || session.id}.txt`, parts.join('\n'));
  }

  function exportSummaryMarkdown() {
    setExportOpen(false);
    if (!summary?.content) return;
    downloadFile(`${session.name || session.id}_summary.md`, summary.content, 'text/markdown');
  }

  function exportSummaryHtml() {
    setExportOpen(false);
    if (!summary?.content) return;
    const html = `<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>${(session.name || session.id).replace(/</g, '&lt;')}</title>
<style>body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;max-width:800px;margin:2rem auto;padding:0 1rem;line-height:1.6;color:#1a1a1a}
h1,h2,h3{margin-top:1.5em}ul,ol{padding-left:1.5em}hr{border:none;border-top:1px solid #ddd;margin:1.5em 0}
a{color:#4f46e5}code{background:#f3f4f6;padding:0.15em 0.3em;border-radius:3px;font-size:0.9em}</style>
</head><body>${renderMarkdown(summary.content)}</body></html>`;
    downloadFile(`${session.name || session.id}_summary.html`, html, 'text/html');
  }

  function exportSummaryPdf() {
    setExportOpen(false);
    if (!summary?.content) return;
    const html = renderMarkdown(summary.content);
    const title = (session.name || session.id).replace(/</g, '&lt;');
    const iframe = document.createElement('iframe');
    iframe.style.cssText = 'position:fixed;left:-9999px;width:800px;height:600px';
    document.body.appendChild(iframe);
    const doc = iframe.contentDocument;
    doc.open();
    doc.write(`<!DOCTYPE html>
<html lang="zh"><head><meta charset="utf-8"><title>${title}</title>
<style>body{font-family:"PingFang SC","Hiragino Sans GB","Microsoft YaHei","WenQuanYi Micro Hei",-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;max-width:800px;margin:2rem auto;padding:0 1rem;line-height:1.8;color:#1a1a1a}
h1,h2,h3{margin-top:1.5em}ul,ol{padding-left:1.5em}hr{border:none;border-top:1px solid #ddd;margin:1.5em 0}
a{color:#4f46e5}code{background:#f3f4f6;padding:0.15em 0.3em;border-radius:3px;font-size:0.9em}
@media print{body{margin:0;max-width:none}@page{margin:1.5cm}}</style>
</head><body>${html}</body></html>`);
    doc.close();
    iframe.onload = () => {
      iframe.contentWindow.print();
      setTimeout(() => document.body.removeChild(iframe), 1000);
    };
  }

  function startRename() {
    setRenameValue(session.name || '');
    setRenaming(true);
    setTimeout(() => renameRef.current?.focus(), 0);
  }

  async function submitRename() {
    setRenaming(false);
    const trimmed = renameValue.trim();
    if (trimmed === (session.name || '')) return;
    await api(`/sessions/${session.id}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: trimmed }),
    });
  }

  const s = session;
  const audioFiles = s.files.filter(f => f.endsWith('.wav') || f.endsWith('.mp3') || f.endsWith('.opus'));
  const hasAudio = s.state === 'stopped' && audioFiles.length > 0;

  return jsx('div', {
    className: 'h-full flex flex-col',
    children: jsxs(Fragment, { children: [
      // Header
      jsxs('div', {
        className: 'flex-shrink-0 px-4 md:px-6 py-3 md:py-4 border-b border-gray-200 dark:border-gray-800',
        children: [
          jsxs('div', { className: 'flex items-center justify-between gap-2', children: [
            jsxs('div', { className: 'flex items-center gap-2 min-w-0', children: [
              isMobile && jsx('button', {
                onClick: onBack,
                className: 'p-1 -ml-1 text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 transition-colors',
                children: jsx(BackIcon, {}),
              }),
              jsx('h2', { className: 'text-base md:text-lg font-semibold tracking-tight flex-shrink-0', children: 'Session' }),
              renaming
                ? jsxs(Fragment, { children: [
                    jsx('input', {
                      ref: renameRef,
                      value: renameValue,
                      onChange: e => setRenameValue(e.target.value),
                      onBlur: submitRename,
                      onKeyDown: e => { if (e.key === 'Enter') e.target.blur(); if (e.key === 'Escape') { setRenaming(false); } },
                      className: 'text-xs md:text-sm px-1.5 py-0.5 border border-blue-400 rounded outline-none bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-300 min-w-0 w-96',
                      placeholder: 'Session name...',
                    }),
                    jsx('span', { className: 'text-xs md:text-sm font-mono text-gray-500 dark:text-gray-400', children: `(${s.id})` }),
                  ]})
                : jsx('span', {
                    onDoubleClick: startRename,
                    className: 'text-xs md:text-sm text-gray-500 dark:text-gray-400 truncate cursor-default select-none',
                    title: 'Double-click to rename',
                    children: s.name
                      ? jsxs(Fragment, { children: [
                          s.name, ' ',
                          jsx('span', { className: 'font-mono', children: `(${s.id})` }),
                        ]})
                      : jsx('span', { className: 'font-mono', children: s.id }),
                  }),
              jsx(StateBadge, { state: s.state }),
            ]}),
            jsxs('div', { className: 'flex items-center gap-1.5 md:gap-2 flex-shrink-0', children: [
              s.state === 'created' && jsx('button', {
                disabled: loading,
                onClick: () => action(() => api(`/sessions/${s.id}/recording/start`, { method: 'POST' })),
                className: 'inline-flex items-center gap-1.5 px-3 md:px-4 py-1.5 md:py-2 rounded-lg text-xs md:text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-40 transition-colors',
                children: jsxs(Fragment, { children: [jsx(PlayIcon, {}), isMobile ? null : 'Record'] }),
              }),
              s.state === 'recording' && jsx('button', {
                disabled: loading,
                onClick: () => action(() => api(`/sessions/${s.id}/recording/stop`, { method: 'POST' })),
                className: 'inline-flex items-center gap-1.5 px-3 md:px-4 py-1.5 md:py-2 rounded-lg text-xs md:text-sm font-medium text-white bg-amber-600 hover:bg-amber-700 disabled:opacity-40 transition-colors',
                children: jsxs(Fragment, { children: [jsx(StopIcon, {}), isMobile ? null : 'Stop'] }),
              }),
              jsx('button', {
                disabled: loading,
                onClick: () => {
                  if (!confirm('Delete this session and all its files? This cannot be undone.')) return;
                  action(async () => {
                    await api(`/sessions/${s.id}`, { method: 'DELETE' });
                    onDeleted();
                  });
                },
                className: 'inline-flex items-center px-2.5 md:px-3 py-1.5 md:py-2 rounded-lg text-xs md:text-sm font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20 hover:bg-red-100 dark:hover:bg-red-900/40 disabled:opacity-40 transition-colors',
                children: 'Delete',
              }),
            ]}),
          ]}),
          error && jsx('p', { className: 'mt-2 text-sm text-red-500', children: error }),
        ],
      }),

      // Content
      jsx('div', {
        className: 'flex-1 overflow-y-auto px-4 md:px-6 py-4 md:py-5',
        children: jsxs('div', { className: 'max-w-3xl space-y-4 md:space-y-6', children: [
          // Info grid
          jsx('div', {
            className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4 md:p-5',
            children: jsxs('div', {
              className: 'grid grid-cols-2 md:grid-cols-3 gap-3 md:gap-4',
              children: [
                jsxs('div', { children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Language' }),
                  s.state === 'stopped'
                    ? jsx('select', {
                        value: s.language,
                        onChange: async (e) => {
                          try {
                            await api(`/sessions/${s.id}`, {
                              method: 'PATCH',
                              body: JSON.stringify({ language: e.target.value }),
                            });
                            await onRefresh();
                          } catch (err) { setError(err.message); }
                        },
                        className: 'text-sm font-medium bg-transparent border border-gray-200 dark:border-gray-700 rounded px-1 py-0.5 cursor-pointer hover:border-blue-400 transition-colors',
                        children: (fields?.language?.options || []).map(o =>
                          jsx('option', { key: o.value, value: o.value, children: o.label })
                        ),
                      })
                    : jsx('p', { className: 'text-sm font-medium', children: (fields?.language?.options?.find(o => o.value === s.language)?.label) || s.language }),
                ]}),
                jsxs('div', { children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Format' }),
                  jsx('p', { className: 'text-sm font-medium', children:
                    s.mp3
                      ? `MP3 / ${s.mp3.bitrate_kbps} kbps CBR @ ${s.mp3.sample_rate >= 1000 ? (s.mp3.sample_rate / 1000) + ' kHz' : s.mp3.sample_rate + ' Hz'}`
                      : s.opus
                        ? `Opus / ${s.opus.bitrate_kbps} kbps VBR / complexity ${s.opus.complexity}`
                        : `WAV / ${s.raw_sample_rate >= 1000 ? (s.raw_sample_rate / 1000) + ' kHz' : s.raw_sample_rate + ' Hz'}`
                  }),
                ]}),
                jsxs('div', { children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Created' }),
                  jsx('p', { className: 'text-sm font-medium', children: new Date(s.created_at).toLocaleString() }),
                ]}),
                s.updated_at !== s.created_at && jsxs('div', { children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Updated' }),
                  jsx('p', { className: 'text-sm font-medium', children: new Date(s.updated_at).toLocaleString() }),
                ]}),
                s.duration_secs != null && jsxs('div', { children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Duration' }),
                  jsx('p', { className: 'text-sm font-medium', children: formatDuration(s.duration_secs) }),
                ]}),
                s.sources && s.sources.length > 0 && jsxs('div', { className: 'col-span-2 md:col-span-3', children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-1', children: 'Sources' }),
                  jsx('div', { className: 'flex flex-wrap gap-1.5', children:
                    s.sources.map(src => jsx('span', {
                      key: src,
                      className: 'inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400',
                      children: src,
                    })),
                  }),
                ]}),
                s.summarization_instruction && jsxs('div', { className: 'col-span-2 md:col-span-3', children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Summarization' }),
                  jsx('p', { className: 'text-sm text-gray-600 dark:text-gray-400', children: s.summarization_instruction }),
                ]}),
                // Tags
                jsxs('div', { className: 'col-span-2 md:col-span-3', children: [
                  jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-1', children: 'Tags' }),
                  jsxs('div', { className: 'flex flex-wrap items-center gap-1.5', children: [
                    ...(s.tags || []).map(tag => jsxs('span', {
                      key: tag,
                      className: `inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium ${tagColor(tag)}`,
                      children: [
                        jsx(TagIcon, { className: 'w-2.5 h-2.5' }),
                        tag,
                        jsx('button', {
                          onClick: () => removeTag(tag),
                          className: 'ml-0.5 opacity-60 hover:opacity-100 transition-opacity',
                          title: `Remove tag "${tag}"`,
                          children: '\u00d7',
                        }),
                      ],
                    })),
                    // Add tag button
                    jsx('button', {
                      onClick: openTagPicker,
                      className: 'inline-flex items-center justify-center w-6 h-6 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors',
                      title: 'Add tag',
                      children: jsx(PlusIcon, {}),
                    }),
                    // Tag picker portal (rendered outside the grid)
                    tagPicker && jsx(SearchableList, {
                      items: tagPicker.items,
                      anchorPoint: tagPicker.anchorPoint,
                      placeholder: 'Search or create tag...',
                      onSelect: (item) => addTag(item.id),
                      onCreateAndSelect: (name) => createAndAddTag(name),
                      onClose: () => setTagPicker(null),
                    }),
                  ]}),
                ]}),
                // Notes
                jsxs('div', { className: 'col-span-2 md:col-span-3', children: [
                  jsxs('div', { className: 'flex items-center gap-2 mb-0.5', children: [
                    jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500', children: 'Notes' }),
                    notesSaving && jsx('span', { className: 'text-[10px] text-blue-500', children: 'Saving...' }),
                  ]}),
                  jsx('textarea', {
                    value: notes,
                    onChange: e => { handleNotesChange(e); autoResize(e); },
                    onInput: autoResize,
                    ref: el => { if (el) { el.style.height = 'auto'; autoResize({ target: el }); } },
                    placeholder: 'Add notes about this session...',
                    rows: 1,
                    className: INPUT_CLS + ' text-xs overflow-hidden',
                  }),
                ]}),
              ],
            }),
          }),

          // Recording indicator
          s.state === 'recording' && jsx('div', {
            className: 'rounded-xl border border-red-200 dark:border-red-900/40 bg-red-50 dark:bg-red-900/10 p-4 md:p-5',
            children: jsxs(Fragment, { children: [
              jsxs('div', { className: 'flex items-center gap-2 mb-3', children: [
                jsx('span', { className: 'w-2.5 h-2.5 rounded-full bg-red-500 animate-pulse-recording' }),
                jsx('p', { className: 'text-sm font-medium text-red-700 dark:text-red-300', children: 'Recording in progress' }),
                jsx('label', {
                  className: 'ml-auto flex items-center gap-1.5 cursor-pointer select-none',
                  title: 'When system audio stopped receiving new audio for 1 minute, the recording will be automatically stopped',
                  children: jsxs(Fragment, { children: [
                    jsx('input', {
                      type: 'checkbox',
                      checked: s.auto_stop || false,
                      onChange: (e) => {
                        api(`/sessions/${s.id}`, {
                          method: 'PATCH',
                          body: JSON.stringify({ auto_stop: e.target.checked }),
                        }).catch(() => {});
                      },
                      className: 'rounded border-red-300 text-red-600 focus:ring-red-500',
                    }),
                    jsx('span', {
                      className: 'text-xs text-red-600 dark:text-red-400',
                      children: 'Auto-stop',
                    }),
                  ]}),
                }),
              ]}),
              s.files.length > 0 && jsx('div', { className: 'flex flex-wrap gap-1.5', children:
                s.files.map(f => jsx('span', {
                  key: f,
                  className: 'inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] bg-red-100 text-red-600 dark:bg-red-900/30 dark:text-red-400 font-mono',
                  children: jsxs(Fragment, { children: [
                    f,
                    s.file_sizes && s.file_sizes[f] != null && jsx('span', {
                      className: 'text-red-400 dark:text-red-500',
                      children: formatFileSize(s.file_sizes[f]),
                    }),
                  ]}),
                })),
              }),
            ]}),
          }),

          // Auto-stop countdown notice
          autoStopCountdown != null && s.state === 'recording' && jsxs('div', {
            className: 'rounded-xl border p-4 border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-900/10 text-amber-800 dark:text-amber-200',
            children: [
              jsxs('div', { className: 'flex items-start gap-2', children: [
                jsx('span', { className: 'flex-shrink-0 text-sm', children: '\u23F1\uFE0F' }),
                jsx('p', { className: 'text-sm font-medium', children: `Auto-stopping in ${autoStopCountdown}s — system audio not receiving data` }),
              ]}),
            ],
          }),

          // Notices
          s.notices && s.notices.length > 0 && jsx('div', {
            className: 'space-y-2',
            children: s.notices.map((n, i) => {
              const colors = {
                warning: 'border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-900/10 text-amber-800 dark:text-amber-200',
                error: 'border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-900/10 text-red-800 dark:text-red-200',
                info: 'border-blue-300 dark:border-blue-700 bg-blue-50 dark:bg-blue-900/10 text-blue-800 dark:text-blue-200',
              };
              const icons = { warning: '\u26A0\uFE0F', error: '\u274C', info: '\u2139\uFE0F' };
              return jsxs('div', {
                key: i,
                className: `rounded-xl border p-4 ${colors[n.level] || colors.info}`,
                children: [
                  jsxs('div', { className: 'flex items-start gap-2', children: [
                    jsx('span', { className: 'flex-shrink-0 text-sm', children: icons[n.level] || icons.info }),
                    jsxs('div', { className: 'flex-1 min-w-0', children: [
                      jsx('p', { className: 'text-sm font-medium', children: n.message }),
                      n.details && jsx('p', { className: 'text-xs mt-1 opacity-80', children: n.details }),
                      n.platform && jsx('span', {
                        className: 'inline-block mt-1.5 px-1.5 py-0.5 rounded text-[10px] font-medium bg-black/5 dark:bg-white/5',
                        children: n.platform,
                      }),
                    ]}),
                  ]}),
                ],
              });
            }),
          }),

          // Audio player
          hasAudio && jsx('div', {
            className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4 md:p-5',
            children: jsxs(Fragment, { children: [
              jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3', children: 'Recordings' }),
              jsx(SyncedPlayer, {
                ref: playerRef,
                sessionId: s.id,
                onTimeUpdate: setPlaybackTime,
                files: audioFiles.map(f => {
                  const ext = f.split('.').pop();
                  const meta = (s.source_meta || []).find(src => src.filename === f);
                  return {
                    name: f,
                    label: meta?.source_label || f.replace(`.${ext}`, '').replace(/_/g, ' '),
                    sourceType: meta?.source_type || null,
                  };
                }),
              }),
            ]}),
          }),

          // Transcription controls
          s.state === 'stopped' && !s.transcript_available && !s.processing_state && jsx('div', {
            className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4 md:p-5',
            children: jsx('button', {
              disabled: loading,
              onClick: () => action(() => api(`/sessions/${s.id}/transcribe`, { method: 'POST' })),
              className: 'w-full flex justify-center items-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium text-white bg-indigo-600 hover:bg-indigo-700 disabled:opacity-50 transition-colors',
              children: jsxs(Fragment, { children: [
                jsx(TranscriptIcon, {}),
                'Transcribe',
              ]}),
            }),
          }),

          // Processing indicator
          s.processing_state && jsx('div', {
            className: 'rounded-xl border border-indigo-200 dark:border-indigo-900/40 bg-indigo-50 dark:bg-indigo-900/10 p-4 md:p-5',
            children: jsxs('div', { className: 'flex items-center gap-3', children: [
              jsx('div', { className: 'w-5 h-5 border-2 border-indigo-400 border-t-transparent rounded-full animate-spin flex-shrink-0' }),
              jsx('p', { className: 'text-sm font-medium text-indigo-700 dark:text-indigo-300', children: PROCESSING_LABELS[s.processing_state] || s.processing_state }),
            ]}),
          }),

          // Transcript / Summary tabbed viewer
          s.transcript_available && jsx('div', {
            className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4 md:p-5',
            children: jsxs(Fragment, { children: [
              jsxs('div', { className: 'flex items-center justify-between mb-3', children: [
                // Tab buttons
                jsxs('div', { className: 'flex gap-1', children: [
                  jsx('button', {
                    onClick: () => setActiveTab('transcript'),
                    className: `px-2 py-1 rounded text-[11px] font-medium uppercase tracking-wider transition-colors ${activeTab === 'transcript' ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-gray-100' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'}`,
                    children: 'Transcript',
                  }),
                  jsx('button', {
                    onClick: () => setActiveTab('summary'),
                    className: `px-2 py-1 rounded text-[11px] font-medium uppercase tracking-wider transition-colors ${activeTab === 'summary' ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-gray-100' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'}`,
                    children: 'Summary',
                  }),
                ]}),
                // Tab-specific controls
                activeTab === 'transcript' && jsxs('div', { className: 'flex items-baseline gap-2', children: [
                  jsxs('div', { ref: exportRef, className: 'relative inline-block', children: [
                    jsx('button', {
                      onClick: () => setExportOpen(v => !v),
                      className: 'text-[11px] text-gray-400 hover:text-blue-500 transition-colors',
                      children: 'Export',
                    }),
                    exportOpen && jsx('div', {
                      className: 'absolute right-0 top-full mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1 z-10 min-w-[180px]',
                      children: [
                        jsx('button', {
                          key: 'lrc',
                          onClick: exportLrc,
                          className: 'w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors',
                          children: 'Lyrics .lrc',
                        }),
                        jsx('button', {
                          key: 'chatgpt',
                          onClick: exportChatGpt,
                          className: 'w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors',
                          children: 'ChatGPT messages .txt',
                        }),
                      ],
                    }),
                  ]}),
                  jsx('button', {
                    onClick: () => {
                      if (!confirm('Re-transcribe will delete the current transcript and summary. Continue?')) return;
                      action(async () => {
                        await api(`/sessions/${s.id}/transcript`, { method: 'DELETE' });
                      });
                    },
                    className: 'text-[11px] text-gray-400 hover:text-red-500 transition-colors',
                    children: 'Re-transcribe',
                  }),
                ]}),
                activeTab === 'summary' && s.summary_available && regenPrompt == null && jsxs('div', { className: 'flex items-baseline gap-2', children: [
                  jsxs('div', { ref: exportRef, className: 'relative inline-block', children: [
                    jsx('button', {
                      onClick: () => setExportOpen(v => !v),
                      className: 'text-[11px] text-gray-400 hover:text-blue-500 transition-colors',
                      children: 'Export',
                    }),
                    exportOpen && jsx('div', {
                      className: 'absolute right-0 top-full mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1 z-10 min-w-[180px]',
                      children: [
                        jsx('button', {
                          key: 'md',
                          onClick: exportSummaryMarkdown,
                          className: 'w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors',
                          children: 'Markdown .md',
                        }),
                        jsx('button', {
                          key: 'html',
                          onClick: exportSummaryHtml,
                          className: 'w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors',
                          children: 'HTML .html',
                        }),
                        jsx('button', {
                          key: 'pdf',
                          onClick: exportSummaryPdf,
                          className: 'w-full text-left px-3 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors',
                          children: 'PDF (Print)',
                        }),
                      ],
                    }),
                  ]}),
                  jsx('button', {
                    onClick: startRegenerate,
                    className: 'text-[11px] text-gray-400 hover:text-blue-500 transition-colors',
                    children: 'Re-generate',
                  }),
                ]}),
              ]}),
              // Tab content
              activeTab === 'transcript' && jsx(TranscriptViewer, {
                sessionId: s.id,
                currentTime: playbackTime,
                onSeek: (t) => {
                  if (playerRef.current) playerRef.current.seekAndPlay(t);
                },
                onSpeakerUpdate: onRefresh,
              }),
              activeTab === 'summary' && jsxs('div', { className: 'mt-2', children: [
                // Additional instructions input (shown for both generate and re-generate)
                regenPrompt != null && jsxs('div', { className: 'mb-3 space-y-2', children: [
                  jsx('textarea', {
                    ref: regenRef,
                    value: regenPrompt,
                    onChange: e => setRegenPrompt(e.target.value),
                    onKeyDown: e => {
                      if (e.key === 'Enter' && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
                        e.preventDefault();
                        submitRegenerate();
                      }
                      if (e.key === 'Escape') { setRegenPrompt(null); }
                    },
                    onInput: autoResize,
                    placeholder: 'Additional instructions (optional). Press Enter to generate, Esc to cancel...',
                    rows: 1,
                    className: INPUT_CLS + ' text-sm overflow-hidden',
                  }),
                  jsxs('div', { className: 'flex gap-2 justify-end', children: [
                    jsx('button', {
                      onClick: () => setRegenPrompt(null),
                      className: 'px-3 py-1 rounded text-xs text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors',
                      children: 'Cancel',
                    }),
                    jsx('button', {
                      onClick: submitRegenerate,
                      className: 'px-3 py-1 rounded text-xs font-medium text-white bg-indigo-600 hover:bg-indigo-700 transition-colors',
                      children: s.summary_available ? 'Re-generate' : 'Generate Summary',
                    }),
                  ]}),
                ]}),
                // Summary content
                s.summary_processing
                  ? jsxs('div', { children: [
                      s.summary_streaming
                        ? jsx('div', {
                            className: 'md-content text-sm text-gray-900 dark:text-gray-100',
                            dangerouslySetInnerHTML: { __html: renderMarkdown(convertCitations(s.summary_streaming, s.id)) },
                          })
                        : null,
                      jsxs('div', { className: 'flex items-center gap-3 py-4 justify-center', children: [
                        jsx('div', { className: 'w-4 h-4 border-2 border-indigo-400 border-t-transparent rounded-full animate-spin' }),
                        jsx('p', { className: 'text-xs text-gray-500', children:
                          (s.summary_streaming ? 'Streaming' : s.summary_processing === 'thinking' ? 'Thinking' : 'Generating summary')
                          + (summaryElapsed > 0 ? ` (${summaryElapsed}s)` : '')
                          + '...',
                        }),
                      ]}),
                    ]})
                  : summary?.content
                    ? jsx('div', {
                        className: 'md-content text-sm text-gray-900 dark:text-gray-100',
                        dangerouslySetInnerHTML: { __html: renderMarkdown(convertCitations(summary.content, s.id)) },
                        ref: el => {
                          if (!el) return;
                          el.querySelectorAll('input[type="checkbox"]').forEach((cb, idx) => {
                            cb.disabled = false;
                            const toggle = () => {
                              let md = summary.content;
                              let n = 0;
                              md = md.replace(/- \[([ xX])\]/g, (match, check) => {
                                if (n++ === idx) return check.trim() ? '- [ ]' : '- [x]';
                                return match;
                              });
                              const updated = { ...summary, content: md };
                              setSummary(updated);
                              api(`/sessions/${s.id}/summary`, { method: 'PATCH', body: JSON.stringify({ content: md }) });
                            };
                            cb.onclick = e => { e.stopPropagation(); toggle(); };
                            // Make the whole <li> clickable
                            const li = cb.closest('li');
                            if (li) {
                              li.style.cursor = 'pointer';
                              li.onclick = e => { if (e.target.tagName !== 'INPUT' && e.target.tagName !== 'A') toggle(); };
                            }
                          });
                          // Intercept citation links for in-app navigation
                          el.querySelectorAll('a[href*="/sessions/"]').forEach(a => {
                            const href = a.getAttribute('href');
                            if (href && href.includes('jump=')) {
                              a.style.cursor = 'pointer';
                              a.onclick = e => {
                                e.preventDefault();
                                e.stopPropagation();
                                const url = new URL(href, window.location.origin);
                                const jump = parseFloat(url.searchParams.get('jump'));
                                if (!isNaN(jump) && playerRef.current) {
                                  setActiveTab('transcript');
                                  playerRef.current.seekAndPlay(jump);
                                }
                              };
                            }
                          });
                        },
                      })
                    : s.summary_streaming
                      ? jsx('div', {
                          className: 'md-content text-sm text-gray-900 dark:text-gray-100',
                          dangerouslySetInnerHTML: { __html: renderMarkdown(convertCitations(s.summary_streaming, s.id)) },
                        })
                    : summaryError
                      ? jsx('p', { className: 'text-sm text-red-500 py-4 text-center', children: `Error: ${summaryError}` })
                      : regenPrompt == null && jsx('div', { className: 'py-8 text-center', children:
                          jsx('button', {
                            onClick: startRegenerate,
                            disabled: loading,
                            className: 'px-4 py-2 rounded-lg text-sm font-medium text-white bg-indigo-600 hover:bg-indigo-700 disabled:opacity-50 transition-colors',
                            children: 'Generate Summary',
                          }),
                        }),
              ]}),
            ]}),
          }),

          // Speaker attribution
          s.transcript_available && jsx(SpeakerAttributionWrapper, {
            sessionId: s.id,
            onUpdate: onRefresh,
            onSelectPerson,
          }),

          // All files
          s.files.length > 0 && s.state === 'stopped' && jsx('div', {
            className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4 md:p-5',
            children: jsxs(Fragment, { children: [
              jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-2', children: 'All Files' }),
              jsx('div', { className: 'space-y-1', children:
                s.files.map(f => jsxs('div', {
                  key: f,
                  className: 'flex items-center justify-between py-1 gap-2',
                  children: [
                    jsx('span', { className: 'text-sm font-mono text-gray-600 dark:text-gray-400 truncate min-w-0', children: f }),
                    jsxs('span', { className: 'flex items-center gap-2 flex-shrink-0', children: [
                      s.file_sizes && s.file_sizes[f] != null && jsx('span', {
                        className: 'text-[11px] text-gray-400 dark:text-gray-500 font-mono',
                        children: formatFileSize(s.file_sizes[f]),
                      }),
                      jsx('a', {
                        href: `${API}/sessions/${s.id}/files/${encodeURIComponent(f)}`,
                        target: '_blank',
                        className: 'text-xs text-blue-600 dark:text-blue-400 hover:underline',
                        children: 'download',
                      }),
                    ]}),
                  ],
                })),
              }),
            ]}),
          }),

          jsxs('p', {
            className: 'text-[11px] text-gray-300 dark:text-gray-700 font-mono break-all',
            children: ['ID: ', s.id],
          }),
        ]}),
      }),
    ]}),
  });
}

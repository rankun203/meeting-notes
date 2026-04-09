import { useState, useEffect, useRef } from 'react';
import { jsx, jsxs, Fragment, API } from '../utils.mjs';
import { SendIcon, SpinnerIcon, ExportIcon, StopSquareIcon } from '../icons.mjs';
import { MentionPopup } from './mentions.mjs';

export function InputComposer({ onSend, onStop, streaming, mentionData, conversationId, onSendToClaudeCode }) {
  const [text, setText] = useState('');
  const [mentions, setMentions] = useState([]);
  const [showMention, setShowMention] = useState(false);
  const [mentionQuery, setMentionQuery] = useState('');
  const textareaRef = useRef(null);
  const mentionRef = useRef(null);

  useEffect(() => {
    if (textareaRef.current) textareaRef.current.focus();
  }, []);

  // Auto-resize textarea after React re-render
  useEffect(() => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = 'auto';
      el.style.height = Math.min(el.scrollHeight, 96) + 'px';
    }
  }, [text]);

  function hasMentionMatches(query) {
    const q = query.toLowerCase();
    for (const t of (mentionData.tags || [])) { if (t.name.toLowerCase().includes(q)) return true; }
    for (const p of (mentionData.people || [])) { if (p.name.toLowerCase().includes(q)) return true; }
    for (const s of (mentionData.sessions || [])) { if ((s.name || s.id).toLowerCase().includes(q)) return true; }
    return false;
  }

  function handleInput(e) {
    const el = e.target;
    const val = el.value;
    const cursor = el.selectionStart;
    setText(val);

    const textBefore = val.slice(0, cursor);
    const atIdx = textBefore.lastIndexOf('@');
    if (atIdx >= 0 && (atIdx === 0 || textBefore[atIdx - 1] === ' ' || textBefore[atIdx - 1] === '\n')) {
      const query = textBefore.slice(atIdx + 1);
      // Allow spaces in query only while there are still matches
      const hasSpace = query.includes(' ');
      if (!hasSpace || hasMentionMatches(query)) {
        setShowMention(true);
        setMentionQuery(query);
        return;
      }
    }
    setShowMention(false);
  }

  function handleKeyDown(e) {
    // Delegate to mention popup when open
    if (showMention && mentionRef.current) {
      const handled = mentionRef.current.handleKey(e);
      if (handled) return;
    }
    if (e.key === 'Escape' && showMention) {
      e.preventDefault();
      setShowMention(false);
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey && !showMention) {
      e.preventDefault();
      send();
    }
  }

  function handleMentionSelect(item) {
    const el = textareaRef.current;
    const cursor = el.selectionStart;
    const before = text.slice(0, cursor);
    const atIdx = before.lastIndexOf('@');
    const after = text.slice(cursor);
    const tag = item.kind === 'session' ? `@${item.kind}:${item.label} (${item.id}) ` : `@${item.kind}:${item.label} `;
    const newText = before.slice(0, atIdx) + tag + after;
    setText(newText);
    const hasSummary = item.summary_available ?? false;
    const hasTranscript = item.transcript_available ?? true;
    const defaultMode = (item.kind === 'tag' || item.kind === 'person') ? 'summary' : (hasSummary ? 'summary' : 'transcript');
    setMentions(prev => [...prev, { kind: item.kind, id: item.id, label: item.label, context_mode: defaultMode, summary_available: hasSummary, transcript_available: hasTranscript }]);
    setShowMention(false);

    setTimeout(() => {
      if (textareaRef.current) {
        const pos = atIdx + tag.length;
        textareaRef.current.focus();
        textareaRef.current.setSelectionRange(pos, pos);
      }
    }, 0);
  }

  function stripMentions(str, mentionList) {
    let result = str;
    for (const m of mentionList) {
      if (m.kind === 'session') {
        result = result.replace(`@${m.kind}:${m.label} (${m.id}) `, '');
        result = result.replace(`@${m.kind}:${m.label} (${m.id})`, '');
      }
      result = result.replace(`@${m.kind}:${m.label} `, '');
      result = result.replace(`@${m.kind}:${m.label}`, '');
    }
    return result.trim();
  }

  function send() {
    const trimmed = text.trim();
    if (!trimmed || streaming) return;
    // In Claude Code mode, keep @mentions in the text as Claude Code reads them inline
    const content = onSendToClaudeCode ? trimmed : (stripMentions(trimmed, mentions) || trimmed);
    onSend(content, mentions);
    setText('');
    setMentions([]);
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.focus();
    }
  }

  const [shakeIdx, setShakeIdx] = useState(null);

  function cycleMode(idx) {
    const m = mentions[idx];
    const hasSummary = m.summary_available ?? false;
    const hasTranscript = m.transcript_available ?? true;
    // Build list of available modes
    const available = [];
    if (hasTranscript) available.push('transcript');
    if (hasSummary) available.push('summary');
    if (hasTranscript && hasSummary) available.push('both');
    // For tags (no direct availability info), allow all modes
    if (m.kind === 'tag') { available.length = 0; available.push('transcript', 'summary', 'both'); }

    if (available.length <= 1) {
      // Shake — can't switch
      setShakeIdx(idx);
      setTimeout(() => setShakeIdx(null), 400);
      return;
    }

    const cur = available.indexOf(m.context_mode || 'transcript');
    const next = available[(cur + 1) % available.length];
    setMentions(prev => prev.map((item, i) => i === idx ? { ...item, context_mode: next } : item));
  }

  const MODE_LABELS = { transcript: 'T', summary: 'S', both: 'T+S' };
  const MODE_TITLES = { transcript: 'Transcript — click to switch', summary: 'Summary — click to switch', both: 'Transcript + Summary — click to switch' };

  // Resolve what will actually be sent, considering availability
  function effectiveMode(m) {
    const mode = m.context_mode || 'summary';
    if (m.kind === 'tag') return mode; // tags: no per-session info, trust the mode
    const hasSummary = m.summary_available ?? false;
    const hasTranscript = m.transcript_available ?? true;
    if (mode === 'summary' && !hasSummary) return 'transcript';
    if (mode === 'transcript' && !hasTranscript) return 'summary';
    if (mode === 'both') {
      if (!hasSummary) return 'transcript';
      if (!hasTranscript) return 'summary';
    }
    return mode;
  }

  const pills = mentions.length > 0 ? jsx('div', {
    className: 'flex flex-wrap gap-1 px-3 pt-2',
    children: mentions.map((m, i) => {
      const effective = effectiveMode(m);
      const isFallback = effective !== (m.context_mode || 'summary');
      return jsxs('span', {
        key: i,
        className: `inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 text-[10px] font-medium ${shakeIdx === i ? 'mention-shake' : ''}`,
        children: [
          m.kind === 'tag' ? '#' : m.kind === 'person' ? '\u{1F464}' : '\u{1F4DD}',
          m.label,
          jsx('button', {
            onClick: () => cycleMode(i),
            title: isFallback
              ? `${MODE_TITLES[effective]} (fallback — ${m.context_mode} not available)`
              : MODE_TITLES[effective],
            className: `ml-0.5 px-1 py-px rounded text-[9px] font-bold transition-colors ${isFallback ? 'bg-yellow-200 dark:bg-yellow-800 text-yellow-700 dark:text-yellow-300' : 'bg-blue-200 dark:bg-blue-800 text-blue-600 dark:text-blue-300 hover:bg-blue-300 dark:hover:bg-blue-700'}`,
            children: MODE_LABELS[effective],
          }),
          jsx('button', {
            onClick: () => setMentions(prev => prev.filter((_, j) => j !== i)),
            className: 'ml-0.5 text-blue-400 hover:text-blue-600',
            children: '\u00d7',
          }),
        ],
      });
    }),
  }) : null;

  return jsx('div', {
    className: 'border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 rounded-b-2xl relative',
    children: jsxs(Fragment, { children: [
      pills,
      jsxs('div', {
        className: 'p-3 flex items-end gap-2 relative',
        children: [
          jsx('textarea', {
            ref: textareaRef,
            value: text,
            onInput: handleInput,
            onKeyDown: handleKeyDown,
            placeholder: 'Type a message... Use @ to add context',
            rows: 1,
            className: 'flex-1 resize-none rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 px-3 py-2 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-blue-500 placeholder-gray-400 dark:placeholder-gray-500',
            style: { maxHeight: '96px' },
          }),
          conversationId && jsx('button', {
            onClick: async () => {
              if (onSendToClaudeCode) {
                onSendToClaudeCode();
                return;
              }
              try {
                const res = await fetch(`${API}/conversations/${conversationId}/export-prompt`);
                if (!res.ok) throw new Error(`HTTP ${res.status}`);
                let content = await res.text();
                // Append current draft message if present
                const draft = stripMentions(text.trim(), mentions);
                if (draft) content += '\n\n=== USER ===\n\n' + draft + '\n';
                // Download as text file
                const blob = new Blob([content], { type: 'text/plain' });
                const url = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url;
                a.download = `prompt-${conversationId}.txt`;
                a.click();
                URL.revokeObjectURL(url);
              } catch (e) {
                alert('Export failed: ' + e.message);
              }
            },
            title: 'Download prompt as text file',
            className: 'flex-shrink-0 w-8 h-8 rounded-full bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 flex items-center justify-center transition-colors',
            children: jsx(ExportIcon, { className: 'w-3.5 h-3.5 text-gray-600 dark:text-gray-300' }),
          }),
          streaming
            ? jsx('button', {
                onClick: onStop,
                className: 'flex-shrink-0 w-8 h-8 rounded-full bg-red-500 hover:bg-red-600 flex items-center justify-center transition-colors',
                title: 'Stop',
                children: jsx(StopSquareIcon, { className: 'w-3.5 h-3.5 text-white' }),
              })
            : jsx('button', {
                onClick: send,
                disabled: !text.trim(),
                className: 'flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 hover:bg-blue-700 disabled:opacity-30 disabled:cursor-not-allowed flex items-center justify-center transition-colors',
                children: jsx(SendIcon, { className: 'w-3.5 h-3.5 text-white' }),
              }),
        ],
      }),
      jsx('div', {
        className: 'absolute bottom-full left-0 right-0',
        style: { display: showMention ? '' : 'none' },
        children: jsx(MentionPopup, {
          ref: mentionRef,
          query: mentionQuery,
          onSelect: handleMentionSelect,
          mentionData,
        }),
      }),
    ]}),
  });
}

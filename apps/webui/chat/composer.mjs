import { useState, useEffect, useRef } from 'react';
import { jsx, jsxs, Fragment, API } from '../utils.mjs';
import { SendIcon, SpinnerIcon, ExportIcon } from '../icons.mjs';
import { MentionPopup } from './mentions.mjs';

export function InputComposer({ onSend, disabled, mentionData, conversationId }) {
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

  function handleInput(e) {
    const el = e.target;
    const val = el.value;
    const cursor = el.selectionStart;
    setText(val);

    const textBefore = val.slice(0, cursor);
    const atIdx = textBefore.lastIndexOf('@');
    if (atIdx >= 0 && (atIdx === 0 || textBefore[atIdx - 1] === ' ' || textBefore[atIdx - 1] === '\n')) {
      const query = textBefore.slice(atIdx + 1);
      if (!query.includes(' ') && query.length < 40) {
        setShowMention(true);
        setMentionQuery(query);
        // Preserve focus and cursor after state update
        requestAnimationFrame(() => {
          if (textareaRef.current && document.activeElement !== textareaRef.current) {
            textareaRef.current.focus();
            textareaRef.current.setSelectionRange(cursor, cursor);
          }
        });
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
    const tag = `@${item.kind}:${item.label} `;
    const newText = before.slice(0, atIdx) + tag + after;
    setText(newText);
    setMentions(prev => [...prev, { kind: item.kind, id: item.id, label: item.label }]);
    setShowMention(false);

    setTimeout(() => {
      if (textareaRef.current) {
        const pos = atIdx + tag.length;
        textareaRef.current.focus();
        textareaRef.current.setSelectionRange(pos, pos);
      }
    }, 0);
  }

  function send() {
    const trimmed = text.trim();
    if (!trimmed || disabled) return;
    const cleanContent = trimmed.replace(/@(tag|person|session):\S+\s?/g, '').trim();
    onSend(cleanContent || trimmed, mentions);
    setText('');
    setMentions([]);
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }

  const pills = mentions.length > 0 ? jsx('div', {
    className: 'flex flex-wrap gap-1 px-3 pt-2',
    children: mentions.map((m, i) =>
      jsxs('span', {
        key: i,
        className: 'inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 text-[10px] font-medium',
        children: [
          m.kind === 'tag' ? '#' : m.kind === 'person' ? '\u{1F464}' : '\u{1F4DD}',
          m.label,
          jsx('button', {
            onClick: () => setMentions(prev => prev.filter((_, j) => j !== i)),
            className: 'ml-0.5 text-blue-400 hover:text-blue-600',
            children: '\u00d7',
          }),
        ],
      })
    ),
  }) : null;

  return jsx('div', {
    className: 'border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 rounded-b-2xl relative',
    children: jsxs(Fragment, { children: [
      showMention && jsx(MentionPopup, {
        ref: mentionRef,
        query: mentionQuery,
        onSelect: handleMentionSelect,
        mentionData,
      }),
      pills,
      jsxs('div', {
        className: 'p-3 flex items-end gap-2',
        children: [
          jsx('textarea', {
            ref: textareaRef,
            value: text,
            onInput: handleInput,
            onKeyDown: handleKeyDown,
            placeholder: 'Type a message... Use @ to add context',
            rows: 1,
            disabled,
            className: 'flex-1 resize-none rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 px-3 py-2 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-blue-500 placeholder-gray-400 dark:placeholder-gray-500 disabled:opacity-50',
            style: { maxHeight: '96px' },
          }),
          conversationId && jsx('button', {
            onClick: async () => {
              try {
                const res = await fetch(`${API}/conversations/${conversationId}/export-prompt`);
                if (!res.ok) throw new Error(`HTTP ${res.status}`);
                let content = await res.text();
                // Append current draft message if present
                const draft = text.trim().replace(/@(tag|person|session):\S+\s?/g, '').trim();
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
          jsx('button', {
            onClick: send,
            disabled: !text.trim() || disabled,
            className: 'flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 hover:bg-blue-700 disabled:opacity-30 disabled:cursor-not-allowed flex items-center justify-center transition-colors',
            children: disabled
              ? jsx(SpinnerIcon, { className: 'w-3.5 h-3.5 text-white' })
              : jsx(SendIcon, { className: 'w-3.5 h-3.5 text-white' }),
          }),
        ],
      }),
    ]}),
  });
}

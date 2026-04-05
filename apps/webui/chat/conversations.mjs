import { jsx, jsxs, Fragment, formatTime, formatFileSize } from '../utils.mjs';
import { PlusIcon } from '../icons.mjs';

function countWords(activeConv) {
  if (!activeConv || !activeConv.messages) return { total: 0, hasContext: false };
  let total = 0;
  let hasContext = false;
  for (const m of activeConv.messages) {
    const text = m.content || '';
    if (text) total += text.split(/\s+/).filter(Boolean).length;
    if (m.role === 'context_result' && m.chunks) {
      for (const c of m.chunks) {
        if (!c) continue;
        if (c.note) { total += c.note.split(/\s+/).filter(Boolean).length; hasContext = true; }
        if (c.segment) {
          const t = c.segment.text || '';
          if (t) { total += t.split(/\s+/).filter(Boolean).length; hasContext = true; }
        }
      }
    }
  }
  return { total, hasContext };
}

function formatWordCount(n, hasContext) {
  const suffix = hasContext ? ' incl. context' : '';
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k words${suffix}`;
  return `${n} words${suffix}`;
}

export function ConversationList({ conversations, activeId, activeConv, onSelect, onNew, onDelete, expanded, onToggleExpanded }) {
  if (!conversations.length) return null;

  const active = conversations.find(c => c.id === activeId);
  const { total: words, hasContext } = countWords(activeConv);

  return jsx('div', {
    className: 'border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50',
    children: jsxs(Fragment, { children: [
      // Header
      jsx('div', {
        className: 'flex items-center justify-between px-4 py-2',
        children: jsxs(Fragment, { children: [
          jsx('span', { className: 'text-[11px] font-medium text-gray-400 dark:text-gray-500 uppercase tracking-wider', children: 'Conversations' }),
          jsx('button', {
            onClick: onNew,
            className: 'p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors',
            title: 'New conversation',
            children: jsx(PlusIcon, { className: 'w-3.5 h-3.5 text-gray-500 dark:text-gray-400' }),
          }),
        ]}),
      }),

      // Current conversation — compact single line
      active && jsx('div', {
        className: 'px-4 pb-2',
        children: jsxs('div', {
          className: 'flex items-center gap-2 text-xs',
          children: [
            jsx('span', { className: 'font-medium text-gray-700 dark:text-gray-300 truncate flex-1 min-w-0', children: active.title || 'New conversation' }),
            jsx('span', { className: 'text-[9px] text-gray-400 flex-shrink-0', children: formatFileSize(active.size_bytes || 0) }),
            words > 0 && jsx('span', { className: 'text-[9px] text-gray-400 flex-shrink-0', children: formatWordCount(words, hasContext) }),
          ],
        }),
      }),

      // Expanded: all conversations
      expanded && jsx('div', {
        className: 'px-2 pb-2 space-y-0.5 max-h-60 overflow-y-auto border-t border-gray-200 dark:border-gray-700 pt-2',
        children: conversations.map(conv =>
          jsx('button', {
            key: conv.id,
            onClick: () => onSelect(conv.id),
            className: [
              'w-full text-left px-3 py-2 rounded-lg text-xs transition-colors flex items-center gap-2',
              conv.id === activeId
                ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800'
                : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800/60 border border-transparent',
            ].join(' '),
            children: jsxs(Fragment, { children: [
              jsx('div', {
                className: 'flex-1 min-w-0',
                children: jsxs(Fragment, { children: [
                  jsx('div', { className: 'font-medium truncate', children: conv.title || 'New conversation' }),
                  conv.last_message_preview && jsx('div', {
                    className: 'text-[10px] text-gray-400 dark:text-gray-500 truncate mt-0.5',
                    children: conv.last_message_preview,
                  }),
                ]}),
              }),
              jsxs('div', {
                className: 'flex flex-col items-end flex-shrink-0 gap-0.5',
                children: [
                  jsx('span', { className: 'text-[10px] text-gray-400 dark:text-gray-500', children: formatTime(conv.updated_at) }),
                  conv.size_bytes && jsx('span', { className: 'text-[9px] text-gray-400', children: formatFileSize(conv.size_bytes) }),
                ],
              }),
              conv.id !== activeId && jsx('button', {
                onClick: (e) => { e.stopPropagation(); onSelect(conv.id); },
                className: 'px-2 py-1 rounded text-[11px] font-medium text-blue-600 dark:text-blue-400 hover:bg-blue-100 dark:hover:bg-blue-900/30 flex-shrink-0 transition-colors',
                children: 'Resume',
              }),
              onDelete && jsx('button', {
                onClick: (e) => { e.stopPropagation(); onDelete(conv.id); },
                className: 'w-6 h-6 rounded flex items-center justify-center text-[13px] text-red-400 hover:text-red-600 hover:bg-red-100 dark:hover:bg-red-900/30 flex-shrink-0 transition-colors',
                title: 'Delete conversation',
                children: '\u00d7',
              }),
            ]}),
          })
        ),
      }),

      // Toggle
      conversations.length > 1 && jsx('button', {
        onClick: onToggleExpanded,
        className: 'w-full text-center py-1.5 text-[10px] text-blue-500 dark:text-blue-400 hover:text-blue-600 transition-colors border-t border-gray-200 dark:border-gray-700',
        children: expanded ? 'Show less' : `See all conversations (${conversations.length})`,
      }),
    ]}),
  });
}

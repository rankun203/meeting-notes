import { useState, useEffect, useImperativeHandle, forwardRef } from 'react';
import { jsx, jsxs, Fragment, formatTime } from '../utils.mjs';
import { TagIcon } from '../icons.mjs';

const FILTERS = ['all', 'tag', 'person', 'session'];
const FILTER_LABELS = { all: 'All', tag: 'Tags', person: 'People', session: 'Sessions' };

export const MentionPopup = forwardRef(function MentionPopup({ query, onSelect, mentionData }, ref) {
  const [filter, setFilter] = useState('all');
  const [highlightIdx, setHighlightIdx] = useState(0);
  const q = query.toLowerCase();

  const items = [];
  if (filter === 'all' || filter === 'tag') {
    for (const t of (mentionData.tags || [])) {
      if (t.name.toLowerCase().includes(q))
        items.push({ kind: 'tag', id: t.name, label: t.name, detail: `${t.session_count || 0} sessions` });
    }
  }
  if (filter === 'all' || filter === 'person') {
    for (const p of (mentionData.people || [])) {
      if (p.name.toLowerCase().includes(q))
        items.push({ kind: 'person', id: p.id, label: p.name });
    }
  }
  if (filter === 'all' || filter === 'session') {
    for (const s of (mentionData.sessions || [])) {
      const name = s.name || s.id;
      if (name.toLowerCase().includes(q))
        items.push({ kind: 'session', id: s.id, label: name, detail: formatTime(s.created_at) });
    }
  }

  const visible = items.slice(0, 20);

  // Reset highlight when items change
  useEffect(() => { setHighlightIdx(0); }, [q, filter]);

  // Expose keyboard handler to parent
  useImperativeHandle(ref, () => ({
    handleKey(e) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setHighlightIdx(prev => (prev + 1) % (visible.length || 1));
        return true;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setHighlightIdx(prev => (prev - 1 + (visible.length || 1)) % (visible.length || 1));
        return true;
      }
      if (e.key === 'ArrowLeft') {
        e.preventDefault();
        const i = FILTERS.indexOf(filter);
        setFilter(FILTERS[(i - 1 + FILTERS.length) % FILTERS.length]);
        return true;
      }
      if (e.key === 'ArrowRight') {
        e.preventDefault();
        const i = FILTERS.indexOf(filter);
        setFilter(FILTERS[(i + 1) % FILTERS.length]);
        return true;
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        if (visible[highlightIdx]) onSelect(visible[highlightIdx]);
        return true;
      }
      return false;
    },
  }), [visible, highlightIdx, filter]);

  const filterBtn = (id) => jsx('button', {
    onMouseDown: e => e.preventDefault(), // prevent textarea blur
    onClick: () => setFilter(id),
    className: `px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${filter === id ? 'bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'}`,
    children: FILTER_LABELS[id],
  });

  const kindIcon = (kind) => {
    if (kind === 'tag') return jsx(TagIcon, { className: 'w-3 h-3 text-blue-500' });
    if (kind === 'person') return jsx('span', { className: 'w-3 h-3 text-[10px] text-center', children: '\u{1F464}' });
    return jsx('span', { className: 'w-3 h-3 text-[10px] text-center', children: '\u{1F4DD}' });
  };

  return jsx('div', {
    onMouseDown: e => e.preventDefault(),
    className: 'absolute bottom-full left-0 right-0 bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 border-b-0 rounded-t-xl shadow-[0_-4px_12px_rgba(0,0,0,0.08)] overflow-hidden z-10',
    children: jsxs(Fragment, { children: [
      jsxs('div', {
        className: 'flex items-center gap-1 px-3 py-1.5 border-b border-gray-200 dark:border-gray-700',
        children: [
          jsx('span', { className: 'text-[10px] text-gray-400 mr-1', children: '@' }),
          filterBtn('all'),
          filterBtn('tag'),
          filterBtn('person'),
          filterBtn('session'),
        ],
      }),
      jsx('div', {
        className: 'max-h-40 overflow-y-auto',
        children: visible.length === 0
          ? jsx('div', { className: 'px-3 py-2 text-xs text-gray-400', children: 'No matches' })
          : visible.map((item, i) =>
              jsx('button', {
                key: `${item.kind}-${item.id}`,
                onMouseDown: e => e.preventDefault(), // prevent textarea blur
                onClick: () => onSelect(item),
                onMouseEnter: () => setHighlightIdx(i),
                className: `w-full text-left px-3 py-1.5 text-xs flex items-center gap-2 transition-colors ${i === highlightIdx ? 'bg-blue-50 dark:bg-blue-900/20' : 'hover:bg-gray-100 dark:hover:bg-gray-800'}`,
                children: jsxs(Fragment, { children: [
                  kindIcon(item.kind),
                  jsx('span', { className: 'font-medium truncate flex-1', children: item.label }),
                  item.detail && jsx('span', { className: 'text-[10px] text-gray-400 flex-shrink-0', children: item.detail }),
                ]}),
              })
            ),
      }),
    ]}),
  });
});

import { useState } from 'react';
import { jsx, jsxs, api, formatTime } from './utils.mjs';

// ── People sidebar list ──

export function PeopleSidebar({ selectedId, onSelect, people, onRefresh }) {
  const [newName, setNewName] = useState('');

  async function createPerson() {
    if (!newName.trim()) return;
    try {
      const result = await api('/people', { method: 'POST', body: JSON.stringify({ name: newName.trim() }) });
      setNewName('');
      onRefresh();
      if (result?.id) onSelect(result.id);
    } catch (e) { alert(e.message); }
  }

  return jsxs('div', { className: 'px-2 py-2 space-y-1', children: [
    jsxs('div', { className: 'flex gap-1 px-1 mb-1', children: [
      jsx('input', {
        type: 'text', value: newName, placeholder: 'New person...',
        onChange: e => setNewName(e.target.value),
        onKeyDown: e => { if (e.key === 'Enter') createPerson(); },
        className: 'flex-1 text-xs rounded border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 px-2 py-1 focus:outline-none focus:ring-1 focus:ring-blue-500',
      }),
      jsx('button', {
        onClick: createPerson, disabled: !newName.trim(),
        className: 'px-2 py-1 rounded text-[11px] font-medium text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50',
        children: 'Add',
      }),
    ]}),
    ...people.map(p => jsx('button', {
      key: p.id,
      onClick: () => onSelect(p.id),
      className: [
        'w-full text-left px-3 py-2 rounded-lg text-xs transition-colors',
        selectedId === p.id
          ? 'bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800'
          : 'hover:bg-gray-100 dark:hover:bg-gray-800/60 border border-transparent',
      ].join(' '),
      children: jsxs('div', { children: [
        jsx('p', { className: 'font-medium text-gray-700 dark:text-gray-300', children: p.name }),
        jsxs('p', { className: 'text-[10px] text-gray-400 mt-0.5', children: [
          `${p.embedding_count || 0} voice samples`,
          p.last_seen ? ` · ${formatTime(p.last_seen)}` : '',
        ]}),
      ]}),
    })),
    people.length === 0 && jsx('p', { className: 'text-[11px] text-gray-400 text-center py-4', children: 'No people yet' }),
  ]});
}

// ── Person detail panel ──

export function PersonDetail({ person, onRefresh }) {
  if (!person) {
    return jsx('div', {
      className: 'h-full flex items-center justify-center',
      children: jsx('p', { className: 'text-sm text-gray-400', children: 'Select a person to see details' }),
    });
  }

  async function deletePerson() {
    if (!confirm(`Delete "${person.name}" and all their voice data?`)) return;
    try {
      await api(`/people/${person.id}`, { method: 'DELETE' });
      onRefresh();
    } catch (e) { alert(e.message); }
  }

  return jsx('div', {
    className: 'h-full overflow-y-auto px-6 py-6',
    children: jsxs('div', { className: 'max-w-xl space-y-4', children: [
      jsxs('div', { className: 'flex items-center justify-between', children: [
        jsx('h2', { className: 'text-lg font-semibold', children: person.name }),
        jsx('button', {
          onClick: deletePerson,
          className: 'text-xs text-red-500 hover:text-red-700 px-2 py-1 rounded hover:bg-red-50 dark:hover:bg-red-900/20',
          children: 'Delete',
        }),
      ]}),
      jsx('div', {
        className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4',
        children: jsxs('div', { className: 'grid grid-cols-2 gap-3', children: [
          jsxs('div', { children: [
            jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Voice Samples' }),
            jsx('p', { className: 'text-sm font-medium', children: person.embedding_count || 0 }),
          ]}),
          jsxs('div', { children: [
            jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'Last Seen' }),
            jsx('p', { className: 'text-sm font-medium', children: person.last_seen ? formatTime(person.last_seen) : 'Never' }),
          ]}),
          jsxs('div', { children: [
            jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-0.5', children: 'ID' }),
            jsx('p', { className: 'text-xs font-mono text-gray-500', children: person.id }),
          ]}),
        ]}),
      }),
    ]}),
  });
}

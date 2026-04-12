import { useState, useEffect, useRef } from 'react';
import { jsx, jsxs, api, INPUT_CLS, autoResize, autoResizeDeferred, stripMd, formatTime, formatDuration } from './utils.mjs';

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
      children: jsxs('div', { className: 'flex items-start gap-1', children: [
        jsx('span', {
          onClick: e => {
            e.stopPropagation();
            api(`/people/${p.id}`, { method: 'PATCH', body: JSON.stringify({ starred: !p.starred }) }).then(onRefresh);
          },
          className: 'cursor-pointer text-sm leading-none mt-0.5 flex-shrink-0 ' + (p.starred ? 'text-yellow-400' : 'text-gray-300 dark:text-gray-600 hover:text-yellow-400'),
          children: p.starred ? '★' : '☆',
        }),
        jsxs('div', { className: 'min-w-0', children: [
          jsx('p', { className: 'font-medium text-gray-700 dark:text-gray-300', children: p.name }),
          jsxs('p', { className: 'text-[10px] text-gray-400 mt-0.5', children: [
            `${p.embedding_count || 0} voice samples`,
            p.last_seen ? ` · ${formatTime(p.last_seen)}` : '',
          ]}),
        ]}),
      ]}),
    })),
    people.length === 0 && jsx('p', { className: 'text-[11px] text-gray-400 text-center py-4', children: 'No people yet' }),
  ]});
}

// ── Person detail panel ──

export function PersonDetail({ person, onRefresh, onSelectSession }) {
  const [sessions, setSessions] = useState([]);
  const [loadingSessions, setLoadingSessions] = useState(false);
  const [notes, setNotes] = useState('');
  const [notesSaving, setNotesSaving] = useState(false);
  const notesTimer = useRef(null);
  const [todos, setTodos] = useState([]);
  const [loadingTodos, setLoadingTodos] = useState(false);

  // Fetch full person detail (list endpoint doesn't include notes)
  useEffect(() => {
    if (!person) return;
    api(`/people/${person.id}`)
      .then(d => setNotes(d.notes || ''))
      .catch(() => {});
  }, [person?.id]);

  function handleNotesChange(e) {
    const val = e.target.value;
    setNotes(val);
    clearTimeout(notesTimer.current);
    notesTimer.current = setTimeout(async () => {
      setNotesSaving(true);
      try {
        await api(`/people/${person.id}`, {
          method: 'PATCH',
          body: JSON.stringify({ notes: val || null }),
        });
      } catch {}
      setNotesSaving(false);
    }, 800);
  }

  useEffect(() => {
    if (!person) { setSessions([]); setTodos([]); return; }
    setLoadingSessions(true);
    setLoadingTodos(true);
    api(`/people/${person.id}/sessions`)
      .then(d => setSessions(d.sessions || []))
      .catch(() => setSessions([]))
      .finally(() => setLoadingSessions(false));
    api(`/people/${person.id}/todos`)
      .then(d => setTodos(d.todos || []))
      .catch(() => setTodos([]))
      .finally(() => setLoadingTodos(false));
  }, [person?.id]);

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
          jsxs('div', { className: 'col-span-2', children: [
            jsxs('div', { className: 'flex items-center gap-2 mb-0.5', children: [
              jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500', children: 'Notes' }),
              notesSaving && jsx('span', { className: 'text-[10px] text-blue-500', children: 'Saving...' }),
            ]}),
            jsx('textarea', {
              value: notes,
              onChange: handleNotesChange,
              onInput: autoResize,
              ref: el => autoResizeDeferred(el),
              placeholder: 'Add notes about this person...',
              rows: 1,
              className: INPUT_CLS + ' text-xs overflow-hidden',
            }),
          ]}),
        ]}),
      }),

      // Recent sessions
      jsx('div', {
        className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4',
        children: jsxs('div', { className: 'space-y-2', children: [
          jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500', children: 'Recent Sessions' }),
          loadingSessions
            ? jsx('p', { className: 'text-xs text-gray-400 py-2', children: 'Loading...' })
            : sessions.length === 0
              ? jsx('p', { className: 'text-xs text-gray-400 py-2', children: 'No sessions found' })
              : jsx('div', { className: 'space-y-1', children:
                  sessions.map(s => jsxs('div', {
                    key: s.id,
                    className: 'px-3 py-2 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors',
                    children: [
                      jsx('button', {
                        onClick: () => onSelectSession && onSelectSession(s.id),
                        className: 'w-full text-left',
                        children: jsxs('div', { className: 'flex items-center justify-between gap-2', children: [
                          jsxs('div', { className: 'min-w-0 flex-1', children: [
                            jsx('p', {
                              className: 'text-xs font-medium text-gray-700 dark:text-gray-300 truncate',
                              children: s.name || s.id,
                            }),
                            jsx('p', {
                              className: 'text-[10px] text-gray-400 mt-0.5',
                              children: formatTime(s.created_at),
                            }),
                          ]}),
                          s.duration_secs != null && jsx('span', {
                            className: 'text-[10px] text-gray-400 flex-shrink-0',
                            children: formatDuration(s.duration_secs),
                          }),
                        ]}),
                      }),
                      (s.matched_speakers && s.matched_speakers.length > 0) && jsx('div', {
                        className: 'flex flex-wrap gap-1 mt-1.5',
                        children: s.matched_speakers.map(ms => jsxs('span', {
                          key: ms.speaker,
                          className: 'inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] bg-violet-50 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
                          children: [
                            ms.speaker,
                            jsx('span', { className: 'text-violet-400 dark:text-violet-500', children: `${Math.round(ms.confidence * 100)}%` }),
                            jsx('button', {
                              onClick: async (e) => {
                                e.stopPropagation();
                                try {
                                  await api(`/sessions/${s.id}/attribution`, {
                                    method: 'POST',
                                    body: JSON.stringify({ attributions: [{ speaker: ms.speaker, action: 'reject' }] }),
                                  });
                                  const d = await api(`/people/${person.id}/sessions`);
                                  setSessions(d.sessions || []);
                                  onRefresh();
                                } catch (err) { alert(err.message); }
                              },
                              title: `Disconnect ${ms.speaker} from ${person.name}`,
                              className: 'ml-0.5 text-red-400 hover:text-red-600 dark:text-red-500 dark:hover:text-red-400 transition-colors',
                              children: '\u00d7',
                            }),
                          ],
                        })),
                      }),
                      // Action items for this session
                      (() => {
                        const sessionTodos = todos.filter(t => t.session_id === s.id);
                        if (sessionTodos.length === 0) return null;
                        return jsx('div', { className: 'mt-2 space-y-0.5', children:
                          sessionTodos.map((todo, i) => jsx('div', {
                            key: todo.todo_index,
                            className: 'flex items-start gap-2 py-1 cursor-pointer rounded hover:bg-gray-100 dark:hover:bg-gray-700/30 px-1 transition-colors',
                            onClick: async (e) => {
                              e.stopPropagation();
                              try {
                                await api(`/sessions/${s.id}/todos/${todo.todo_index}`, { method: 'PATCH', body: '{}' });
                                setTodos(prev => prev.map(t =>
                                  t.session_id === s.id && t.todo_index === todo.todo_index
                                    ? { ...t, completed: !t.completed } : t
                                ));
                              } catch {}
                            },
                            children: [
                              jsx('input', {
                                type: 'checkbox',
                                checked: todo.completed,
                                readOnly: true,
                                className: 'w-3.5 h-3.5 rounded border-gray-300 text-blue-600 flex-shrink-0 mt-0.5',
                              }),
                              jsx('span', {
                                className: `text-[11px] ${todo.completed ? 'line-through text-gray-400' : 'text-gray-700 dark:text-gray-300'}`,
                                children: stripMd(todo.full_text || todo.text),
                              }),
                            ],
                          })),
                        });
                      })(),
                    ],
                  })),
                }),
        ]}),
      }),
    ]}),
  });
}

import { useState, useEffect } from 'react';
import { jsx, jsxs, Fragment, api, formatTime, formatFileSize } from '../utils.mjs';

export function ConversationsSettings() {
  const [conversations, setConversations] = useState([]);
  const [loading, setLoading] = useState(true);
  const [confirmClearAll, setConfirmClearAll] = useState(false);

  async function refresh() {
    try {
      const data = await api('/conversations');
      setConversations(data.conversations || []);
    } catch {}
    setLoading(false);
  }

  useEffect(() => { refresh(); }, []);

  async function deleteConversation(id) {
    try {
      await api(`/conversations/${id}`, { method: 'DELETE' });
      setConversations(prev => prev.filter(c => c.id !== id));
    } catch {}
  }

  async function clearAll() {
    for (const conv of conversations) {
      try { await api(`/conversations/${conv.id}`, { method: 'DELETE' }); } catch {}
    }
    setConversations([]);
    setConfirmClearAll(false);
  }

  if (loading) return jsx('p', { className: 'text-sm text-gray-400', children: 'Loading...' });

  return jsx('div', {
    className: 'space-y-4',
    children: jsxs(Fragment, { children: [
      jsxs('div', {
        className: 'flex items-center justify-between',
        children: [
          jsx('h3', { className: 'text-sm font-medium text-gray-900 dark:text-gray-100', children: 'Chat Conversations' }),
          conversations.length > 0 && (
            confirmClearAll
              ? jsxs('div', {
                  className: 'flex items-center gap-2',
                  children: [
                    jsx('span', { className: 'text-xs text-red-500', children: 'Delete all?' }),
                    jsx('button', {
                      onClick: clearAll,
                      className: 'px-2 py-1 rounded text-xs font-medium text-white bg-red-500 hover:bg-red-600 transition-colors',
                      children: 'Yes',
                    }),
                    jsx('button', {
                      onClick: () => setConfirmClearAll(false),
                      className: 'px-2 py-1 rounded text-xs font-medium text-gray-500 hover:text-gray-700 transition-colors',
                      children: 'Cancel',
                    }),
                  ],
                })
              : jsx('button', {
                  onClick: () => setConfirmClearAll(true),
                  className: 'text-[11px] text-red-400 hover:text-red-500 transition-colors',
                  children: 'Clear all',
                })
          ),
        ],
      }),
      conversations.length === 0
        ? jsx('p', { className: 'text-sm text-gray-400 dark:text-gray-500', children: 'No conversations yet.' })
        : jsx('div', {
            className: 'space-y-2',
            children: conversations.map(conv =>
              jsx('div', {
                key: conv.id,
                className: 'flex items-center gap-3 px-3 py-2.5 rounded-lg border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900',
                children: jsxs(Fragment, { children: [
                  jsx('div', {
                    className: 'flex-1 min-w-0',
                    children: jsxs(Fragment, { children: [
                      jsx('div', { className: 'text-sm font-medium text-gray-900 dark:text-gray-100 truncate', children: conv.title || 'New conversation' }),
                      jsxs('div', {
                        className: 'text-[11px] text-gray-400 dark:text-gray-500 mt-0.5',
                        children: `${conv.message_count} message${conv.message_count !== 1 ? 's' : ''} \u00b7 ${formatFileSize(conv.size_bytes || 0)} \u00b7 ${formatTime(conv.updated_at)}`,
                      }),
                    ]}),
                  }),
                  jsx('button', {
                    onClick: () => deleteConversation(conv.id),
                    className: 'px-2 py-1 rounded text-[11px] font-medium text-red-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors',
                    children: 'Delete',
                  }),
                ]}),
              })
            ),
          }),
    ]}),
  });
}

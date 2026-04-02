import { useState, useEffect, useRef, useMemo } from 'react';
import { jsx, jsxs, Fragment, formatTime } from '../utils.mjs';
import { SparkleIcon, ContextIcon } from '../icons.mjs';

// Lazy-load marked only when first needed
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

// Three bouncing dots component
function ThinkingDots() {
  return jsxs('span', {
    className: 'dot-loading inline-flex items-center gap-1 text-blue-400 dark:text-blue-500',
    children: [
      jsx('span', {}),
      jsx('span', {}),
      jsx('span', {}),
    ],
  });
}

export function MessageThread({ messages, streamingContent, streamingPhase, onDeleteMessage }) {
  const scrollRef = useRef(null);
  const [, forceRender] = useState(0);

  // Lazy-load marked when component mounts
  useEffect(() => {
    if (!markedModule) ensureMarked(() => forceRender(n => n + 1));
  }, []);

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages.length, streamingContent]);

  // Build display messages, appending streaming state
  const allMessages = useMemo(() => {
    const msgs = [...messages];
    if (streamingPhase === 'thinking') {
      msgs.push({ role: 'assistant', id: '_thinking', _thinking: true, timestamp: new Date().toISOString() });
    } else if (streamingContent !== null && streamingPhase === 'streaming') {
      msgs.push({ role: 'assistant', id: '_streaming', content: streamingContent, _streaming: true, timestamp: new Date().toISOString() });
    }
    return msgs;
  }, [messages, streamingContent, streamingPhase]);

  if (!allMessages.length) {
    return jsx('div', {
      ref: scrollRef,
      className: 'flex-1 flex flex-col items-center justify-center text-gray-400 dark:text-gray-500 px-6',
      children: jsxs(Fragment, { children: [
        jsx(SparkleIcon, { className: 'w-10 h-10 mb-3 opacity-40' }),
        jsx('p', { className: 'text-sm font-medium', children: 'No messages yet' }),
        jsx('p', { className: 'text-xs mt-1 text-center', children: 'Ask a question about your meeting notes' }),
        jsx('p', { className: 'text-[10px] mt-2 text-center opacity-70', children: 'Use @ to include tags, people, or sessions as context' }),
      ]}),
    });
  }

  return jsx('div', {
    ref: scrollRef,
    className: 'flex-1 overflow-y-auto px-4 py-3 space-y-3',
    children: allMessages.map((msg) => {
      if (msg.role === 'context_result') {
        const chunks = msg.chunks || [];
        const sessionCount = new Set(chunks.filter(c => c).map(c => c.source_id)).size;
        return jsx('div', {
          key: msg.id,
          className: 'flex justify-center',
          children: jsxs('div', {
            className: 'inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-gray-100 dark:bg-gray-800 text-[10px] text-gray-500 dark:text-gray-400',
            children: [
              jsx(ContextIcon, { className: 'w-3 h-3' }),
              `Context loaded: ${chunks.length} segments from ${sessionCount} session${sessionCount !== 1 ? 's' : ''}`,
            ],
          }),
        });
      }

      const isUser = msg.role === 'user';
      const isThinking = msg._thinking;
      const isStreaming = msg._streaming;

      return jsx('div', {
        key: msg.id,
        className: `flex ${isUser ? 'justify-end' : 'justify-start'} gap-2`,
        children: jsxs(Fragment, { children: [
          !isUser && jsx('div', {
            className: 'flex-shrink-0 w-6 h-6 rounded-full bg-blue-100 dark:bg-blue-900/40 flex items-center justify-center mt-0.5',
            children: jsx(SparkleIcon, { className: 'w-3.5 h-3.5 text-blue-600 dark:text-blue-400' }),
          }),
          jsxs('div', {
            className: `flex flex-col ${isUser ? 'items-end' : 'items-start'} max-w-[80%]`,
            children: [
              // Mention pills for user messages
              isUser && msg.mentions && msg.mentions.length > 0 && jsx('div', {
                className: 'flex flex-wrap gap-1 mb-1',
                children: msg.mentions.map((m, j) =>
                  jsx('span', {
                    key: j,
                    className: 'inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-full bg-blue-100 dark:bg-blue-500/20 text-[9px] text-blue-600 dark:text-blue-300 font-medium',
                    children: `${m.kind === 'tag' ? '#' : m.kind === 'person' ? '\u{1F464}' : '\u{1F4DD}'} ${m.label}`,
                  })
                ),
              }),
              // Message bubble
              isThinking
                ? jsx('div', {
                    className: 'px-4 py-3 text-sm rounded-2xl rounded-bl-md bg-gray-100 dark:bg-gray-800 min-w-[60px]',
                    children: jsx(ThinkingDots, {}),
                  })
                : isUser
                  ? jsx('div', {
                      className: 'px-3 py-2 text-sm rounded-2xl rounded-br-md bg-blue-600 text-white',
                      children: msg.content,
                    })
                  : jsx('div', {
                      className: 'px-3 py-2 text-sm rounded-2xl rounded-bl-md bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100 min-w-[120px] md-content',
                      dangerouslySetInnerHTML: { __html: renderMarkdown(msg.content || '') },
                    }),
              // Status line
              jsxs('span', {
                className: 'text-[10px] text-gray-400 dark:text-gray-500 mt-0.5 px-1 flex items-center gap-1.5',
                children: [
                  isThinking ? 'Thinking...' : isStreaming ? 'Streaming...' : formatTime(msg.timestamp),
                  !isThinking && !isStreaming && onDeleteMessage && jsx('button', {
                    onClick: () => onDeleteMessage(msg.id),
                    className: 'text-gray-300 dark:text-gray-600 hover:text-red-400 dark:hover:text-red-400 transition-colors',
                    title: 'Delete message',
                    children: '\u00d7',
                  }),
                ],
              }),
            ],
          }),
        ]}),
      });
    }),
  });
}

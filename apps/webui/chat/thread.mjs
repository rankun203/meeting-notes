import { useState, useEffect, useRef, useMemo } from 'react';
import { jsx, jsxs, Fragment, formatTime } from '../utils.mjs';
import { SparkleIcon, ContextIcon, CloseIcon } from '../icons.mjs';

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
    const renderer = new mod.Renderer();
    const origLink = renderer.link.bind(renderer);
    renderer.link = function({ href, title, text }) {
      return `<a href="${href}"${title ? ` title="${title}"` : ''} target="_blank" rel="noopener noreferrer">${text}</a>`;
    };
    markedModule.setOptions({ breaks: true, gfm: true, renderer });
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

// Horizontal scrolling thinking bar — single line, no visible scrollbar, click to expand
function ThinkingBar({ content, onClick }) {
  const ref = useRef(null);
  useEffect(() => {
    if (ref.current) ref.current.scrollLeft = ref.current.scrollWidth;
  }, [content]);
  if (!content) return null;
  const flat = content.replace(/\n/g, ' ');
  return jsx('div', {
    ref,
    onClick,
    className: 'w-full overflow-x-auto no-scrollbar whitespace-nowrap text-[11px] text-gray-400 dark:text-gray-500 italic px-3 py-1.5 mb-1 rounded-lg cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors select-none',
    style: { scrollbarWidth: 'none' },
    children: flat,
  });
}

// Modal to show full thinking content
function ThinkingModal({ content, onClose }) {
  if (!content) return null;
  return jsx('div', {
    className: 'absolute inset-0 z-[20000] flex items-center justify-center bg-black/40 p-4',
    onClick: (e) => { if (e.target === e.currentTarget) onClose(); },
    children: jsx('div', {
      className: 'bg-white dark:bg-gray-900 rounded-xl shadow-2xl border border-gray-200 dark:border-gray-700 w-full max-h-[80%] flex flex-col overflow-hidden',
      children: jsxs(Fragment, { children: [
        jsxs('div', {
          className: 'flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700 flex-shrink-0',
          children: [
            jsx('span', { className: 'text-sm font-medium text-gray-700 dark:text-gray-300', children: 'Thinking' }),
            jsx('button', {
              onClick: onClose,
              className: 'p-1 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors',
              children: jsx(CloseIcon, { className: 'w-4 h-4 text-gray-500' }),
            }),
          ],
        }),
        jsx('div', {
          className: 'flex-1 overflow-y-auto px-4 py-3 text-sm text-gray-600 dark:text-gray-400 whitespace-pre-wrap',
          children: content,
        }),
      ]}),
    }),
  });
}

export function MessageThread({ messages, streamingContent, streamingThinking, streamingPhase, onDeleteMessage, toolActivities }) {
  const scrollRef = useRef(null);
  const [, forceRender] = useState(0);
  const [thinkingModal, setThinkingModal] = useState(null); // null = closed, string = content

  // Lazy-load marked when component mounts
  useEffect(() => {
    if (!markedModule) ensureMarked(() => forceRender(n => n + 1));
  }, []);

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages.length, streamingContent, streamingThinking, toolActivities]);

  // Keep modal content in sync while open and thinking is still streaming
  useEffect(() => {
    if (thinkingModal !== null && streamingThinking) {
      setThinkingModal(streamingThinking);
    }
  }, [streamingThinking]);

  // Build display messages, appending streaming state
  const allMessages = useMemo(() => {
    const msgs = [...messages];
    if (streamingPhase === 'thinking') {
      msgs.push({ role: 'assistant', id: '_thinking', _thinking: true, _thinkingContent: streamingThinking || '', timestamp: new Date().toISOString() });
    } else if (streamingContent !== null && streamingPhase === 'streaming') {
      // Include thinking bar above the streaming content if there was thinking
      if (streamingThinking) {
        msgs.push({ role: 'assistant', id: '_streaming', content: streamingContent, _streaming: true, _thinkingContent: streamingThinking, timestamp: new Date().toISOString() });
      } else {
        msgs.push({ role: 'assistant', id: '_streaming', content: streamingContent, _streaming: true, _toolActivities: toolActivities || [], timestamp: new Date().toISOString() });
      }
    }
    return msgs;
  }, [messages, streamingContent, streamingThinking, streamingPhase, toolActivities]);

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

  return jsxs(Fragment, { children: [
    jsx('div', {
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
                    children: `${m.kind === 'tag' ? '#' : m.kind === 'person' ? '\u{1F464}' : '\u{1F4DD}'} ${m.label}${m.kind === 'session' ? ` (${m.id})` : ''}`,
                  })
                ),
              }),
              // Thinking bar (shown during thinking phase or above streamed/final content)
              !isUser && msg._thinkingContent && jsx(ThinkingBar, {
                content: msg._thinkingContent,
                onClick: () => setThinkingModal(msg._thinkingContent),
              }),
              // Tool activity indicators (Claude Code) — above message bubble
              msg._toolActivities && msg._toolActivities.length > 0 && jsx('div', {
                className: 'flex flex-col gap-0.5 mb-1',
                children: msg._toolActivities.map((ta, idx) => {
                  const fullText = `${ta.tool}${ta.summary ? ': ' + ta.summary : ''}`;
                  const isLast = isStreaming && idx === msg._toolActivities.length - 1;
                  return jsx('div', {
                    key: idx,
                    className: 'flex items-start gap-1.5 px-2 py-0.5 rounded text-[10px] font-medium text-gray-500 dark:text-gray-400 cursor-default',
                    title: fullText,
                    children: [
                      jsx('span', {
                        className: `inline-block w-1.5 h-1.5 mt-[3px] flex-shrink-0 rounded-full ${isLast ? 'bg-amber-400 animate-pulse' : 'bg-gray-300 dark:bg-gray-600'}`,
                      }),
                      jsx('span', {
                        className: 'line-clamp-2 break-all',
                        children: fullText,
                      }),
                    ],
                  });
                }),
              }),
              // Message bubble
              isThinking
                ? !msg._thinkingContent && jsx('div', {
                    className: 'px-4 py-3 text-sm rounded-2xl rounded-bl-md bg-gray-100 dark:bg-gray-800 min-w-[60px]',
                    children: jsx(ThinkingDots, {}),
                  })
                : isUser
                  ? jsx('div', {
                      className: 'px-3 py-2 text-sm rounded-2xl rounded-br-md bg-blue-600 text-white',
                      style: { overflowWrap: 'anywhere' },
                      children: msg.content,
                    })
                  : (msg.content || !isStreaming) && jsx('div', {
                      className: 'px-3 py-2 text-sm rounded-2xl rounded-bl-md bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100 min-w-0 md-content',
                      dangerouslySetInnerHTML: { __html: renderMarkdown(msg.content || '') },
                    }),
              // Status line
              jsxs('span', {
                className: 'text-[10px] text-gray-400 dark:text-gray-500 mt-0.5 px-1 flex items-center gap-1.5',
                children: [
                  isThinking ? 'Thinking...' : isStreaming ? 'Streaming...' : formatTime(msg.timestamp),
                  !isThinking && !isStreaming && onDeleteMessage && jsx('button', {
                    onClick: () => { if (confirm('Delete this message?')) onDeleteMessage(msg.id); },
                    className: 'text-gray-400 dark:text-gray-500 hover:text-red-500 dark:hover:text-red-400 transition-colors',
                    children: 'Delete',
                  }),
                ],
              }),
            ],
          }),
        ]}),
      });
    }),
  }),
    thinkingModal !== null && jsx(ThinkingModal, {
      content: thinkingModal,
      onClose: () => setThinkingModal(null),
    }),
  ]});
}

import { useState, useEffect, useCallback } from 'react';
import { jsx, jsxs, Fragment } from '../utils.mjs';
import { SparkleIcon, MinimizeIcon, MaximizeIcon, RestoreIcon, CloseIcon } from '../icons.mjs';
import { panelPosition, BUBBLE_SIZE, BUBBLE_SIZE_MOBILE } from './constants.mjs';
import { ConversationList } from './conversations.mjs';
import { MessageThread } from './thread.mjs';
import { InputComposer } from './composer.mjs';

const PERMISSION_OPTIONS = [
  { scope: 'once', label: 'Allow once', className: 'bg-amber-200 dark:bg-amber-800 text-amber-800 dark:text-amber-200' },
  { scope: 'session', label: 'Allow in session', className: 'bg-amber-400 dark:bg-amber-700 text-amber-900 dark:text-amber-100' },
  { scope: 'permanent', label: 'Always allow', className: 'bg-amber-600 dark:bg-amber-800 text-white' },
  { scope: 'deny', label: 'Deny', className: 'bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-300' },
];

function PermissionBanner({ pendingPermissions, onApproveTools, onDeny }) {
  const [selectedIdx, setSelectedIdx] = useState(0);

  const allTools = pendingPermissions.flatMap(p => p.tools);
  const toolLabel = [...new Set(allTools)].join(', ');
  const preview = pendingPermissions.find(p => p.preview)?.preview;

  const handleAction = useCallback((idx) => {
    const opt = PERMISSION_OPTIONS[idx];
    if (opt.scope === 'deny') {
      if (onDeny) onDeny();
    } else if (onApproveTools) {
      onApproveTools(allTools, opt.scope);
    }
  }, [allTools, onApproveTools, onDeny]);

  const handleKeyDown = useCallback((e) => {
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIdx(prev => (prev - 1 + PERMISSION_OPTIONS.length) % PERMISSION_OPTIONS.length);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIdx(prev => (prev + 1) % PERMISSION_OPTIONS.length);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      handleAction(selectedIdx);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (onDeny) onDeny();
    }
  }, [selectedIdx, handleAction, onDeny]);

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  return jsx('div', {
    className: 'flex-shrink-0 border-t border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-900/20 px-4 py-3',
    children: jsxs('div', {
      className: 'space-y-2',
      children: [
        jsx('div', { className: 'text-[11px] font-medium text-amber-800 dark:text-amber-200 break-all', children: toolLabel }),
        preview && jsx('div', {
          className: 'text-[10px] text-amber-700 dark:text-amber-300 bg-amber-100 dark:bg-amber-900/40 rounded px-2 py-1.5 break-all font-mono max-h-24 overflow-y-auto',
          children: preview,
        }),
        jsx('div', {
          className: 'flex flex-col gap-1',
          children: PERMISSION_OPTIONS.map((opt, i) =>
            jsx('button', {
              key: opt.scope,
              onClick: () => handleAction(i),
              className: [
                'w-full px-3 py-1.5 rounded text-[11px] font-medium text-left transition-all',
                opt.className,
                i === selectedIdx ? 'ring-2 ring-amber-500 ring-offset-1 dark:ring-offset-gray-900' : 'opacity-70',
              ].join(' '),
              children: `${i === selectedIdx ? '\u25b8 ' : '  '}${opt.label}`,
            })
          ),
        }),
        jsx('div', { className: 'text-[9px] text-amber-600 dark:text-amber-400', children: '\u2191\u2193 select \u00b7 Enter confirm \u00b7 Esc deny' }),
      ],
    }),
  });
}

export function ChatPanel({ conversations, activeConv, activeId, onSelectConversation, onNewConversation, onDeleteConversation, onSend, onStop, onDeleteMessage, onClose, onMinimize, bubblePos, isMobile, closing, streaming, streamingContent, streamingThinking, streamingPhase, tokenUsage, mentionData, llmConfigured, chatBackend, toolActivities, onSendToClaudeCode, onApproveTools, onDenyPermissions, pendingPermissions }) {
  const [listExpanded, setListExpanded] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const bSize = isMobile ? BUBBLE_SIZE_MOBILE : BUBBLE_SIZE;
  const pos = panelPosition(bubblePos.x, bubblePos.y, bSize, isMobile);

  const panelStyle = isMobile || maximized
    ? { position: 'fixed', top: maximized ? 0 : pos.top, left: 0, right: 0, bottom: 0, zIndex: 10000 }
    : { position: 'fixed', top: pos.top, left: pos.left, width: pos.width, height: pos.height, zIndex: 10000 };

  const bCenterX = bubblePos.x + bSize / 2, bCenterY = bubblePos.y + bSize / 2;
  const originX = isMobile ? '50%' : (bCenterX > window.innerWidth / 2 ? '100%' : '0%');
  const originY = isMobile ? '100%' : (bCenterY > window.innerHeight / 2 ? '100%' : '0%');

  return jsx('div', {
    style: { ...panelStyle, transformOrigin: `${originX} ${originY}` },
    className: `flex flex-col bg-white dark:bg-gray-900 ${maximized ? '' : isMobile ? 'rounded-t-2xl' : 'rounded-2xl'} shadow-2xl border border-gray-200 dark:border-gray-800 overflow-hidden ${closing ? 'chat-panel-exit' : 'chat-panel-enter'}`,
    children: jsxs(Fragment, { children: [
      // Header
      jsx('div', {
        className: 'flex items-center justify-between px-4 py-3 bg-blue-600 dark:bg-blue-700 text-white flex-shrink-0',
        children: jsxs(Fragment, { children: [
          jsxs('div', {
            className: 'flex items-center gap-2',
            children: [
              jsx('div', {
                className: 'w-7 h-7 rounded-full bg-white/20 flex items-center justify-center',
                children: jsx(SparkleIcon, { className: 'w-4 h-4 text-white' }),
              }),
              jsxs('div', { children: [
                jsx('div', { className: 'text-sm font-semibold leading-tight', children: chatBackend === 'claude_code' ? 'Claude Code' : 'Meeting Notes' }),
                jsxs('div', {
                  className: 'flex items-center gap-1 text-[10px] text-blue-100',
                  children: [
                    jsx('span', { className: `w-1.5 h-1.5 rounded-full inline-block ${streamingPhase ? 'bg-blue-300' : llmConfigured ? 'bg-green-400' : 'bg-yellow-400'}` }),
                    streamingPhase === 'thinking' ? 'Thinking...' : streamingPhase === 'streaming' ? 'Streaming...' : llmConfigured ? 'Online' : 'Not configured',
                  ],
                }),
              ]}),
            ],
          }),
          jsxs('div', {
            className: 'flex items-center gap-1',
            children: [
              !isMobile && jsx('button', {
                onClick: () => setMaximized(v => !v),
                className: 'p-1.5 rounded-lg hover:bg-white/20 transition-colors',
                title: maximized ? 'Restore' : 'Maximize',
                children: maximized
                  ? jsx(RestoreIcon, { className: 'w-4 h-4 text-white' })
                  : jsx(MaximizeIcon, { className: 'w-4 h-4 text-white' }),
              }),
              jsx('button', {
                onClick: onMinimize,
                className: 'p-1.5 rounded-lg hover:bg-white/20 transition-colors',
                title: 'Minimize',
                children: jsx(MinimizeIcon, { className: 'w-4 h-4 text-white' }),
              }),
              jsx('button', {
                onClick: onClose,
                className: 'p-1.5 rounded-lg hover:bg-white/20 transition-colors',
                title: 'Close',
                children: jsx(CloseIcon, { className: 'w-4 h-4 text-white' }),
              }),
            ],
          }),
        ]}),
      }),
      jsx(ConversationList, {
        conversations, activeId, activeConv, tokenUsage,
        onSelect: (id) => { onSelectConversation(id); setListExpanded(false); },
        onNew: onNewConversation,
        onDelete: onDeleteConversation,
        expanded: listExpanded,
        onToggleExpanded: () => setListExpanded(!listExpanded),
      }),
      jsx(MessageThread, { messages: activeConv ? activeConv.messages : [], streamingContent, streamingThinking, streamingPhase, onDeleteMessage, toolActivities }),
      pendingPermissions && pendingPermissions.length > 0 && jsx(PermissionBanner, {
        pendingPermissions,
        onApproveTools,
        onDeny: onDenyPermissions,
      }),
      jsx(InputComposer, { onSend, onStop, streaming, mentionData, conversationId: activeId, onSendToClaudeCode }),
    ]}),
  });
}

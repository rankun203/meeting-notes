import { useState } from 'react';
import { jsx, jsxs, Fragment } from '../utils.mjs';
import { SparkleIcon, MinimizeIcon, MaximizeIcon, RestoreIcon, CloseIcon } from '../icons.mjs';
import { panelPosition, BUBBLE_SIZE, BUBBLE_SIZE_MOBILE } from './constants.mjs';
import { ConversationList } from './conversations.mjs';
import { MessageThread } from './thread.mjs';
import { InputComposer } from './composer.mjs';

export function ChatPanel({ conversations, activeConv, activeId, onSelectConversation, onNewConversation, onDeleteConversation, onSend, onStop, onDeleteMessage, onClose, onMinimize, bubblePos, isMobile, closing, streaming, streamingContent, streamingPhase, mentionData, llmConfigured }) {
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
                jsx('div', { className: 'text-sm font-semibold leading-tight', children: 'Meeting Notes' }),
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
        conversations, activeId, activeConv,
        onSelect: (id) => { onSelectConversation(id); setListExpanded(false); },
        onNew: onNewConversation,
        onDelete: onDeleteConversation,
        expanded: listExpanded,
        onToggleExpanded: () => setListExpanded(!listExpanded),
      }),
      jsx(MessageThread, { messages: activeConv ? activeConv.messages : [], streamingContent, streamingPhase, onDeleteMessage }),
      jsx(InputComposer, { onSend, onStop, streaming, mentionData, conversationId: activeId }),
    ]}),
  });
}

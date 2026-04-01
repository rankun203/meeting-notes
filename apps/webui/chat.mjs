import { useState, useEffect, useRef } from 'react';
import { jsx, jsxs, Fragment, useIsMobile, formatTime } from './utils.mjs';
import { ChatIcon, CloseIcon, SendIcon, MinimizeIcon, SparkleIcon, NewChatIcon } from './icons.mjs';

// ── Constants ──

const STORAGE_KEYS = {
  conversations: 'chat-conversations',
  activeId: 'chat-active-conversation-id',
  bubbleSnap: 'chat-bubble-position',
};

const MARGIN = 20;
const BUBBLE_SIZE = 56;
const BUBBLE_SIZE_MOBILE = 48;

// 8 snap positions: TL, TC, TR, ML, MR, BL, BC, BR
function getSnapPoints(w, h, size) {
  const m = MARGIN;
  return [
    { x: m, y: m },                                    // TL
    { x: (w - size) / 2, y: m },                       // TC
    { x: w - size - m, y: m },                          // TR
    { x: m, y: (h - size) / 2 },                        // ML
    { x: w - size - m, y: (h - size) / 2 },             // MR
    { x: m, y: h - size - m },                          // BL
    { x: (w - size) / 2, y: h - size - m },             // BC
    { x: w - size - m, y: h - size - m },               // BR (default)
  ];
}

function nearestSnap(x, y, w, h, size) {
  const points = getSnapPoints(w, h, size);
  let best = 7, bestDist = Infinity;
  for (let i = 0; i < points.length; i++) {
    const dx = x - points[i].x, dy = y - points[i].y;
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = i; }
  }
  return best;
}

function generateId() {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 7);
}

function createConversation() {
  const now = new Date().toISOString();
  return { id: generateId(), title: '', messages: [], created_at: now, updated_at: now };
}

// ── localStorage helpers ──

function loadJSON(key, fallback) {
  try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : fallback; }
  catch { return fallback; }
}

function saveJSON(key, value) {
  localStorage.setItem(key, JSON.stringify(value));
}

// ── Panel position relative to bubble ──

function panelPosition(bx, by, bSize, isMobile) {
  if (isMobile) return { top: 8, left: 0, right: 0, bottom: 0 };
  const w = window.innerWidth, h = window.innerHeight;
  const pw = 460, ph = Math.round(h * 0.9);
  // Determine which side the panel should open toward
  const centerX = bx + bSize / 2, centerY = by + bSize / 2;
  let left, top;
  // Horizontal: open toward center
  if (centerX > w / 2) left = bx - pw - 12;
  else left = bx + bSize + 12;
  // Vertical: align top of panel with bubble, but keep in viewport
  if (centerY > h / 2) top = by + bSize - ph;
  else top = by;
  // Clamp to viewport
  left = Math.max(8, Math.min(left, w - pw - 8));
  top = Math.max(8, Math.min(top, h - ph - 8));
  return { top, left, width: pw, height: ph };
}

// ── InputComposer ──

function InputComposer({ onSend }) {
  const [text, setText] = useState('');
  const textareaRef = useRef(null);

  useEffect(() => {
    if (textareaRef.current) textareaRef.current.focus();
  }, []);

  function handleInput(e) {
    setText(e.target.value);
    const el = e.target;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 96) + 'px';
  }

  function handleKeyDown(e) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  function send() {
    const trimmed = text.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setText('');
    if (textareaRef.current) { textareaRef.current.style.height = 'auto'; }
  }

  return jsx('div', {
    className: 'border-t border-gray-200 dark:border-gray-700 p-3 flex items-end gap-2 bg-white dark:bg-gray-900 rounded-b-2xl',
    children: jsxs(Fragment, { children: [
      jsx('textarea', {
        ref: textareaRef,
        value: text,
        onInput: handleInput,
        onKeyDown: handleKeyDown,
        placeholder: 'Type a message...',
        rows: 1,
        className: 'flex-1 resize-none rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 px-3 py-2 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-blue-500 placeholder-gray-400 dark:placeholder-gray-500',
        style: { maxHeight: '96px' },
      }),
      jsx('button', {
        onClick: send,
        disabled: !text.trim(),
        className: 'flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 hover:bg-blue-700 disabled:opacity-30 disabled:cursor-not-allowed flex items-center justify-center transition-colors',
        children: jsx(SendIcon, { className: 'w-3.5 h-3.5 text-white' }),
      }),
    ]}),
  });
}

// ── MessageThread ──

function MessageThread({ messages }) {
  const scrollRef = useRef(null);

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages.length]);

  if (!messages.length) {
    return jsx('div', {
      ref: scrollRef,
      className: 'flex-1 flex flex-col items-center justify-center text-gray-400 dark:text-gray-500 px-6',
      children: jsxs(Fragment, { children: [
        jsx(SparkleIcon, { className: 'w-10 h-10 mb-3 opacity-40' }),
        jsx('p', { className: 'text-sm font-medium', children: 'No messages yet' }),
        jsx('p', { className: 'text-xs mt-1 text-center', children: 'Ask a question about your meeting notes' }),
      ]}),
    });
  }

  // Group consecutive messages by role
  const groups = [];
  for (const msg of messages) {
    const last = groups[groups.length - 1];
    if (last && last.role === msg.role) last.msgs.push(msg);
    else groups.push({ role: msg.role, msgs: [msg] });
  }

  return jsx('div', {
    ref: scrollRef,
    className: 'flex-1 overflow-y-auto px-4 py-3 space-y-3',
    children: groups.map((group, gi) =>
      jsx('div', {
        key: gi,
        className: `flex ${group.role === 'user' ? 'justify-end' : 'justify-start'} gap-2`,
        children: jsxs(Fragment, { children: [
          group.role === 'assistant' && jsx('div', {
            className: 'flex-shrink-0 w-6 h-6 rounded-full bg-blue-100 dark:bg-blue-900/40 flex items-center justify-center mt-0.5',
            children: jsx(SparkleIcon, { className: 'w-3.5 h-3.5 text-blue-600 dark:text-blue-400' }),
          }),
          jsx('div', {
            className: `flex flex-col ${group.role === 'user' ? 'items-end' : 'items-start'} max-w-[75%] gap-0.5`,
            children: group.msgs.map((msg, mi) =>
              jsxs('div', {
                key: msg.id,
                children: [
                  jsx('div', {
                    className: group.role === 'user'
                      ? `px-3 py-2 text-sm rounded-2xl ${mi === group.msgs.length - 1 ? 'rounded-br-md' : ''} bg-blue-600 text-white`
                      : `px-3 py-2 text-sm rounded-2xl ${mi === group.msgs.length - 1 ? 'rounded-bl-md' : ''} bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100`,
                    children: msg.content,
                  }),
                  mi === group.msgs.length - 1 && jsx('span', {
                    className: 'text-[10px] text-gray-400 dark:text-gray-500 mt-0.5 px-1',
                    children: formatTime(msg.timestamp),
                  }),
                ],
              })
            ),
          }),
        ]}),
      })
    ),
  });
}

// ── ConversationList ──

function ConversationList({ conversations, activeId, onSelect, onNew, expanded, onToggleExpanded }) {
  if (!conversations.length) return null;

  const sorted = [...conversations].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  const shown = expanded ? sorted : sorted.slice(0, 1);

  return jsx('div', {
    className: 'border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-850',
    children: jsxs(Fragment, { children: [
      // Header row
      jsx('div', {
        className: 'flex items-center justify-between px-4 py-2',
        children: jsxs(Fragment, { children: [
          jsx('span', { className: 'text-[11px] font-medium text-gray-400 dark:text-gray-500 uppercase tracking-wider', children: 'Conversations' }),
          jsx('button', {
            onClick: onNew,
            className: 'p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors',
            title: 'New conversation',
            children: jsx(NewChatIcon, { className: 'w-3.5 h-3.5 text-gray-500 dark:text-gray-400' }),
          }),
        ]}),
      }),
      // Conversation items
      jsx('div', {
        className: 'px-2 pb-2 space-y-0.5',
        children: shown.map(conv =>
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
                  jsx('div', {
                    className: 'font-medium truncate',
                    children: conv.title || 'New conversation',
                  }),
                  conv.messages.length > 0 && jsx('div', {
                    className: 'text-[10px] text-gray-400 dark:text-gray-500 truncate mt-0.5',
                    children: conv.messages[conv.messages.length - 1].content,
                  }),
                ]}),
              }),
              jsx('span', {
                className: 'text-[10px] text-gray-400 dark:text-gray-500 flex-shrink-0',
                children: formatTime(conv.updated_at),
              }),
              conv.id !== activeId && jsx('span', {
                className: 'text-[10px] text-blue-500 dark:text-blue-400 flex-shrink-0 font-medium',
                children: 'Resume',
              }),
            ]}),
          })
        ),
      }),
      // Toggle expand
      sorted.length > 1 && jsx('button', {
        onClick: onToggleExpanded,
        className: 'w-full text-center py-1.5 text-[10px] text-blue-500 dark:text-blue-400 hover:text-blue-600 transition-colors border-t border-gray-200 dark:border-gray-700',
        children: expanded ? 'Show less' : `See all conversations (${sorted.length})`,
      }),
    ]}),
  });
}

// ── ChatPanel ──

function ChatPanel({ conversations, activeId, onSelectConversation, onNewConversation, onSend, onClose, onMinimize, bubblePos, isMobile, closing }) {
  const [listExpanded, setListExpanded] = useState(false);
  const activeConv = conversations.find(c => c.id === activeId);
  const bSize = isMobile ? BUBBLE_SIZE_MOBILE : BUBBLE_SIZE;
  const pos = panelPosition(bubblePos.x, bubblePos.y, bSize, isMobile);

  const panelStyle = isMobile
    ? { position: 'fixed', top: pos.top, left: pos.left, right: pos.right, bottom: pos.bottom, zIndex: 10000 }
    : { position: 'fixed', top: pos.top, left: pos.left, width: pos.width, height: pos.height, zIndex: 10000 };

  // Transform origin from bubble direction
  const bCenterX = bubblePos.x + bSize / 2, bCenterY = bubblePos.y + bSize / 2;
  const originX = isMobile ? '50%' : (bCenterX > window.innerWidth / 2 ? '100%' : '0%');
  const originY = isMobile ? '100%' : (bCenterY > window.innerHeight / 2 ? '100%' : '0%');

  return jsx('div', {
    style: { ...panelStyle, transformOrigin: `${originX} ${originY}` },
    className: `flex flex-col bg-white dark:bg-gray-900 ${isMobile ? 'rounded-t-2xl' : 'rounded-2xl'} shadow-2xl border border-gray-200 dark:border-gray-800 overflow-hidden ${closing ? 'chat-panel-exit' : 'chat-panel-enter'}`,
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
                    jsx('span', { className: 'w-1.5 h-1.5 rounded-full bg-green-400 inline-block' }),
                    'Online',
                  ],
                }),
              ]}),
            ],
          }),
          jsxs('div', {
            className: 'flex items-center gap-1',
            children: [
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
      // Conversation list
      conversations.length > 0 && jsx(ConversationList, {
        conversations,
        activeId,
        onSelect: onSelectConversation,
        onNew: onNewConversation,
        expanded: listExpanded,
        onToggleExpanded: () => setListExpanded(!listExpanded),
      }),
      // Message thread
      jsx(MessageThread, { messages: activeConv ? activeConv.messages : [] }),
      // Input composer
      jsx(InputComposer, { onSend }),
    ]}),
  });
}

// ── ChatBubble (main exported component) ──

export function ChatBubble() {
  const isMobile = useIsMobile();
  const bSize = isMobile ? BUBBLE_SIZE_MOBILE : BUBBLE_SIZE;

  // Conversations state
  const [conversations, setConversations] = useState(() => loadJSON(STORAGE_KEYS.conversations, []));
  const [activeId, setActiveId] = useState(() => loadJSON(STORAGE_KEYS.activeId, null));

  // Panel state
  const [panelOpen, setPanelOpen] = useState(false);
  const [panelClosing, setPanelClosing] = useState(false);

  // Bubble position
  const [snapIndex, setSnapIndex] = useState(() => loadJSON(STORAGE_KEYS.bubbleSnap, 7));
  const [bubblePos, setBubblePos] = useState(() => {
    const pts = getSnapPoints(window.innerWidth, window.innerHeight, bSize);
    const idx = loadJSON(STORAGE_KEYS.bubbleSnap, 7);
    return pts[idx] || pts[7];
  });
  const [dragging, setDragging] = useState(false);
  const [animating, setAnimating] = useState(false);
  const dragRef = useRef({ startX: 0, startY: 0, startBX: 0, startBY: 0, moved: false });

  // Tooltip state (hover-only)
  const [hovering, setHovering] = useState(false);

  // Ensure active conversation exists
  useEffect(() => {
    if (!activeId || !conversations.find(c => c.id === activeId)) {
      if (conversations.length > 0) {
        const sorted = [...conversations].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
        setActiveId(sorted[0].id);
      }
    }
  }, [activeId, conversations]);

  // Persist conversations
  useEffect(() => {
    saveJSON(STORAGE_KEYS.conversations, conversations);
  }, [conversations]);

  // Persist active ID
  useEffect(() => {
    saveJSON(STORAGE_KEYS.activeId, activeId);
  }, [activeId]);

  // Persist snap index
  useEffect(() => {
    saveJSON(STORAGE_KEYS.bubbleSnap, snapIndex);
  }, [snapIndex]);

  // Recalculate bubble position on window resize
  useEffect(() => {
    function onResize() {
      const newSize = window.innerWidth < 768 ? BUBBLE_SIZE_MOBILE : BUBBLE_SIZE;
      const pts = getSnapPoints(window.innerWidth, window.innerHeight, newSize);
      setBubblePos(pts[snapIndex] || pts[7]);
    }
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [snapIndex]);

  // ── Drag handlers ──

  function onPointerDown(e) {
    if (e.button !== 0) return;
    e.preventDefault();
    setHovering(false);
    dragRef.current = { startX: e.clientX, startY: e.clientY, startBX: bubblePos.x, startBY: bubblePos.y, moved: false };
    setDragging(true);
    setAnimating(false);
  }

  useEffect(() => {
    if (!dragging) return;

    function onMove(e) {
      const dx = e.clientX - dragRef.current.startX;
      const dy = e.clientY - dragRef.current.startY;
      if (Math.abs(dx) > 5 || Math.abs(dy) > 5) dragRef.current.moved = true;
      if (!dragRef.current.moved) return;
      const newX = Math.max(0, Math.min(window.innerWidth - bSize, dragRef.current.startBX + dx));
      const newY = Math.max(0, Math.min(window.innerHeight - bSize, dragRef.current.startBY + dy));
      setBubblePos({ x: newX, y: newY });
    }

    function onUp(e) {
      setDragging(false);
      if (dragRef.current.moved) {
        // Snap to nearest position
        const idx = nearestSnap(bubblePos.x, bubblePos.y, window.innerWidth, window.innerHeight, bSize);
        const pts = getSnapPoints(window.innerWidth, window.innerHeight, bSize);
        setSnapIndex(idx);
        setAnimating(true);
        setBubblePos(pts[idx]);
        setTimeout(() => setAnimating(false), 300);
      } else {
        // Click — toggle panel
        togglePanel();
      }
    }

    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
  }, [dragging, bubblePos, bSize]);

  // ── Panel toggle ──

  function togglePanel() {
    if (panelOpen) {
      closePanel();
    } else {
      // Ensure there's an active conversation
      if (!activeId || !conversations.find(c => c.id === activeId)) {
        const conv = createConversation();
        setConversations(prev => [...prev, conv]);
        setActiveId(conv.id);
      }
      setPanelOpen(true);
      setPanelClosing(false);
    }
  }

  function closePanel() {
    setPanelClosing(true);
    setTimeout(() => {
      setPanelOpen(false);
      setPanelClosing(false);
    }, 150);
  }

  // ── Conversation actions ──

  function handleNewConversation() {
    const conv = createConversation();
    setConversations(prev => [...prev, conv]);
    setActiveId(conv.id);
  }

  function handleSelectConversation(id) {
    setActiveId(id);
  }

  function handleSend(content) {
    const now = new Date().toISOString();
    const msg = { id: generateId(), role: 'user', content, timestamp: now };
    setConversations(prev => prev.map(c => {
      if (c.id !== activeId) return c;
      const updated = { ...c, messages: [...c.messages, msg], updated_at: now };
      // Auto-title from first message
      if (!c.title && c.messages.length === 0) updated.title = content.slice(0, 50);
      return updated;
    }));
  }

  // ── Unread count (placeholder for future) ──
  const unreadCount = 0;

  // ── Tooltip position ──
  const bCenterX = bubblePos.x + bSize / 2;
  const tooltipOnLeft = bCenterX > window.innerWidth / 2;

  return jsxs(Fragment, { children: [
    // Tooltip
    hovering && !panelOpen && !dragging && jsx('div', {
      className: 'chat-tooltip-enter fixed z-[9998] pointer-events-none',
      style: {
        top: bubblePos.y + bSize / 2 - 16,
        ...(tooltipOnLeft
          ? { right: window.innerWidth - bubblePos.x + 8 }
          : { left: bubblePos.x + bSize + 8 }),
      },
      children: jsx('div', {
        className: 'bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 text-xs font-medium px-3 py-1.5 rounded-lg shadow-lg whitespace-nowrap',
        children: 'Chat with my meeting notes?',
      }),
    }),

    // Bubble
    jsx('button', {
      onPointerDown,
      onMouseEnter: () => setHovering(true),
      onMouseLeave: () => setHovering(false),
      style: {
        position: 'fixed',
        left: bubblePos.x,
        top: bubblePos.y,
        width: bSize,
        height: bSize,
        zIndex: 9999,
        touchAction: 'none',
        transition: animating ? 'left 0.3s cubic-bezier(0.25,1,0.5,1), top 0.3s cubic-bezier(0.25,1,0.5,1)' : 'none',
        cursor: dragging ? 'grabbing' : 'pointer',
      },
      className: 'rounded-full bg-blue-600 hover:bg-blue-700 shadow-lg hover:shadow-xl flex items-center justify-center text-white select-none active:scale-95 transition-shadow',
      children: jsxs(Fragment, { children: [
        panelOpen
          ? jsx(CloseIcon, { className: 'w-5 h-5 text-white pointer-events-none' })
          : jsx(ChatIcon, { className: 'w-6 h-6 text-white pointer-events-none' }),
        unreadCount > 0 && jsx('span', {
          className: 'absolute -top-1 -right-1 min-w-[20px] h-5 rounded-full bg-red-500 text-white text-[10px] font-bold flex items-center justify-center px-1 pointer-events-none',
          children: unreadCount,
        }),
      ]}),
    }),

    // Panel
    panelOpen && jsx(ChatPanel, {
      conversations,
      activeId,
      onSelectConversation: handleSelectConversation,
      onNewConversation: handleNewConversation,
      onSend: handleSend,
      onClose: closePanel,
      onMinimize: closePanel,
      bubblePos,
      isMobile,
      closing: panelClosing,
    }),
  ]});
}

// ── Settings: ConversationsSettings ──

export function ConversationsSettings() {
  const [conversations, setConversations] = useState(() => loadJSON(STORAGE_KEYS.conversations, []));
  const [confirmClearAll, setConfirmClearAll] = useState(false);

  function refresh() {
    setConversations(loadJSON(STORAGE_KEYS.conversations, []));
  }

  function deleteConversation(id) {
    const updated = conversations.filter(c => c.id !== id);
    saveJSON(STORAGE_KEYS.conversations, updated);
    // Also update active ID if needed
    const activeId = loadJSON(STORAGE_KEYS.activeId, null);
    if (activeId === id) {
      saveJSON(STORAGE_KEYS.activeId, updated.length > 0 ? updated[0].id : null);
    }
    setConversations(updated);
  }

  function clearAll() {
    saveJSON(STORAGE_KEYS.conversations, []);
    saveJSON(STORAGE_KEYS.activeId, null);
    setConversations([]);
    setConfirmClearAll(false);
  }

  const sorted = [...conversations].sort((a, b) => b.updated_at.localeCompare(a.updated_at));

  return jsx('div', {
    className: 'space-y-4',
    children: jsxs(Fragment, { children: [
      // Header
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

      // List
      sorted.length === 0
        ? jsx('p', { className: 'text-sm text-gray-400 dark:text-gray-500', children: 'No conversations yet.' })
        : jsx('div', {
            className: 'space-y-2',
            children: sorted.map(conv =>
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
                        children: `${conv.messages.length} message${conv.messages.length !== 1 ? 's' : ''} · ${formatTime(conv.updated_at)}`,
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

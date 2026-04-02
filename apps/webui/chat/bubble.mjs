import { useState, useEffect, useRef, useCallback } from 'react';
import { jsx, jsxs, Fragment, api, API, useIsMobile } from '../utils.mjs';
import { ChatIcon, CloseIcon } from '../icons.mjs';
import { BUBBLE_SNAP_KEY, BUBBLE_SIZE, BUBBLE_SIZE_MOBILE, getSnapPoints, nearestSnap, loadSnap } from './constants.mjs';
import { ChatPanel } from './panel.mjs';

export function ChatBubble() {
  const isMobile = useIsMobile();
  const bSize = isMobile ? BUBBLE_SIZE_MOBILE : BUBBLE_SIZE;

  // Conversations state (API-backed)
  const [convList, setConvList] = useState([]);
  const [activeId, setActiveId] = useState(null);
  const [activeConv, setActiveConv] = useState(null);

  // Panel state
  const [panelOpen, setPanelOpen] = useState(false);
  const [panelClosing, setPanelClosing] = useState(false);

  // Streaming state
  const [streaming, setStreaming] = useState(false);
  const [streamingContent, setStreamingContent] = useState(null);
  const [streamingPhase, setStreamingPhase] = useState(null); // 'thinking' | 'streaming' | null

  // Mention data (cached)
  const [mentionData, setMentionData] = useState({ tags: [], people: [], sessions: [] });

  // LLM configured status
  const [llmConfigured, setLlmConfigured] = useState(false);

  // Bubble position
  const [snapIndex, setSnapIndex] = useState(loadSnap);
  const [bubblePos, setBubblePos] = useState(() => {
    const pts = getSnapPoints(window.innerWidth, window.innerHeight, bSize);
    const idx = loadSnap();
    return pts[idx] || pts[7];
  });
  const [dragging, setDragging] = useState(false);
  const [animating, setAnimating] = useState(false);
  const dragRef = useRef({ startX: 0, startY: 0, startBX: 0, startBY: 0, moved: false });
  const [hovering, setHovering] = useState(false);

  const refreshConvList = useCallback(async () => {
    try {
      const data = await api('/conversations');
      setConvList(data.conversations || []);
    } catch {}
  }, []);

  const loadConversation = useCallback(async (id) => {
    if (!id) { setActiveConv(null); return; }
    try {
      const conv = await api(`/conversations/${id}`);
      setActiveConv(conv);
    } catch { setActiveConv(null); }
  }, []);

  // Load mention data when panel opens
  useEffect(() => {
    if (!panelOpen) return;
    refreshConvList();
    api('/settings').then(s => setLlmConfigured(!!s.llm_api_key_set)).catch(() => {});
    Promise.all([
      api('/tags').catch(() => ({ tags: [] })),
      api('/people').catch(() => ({ people: [] })),
      api('/sessions?limit=100&offset=0').catch(() => ({ sessions: [] })),
    ]).then(([tags, people, sessions]) => {
      setMentionData({
        tags: tags.tags || [],
        people: people.people || [],
        sessions: sessions.sessions || [],
      });
    });
  }, [panelOpen]);

  useEffect(() => {
    if (activeId) loadConversation(activeId);
  }, [activeId]);

  useEffect(() => {
    localStorage.setItem(BUBBLE_SNAP_KEY, JSON.stringify(snapIndex));
  }, [snapIndex]);

  useEffect(() => {
    function onResize() {
      const newSize = window.innerWidth < 768 ? BUBBLE_SIZE_MOBILE : BUBBLE_SIZE;
      const pts = getSnapPoints(window.innerWidth, window.innerHeight, newSize);
      setBubblePos(pts[snapIndex] || pts[7]);
    }
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [snapIndex]);

  // ── Drag ──

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
      setBubblePos({
        x: Math.max(0, Math.min(window.innerWidth - bSize, dragRef.current.startBX + dx)),
        y: Math.max(0, Math.min(window.innerHeight - bSize, dragRef.current.startBY + dy)),
      });
    }
    function onUp() {
      setDragging(false);
      if (dragRef.current.moved) {
        const idx = nearestSnap(bubblePos.x, bubblePos.y, window.innerWidth, window.innerHeight, bSize);
        const pts = getSnapPoints(window.innerWidth, window.innerHeight, bSize);
        setSnapIndex(idx);
        setAnimating(true);
        setBubblePos(pts[idx]);
        setTimeout(() => setAnimating(false), 300);
      } else {
        togglePanel();
      }
    }
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    return () => { window.removeEventListener('pointermove', onMove); window.removeEventListener('pointerup', onUp); };
  }, [dragging, bubblePos, bSize]);

  // ── Panel ──

  async function togglePanel() {
    if (panelOpen) {
      closePanel();
    } else {
      setPanelOpen(true);
      setPanelClosing(false);
      if (!activeId) {
        try {
          const conv = await api('/conversations', { method: 'POST', body: JSON.stringify({}) });
          setActiveId(conv.id);
          await refreshConvList();
        } catch {}
      }
    }
  }

  function closePanel() {
    setPanelClosing(true);
    setTimeout(() => { setPanelOpen(false); setPanelClosing(false); }, 150);
  }

  async function handleNewConversation() {
    try {
      const conv = await api('/conversations', { method: 'POST', body: JSON.stringify({}) });
      setActiveId(conv.id);
      setActiveConv(conv);
      await refreshConvList();
    } catch {}
  }

  function handleSelectConversation(id) {
    setActiveId(id);
  }

  async function handleSend(content, mentions) {
    if (!activeId || streaming) return;

    const userMsg = { role: 'user', id: 'pending_' + Date.now(), content, mentions, timestamp: new Date().toISOString() };
    setActiveConv(prev => prev ? { ...prev, messages: [...prev.messages, userMsg] } : prev);
    setStreaming(true);
    setStreamingContent('');
    setStreamingPhase('thinking');

    try {
      const res = await fetch(`${API}/conversations/${activeId}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content, mentions }),
      });

      if (!res.ok) {
        const err = await res.json().catch(() => ({}));
        throw new Error(err.error || `HTTP ${res.status}`);
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let fullContent = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.startsWith('data: ')) continue;
          const data = line.slice(6);
          try {
            const parsed = JSON.parse(data);
            if (parsed.content !== undefined) {
              fullContent += parsed.content;
              setStreamingPhase('streaming');
              setStreamingContent(fullContent);
            } else if (parsed.error) {
              setStreamingContent(null);
              setStreaming(false);
              setStreamingPhase(null);
              setActiveConv(prev => prev ? {
                ...prev,
                messages: [...prev.messages, { role: 'assistant', id: 'err_' + Date.now(), content: `Error: ${parsed.error}`, timestamp: new Date().toISOString() }]
              } : prev);
              return;
            } else if (parsed.chunk_count !== undefined) {
              setActiveConv(prev => prev ? {
                ...prev,
                messages: [...prev.messages, { role: 'context_result', id: 'ctx_' + Date.now(), chunks: new Array(parsed.chunk_count), timestamp: new Date().toISOString() }]
              } : prev);
            }
          } catch {}
        }
      }

      setStreamingContent(null);
      setStreaming(false);
      setStreamingPhase(null);
      await loadConversation(activeId);
      await refreshConvList();

    } catch (e) {
      setStreamingContent(null);
      setStreaming(false);
      setStreamingPhase(null);
      setActiveConv(prev => prev ? {
        ...prev,
        messages: [...prev.messages, { role: 'assistant', id: 'err_' + Date.now(), content: `Error: ${e.message}`, timestamp: new Date().toISOString() }]
      } : prev);
    }
  }

  const unreadCount = 0;
  const bCenterX = bubblePos.x + bSize / 2;
  const tooltipOnLeft = bCenterX > window.innerWidth / 2;

  return jsxs(Fragment, { children: [
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

    jsx('button', {
      onPointerDown,
      onMouseEnter: () => setHovering(true),
      onMouseLeave: () => setHovering(false),
      style: {
        position: 'fixed', left: bubblePos.x, top: bubblePos.y,
        width: bSize, height: bSize, zIndex: 9999, touchAction: 'none',
        transition: animating ? 'left 0.3s cubic-bezier(0.25,1,0.5,1), top 0.3s cubic-bezier(0.25,1,0.5,1)' : 'none',
        cursor: dragging ? 'grabbing' : 'pointer',
      },
      className: 'rounded-full bg-blue-600 hover:bg-blue-700 shadow-lg hover:shadow-xl flex items-center justify-center text-white select-none active:scale-95 transition-shadow',
      children: jsxs(Fragment, { children: [
        panelOpen
          ? jsx(CloseIcon, { className: 'w-6 h-6 text-white pointer-events-none' })
          : jsx(ChatIcon, { className: 'w-6 h-6 text-white pointer-events-none' }),
        unreadCount > 0 && jsx('span', {
          className: 'absolute -top-1 -right-1 min-w-[20px] h-5 rounded-full bg-red-500 text-white text-[10px] font-bold flex items-center justify-center px-1 pointer-events-none',
          children: unreadCount,
        }),
      ]}),
    }),

    panelOpen && jsx(ChatPanel, {
      conversations: convList, activeConv, activeId,
      onSelectConversation: handleSelectConversation,
      onNewConversation: handleNewConversation,
      onSend: handleSend,
      onClose: closePanel, onMinimize: closePanel,
      bubblePos, isMobile, closing: panelClosing,
      streaming, streamingContent, streamingPhase, mentionData, llmConfigured,
    }),
  ]});
}

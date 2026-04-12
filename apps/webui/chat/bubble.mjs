import { useState, useEffect, useRef, useCallback } from 'react';
import { jsx, jsxs, Fragment, api, API, apiClaudeSend, apiSendMessage, useIsMobile } from '../utils.mjs';
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
  const [streamingThinking, setStreamingThinking] = useState(null);
  const [streamingPhase, setStreamingPhase] = useState(null); // 'thinking' | 'streaming' | null
  const [tokenUsage, setTokenUsage] = useState(null); // { prompt_tokens, completion_tokens }

  // Mention data (cached)
  const [mentionData, setMentionData] = useState({ tags: [], people: [], sessions: [] });

  // LLM configured status
  const [llmConfigured, setLlmConfigured] = useState(false);

  // Chat backend: 'openrouter' or 'claude_code'
  const [chatBackend, setChatBackend] = useState('openrouter');
  // Claude Code session ID for --resume
  const [claudeSessionId, setClaudeSessionId] = useState(null);
  // Tool activity for Claude Code (accumulates during a response)
  const [toolActivities, setToolActivities] = useState([]);
  // Full prompts sent to Claude Code (for export)
  const claudePromptsRef = useRef([]);
  // Track tool activities during streaming (ref for access in finalize)
  const toolActivitiesRef = useRef([]);
  // Pending permission requests (persists across conversation reloads)
  const [pendingPermissions, setPendingPermissions] = useState([]);

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
  const abortRef = useRef(null);

  const chatBackendRef = useRef(chatBackend);
  useEffect(() => { chatBackendRef.current = chatBackend; }, [chatBackend]);

  const refreshConvList = useCallback(async () => {
    try {
      const data = await api('/conversations');
      const backend = chatBackendRef.current;
      // Filter by backend
      const filtered = (data.conversations || []).filter(c =>
        backend === 'claude_code'
          ? c.chat_backend === 'claude_code'
          : !c.chat_backend || c.chat_backend === 'openrouter'
      );
      setConvList(filtered);
    } catch {}
  }, []);

  const loadConversation = useCallback(async (id) => {
    if (!id) { setActiveConv(null); return; }
    try {
      const conv = await api(`/conversations/${id}`);
      setActiveConv(conv);
      if (conv.claude_session_id) {
        setClaudeSessionId(conv.claude_session_id);
      }
    } catch { setActiveConv(null); }
  }, []);

  // Load mention data when panel opens
  useEffect(() => {
    if (!panelOpen) return;
    api('/settings').then(s => {
      setLlmConfigured(!!s.llm_api_key_set);
      const backend = s.chat_backend || 'openrouter';
      setChatBackend(backend);
      chatBackendRef.current = backend;
      refreshConvList();
    }).catch(() => { refreshConvList(); });
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
    if (activeId && !streaming) loadConversation(activeId);
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
    }
  }

  function closePanel() {
    setPanelClosing(true);
    setTimeout(() => { setPanelOpen(false); setPanelClosing(false); }, 150);
  }

  function handleNewConversation() {
    setActiveId(null);
    setActiveConv(null);
  }

  function handleSelectConversation(id) {
    setActiveId(id);
  }

  async function handleDeleteConversation(id) {
    if (!confirm('Delete this conversation?')) return;
    try {
      await api(`/conversations/${id}`, { method: 'DELETE' });
      if (activeId === id) {
        setActiveId(null);
        setActiveConv(null);
      }
      await refreshConvList();
    } catch {}
  }

  async function handleDeleteMessage(msgId) {
    if (!activeId) return;
    try {
      await api(`/conversations/${activeId}/messages/${msgId}`, { method: 'DELETE' });
      await loadConversation(activeId);
      await refreshConvList();
    } catch {}
  }

  function handleStop() {
    if (abortRef.current) abortRef.current.abort();
    if (chatBackend === 'claude_code') {
      api('/claude/stop', { method: 'POST' }).catch(() => {});
    }
  }

  async function handleSendClaude(content, mentions, { internal = false } = {}) {
    if (streaming) return;

    // Create app conversation lazily on first message
    let convId = activeId;
    if (!convId) {
      try {
        const conv = await api('/conversations', { method: 'POST', body: JSON.stringify({ title: content.slice(0, 60), chat_backend: 'claude_code' }) });
        convId = conv.id;
        setActiveId(convId);
        setActiveConv(conv);
      } catch { return; }
    }

    const userMsgId = 'pending_' + Date.now();
    if (!internal) {
      const userMsg = { role: 'user', id: userMsgId, content, mentions, timestamp: new Date().toISOString() };
      setActiveConv(prev => prev
        ? { ...prev, messages: [...prev.messages, userMsg] }
        : { id: convId, title: content.slice(0, 60), messages: [userMsg] });
    }
    setStreaming(true);
    setStreamingContent('');
    setStreamingPhase('streaming');
    setToolActivities([]);
    toolActivitiesRef.current = [];

    // Wrap the callback-based apiClaudeSend in a Promise so the rest
    // of this function can keep its imperative shape. The helper works
    // on both transports (SSE in the daemon-served browser, Tauri
    // `Channel<ClaudeStreamEvent>` in the desktop app) so this single
    // code path handles both.
    let fullContent = '';
    let aborted = false;
    const streamPromise = new Promise((resolve, reject) => {
      const stop = apiClaudeSend(
        {
          prompt: content,
          session_id: claudeSessionId,
          mentions: mentions?.length ? mentions : undefined,
        },
        (event) => {
          if (aborted) return;
          switch (event.type) {
            case 'prompt':
              claudePromptsRef.current.push(event.full_prompt);
              break;
            case 'init':
              setClaudeSessionId(event.session_id);
              break;
            case 'delta':
              if (event.content) {
                fullContent += event.content;
                setStreamingContent(fullContent);
              }
              break;
            case 'tool_use': {
              const ta = { tool: event.tool, summary: event.input_summary };
              toolActivitiesRef.current.push(ta);
              setToolActivities(prev => [...prev, ta]);
              break;
            }
            case 'permission_request': {
              const tools = event.tools || [];
              if (tools.length > 0) {
                const lastActivity = toolActivitiesRef.current[toolActivitiesRef.current.length - 1];
                const preview = lastActivity ? `${lastActivity.tool}: ${lastActivity.summary}` : null;
                setPendingPermissions(prev => {
                  const existing = new Set(prev.flatMap(p => p.tools));
                  const newTools = tools.filter(t => !existing.has(t));
                  return newTools.length > 0 ? [...prev, { id: Date.now(), tools: newTools, preview }] : prev;
                });
              }
              break;
            }
            case 'done':
              setClaudeSessionId(event.session_id);
              setTokenUsage(event.cost_usd != null ? { cost_usd: event.cost_usd } : null);
              resolve();
              break;
            case 'error':
              reject(new Error(event.error || 'claude_send error'));
              break;
          }
        }
      );
      // Expose the abort handle so the stop button + session switches
      // can cancel a running stream. We stash both the abort fn and a
      // flag the event handler honours, because Tauri Channels can't
      // actually interrupt an already-spawned Rust command — we just
      // stop reacting to further events.
      abortRef.current = {
        abort() {
          aborted = true;
          try { stop(); } catch {}
          reject(Object.assign(new Error('aborted'), { name: 'AbortError' }));
        },
      };
    });

    try {
      await streamPromise;

      // Finalize: add assistant message to local state (with tool activities preserved)
      const finalTools = toolActivitiesRef.current.length > 0 ? [...toolActivitiesRef.current] : undefined;
      setStreamingContent(null);
      setStreaming(false);
      setStreamingPhase(null);
      setToolActivities([]);
      if (fullContent) {
        setActiveConv(prev => prev ? {
          ...prev,
          messages: [...prev.messages, { role: 'assistant', id: 'claude_' + Date.now(), content: fullContent, _toolActivities: finalTools, timestamp: new Date().toISOString() }]
        } : prev);
      }
      // Persist to app conversation system
      if (convId) {
        const syncMessages = [];
        if (!internal) {
          syncMessages.push({ role: 'user', id: userMsgId, content, mentions });
        }
        if (fullContent) {
          syncMessages.push({ role: 'assistant', id: 'claude_' + Date.now(), content: fullContent });
        }
        await api(`/conversations/${convId}/claude-sync`, {
          method: 'POST',
          body: JSON.stringify({
            claude_session_id: claudeSessionId,
            messages: syncMessages,
          }),
        }).catch(() => {});
        await loadConversation(convId);
        await refreshConvList();
      }
    } catch (e) {
      if (e.name === 'AbortError') {
        setStreamingContent(null);
        setStreaming(false);
        setStreamingPhase(null);
        setToolActivities([]);
        return;
      }
      setStreamingContent(null);
      setStreaming(false);
      setStreamingPhase(null);
      setToolActivities([]);
      setActiveConv(prev => prev ? {
        ...prev,
        messages: [...prev.messages, { role: 'assistant', id: 'err_' + Date.now(), content: `Error: ${e.message}`, timestamp: new Date().toISOString() }]
      } : prev);
    } finally {
      abortRef.current = null;
    }
  }

  async function handleApproveTools(tools, scope) {
    try {
      await api('/claude/approve-tools', {
        method: 'POST',
        body: JSON.stringify({ tools, scope }),
      });
      setPendingPermissions(prev => prev.filter(p => !p.tools.every(t => tools.includes(t))));
      // Resume the session with a short continuation prompt instead of re-sending the original
      if (!streaming && claudeSessionId) {
        handleSendClaude('Permission approved, please continue.', [], { internal: true });
      }
    } catch (e) {
      alert('Failed to approve: ' + e.message);
    }
  }

  function handleDenyPermissions() {
    setPendingPermissions([]);
  }

  function handleExportClaude() {
    const conv = activeConv;
    if (!conv || !conv.messages?.length) return;
    const prompts = claudePromptsRef.current;
    let out = '';
    let promptIdx = 0;
    for (const msg of conv.messages) {
      if (msg.role === 'user') {
        out += `\n=== USER ===\n\n`;
        // Use the full prompt (with references) if available
        if (promptIdx < prompts.length) {
          out += prompts[promptIdx] + '\n';
          promptIdx++;
        } else {
          out += (msg.content || '') + '\n';
        }
      } else if (msg.role === 'assistant') {
        out += `\n=== ASSISTANT ===\n\n`;
        out += (msg.content || '') + '\n';
      }
    }
    const blob = new Blob([out.trim() + '\n'], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `claude-chat-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }

  async function handleSend(content, mentions) {
    if (streaming) return;

    // Create conversation lazily on first message
    let convId = activeId;
    if (!convId) {
      try {
        const conv = await api('/conversations', { method: 'POST', body: JSON.stringify({}) });
        convId = conv.id;
        setActiveId(convId);
        setActiveConv(conv);
      } catch { return; }
    }

    const userMsg = { role: 'user', id: 'pending_' + Date.now(), content, mentions, timestamp: new Date().toISOString() };
    setActiveConv(prev => prev ? { ...prev, messages: [...prev.messages, userMsg] } : { id: convId, messages: [userMsg] });
    setStreaming(true);
    setStreamingContent('');
    setStreamingThinking('');
    setStreamingPhase('thinking');
    setTokenUsage(null);

    // Wrap the callback-based apiSendMessage in a Promise so the rest
    // of this function can keep its imperative shape. Works on both
    // transports (SSE in daemon/browser, Tauri Channel<ChatEvent> in
    // the desktop app).
    let aborted = false;
    const streamPromise = new Promise((resolve, reject) => {
      const stop = apiSendMessage(
        convId,
        { content, mentions },
        (event) => {
          if (aborted) return;
          switch (event.type) {
            case 'context_loaded':
              setActiveConv(prev => prev ? {
                ...prev,
                messages: [...prev.messages, {
                  role: 'context_result',
                  id: 'ctx_' + Date.now(),
                  chunks: new Array(event.chunk_count || 0),
                  timestamp: new Date().toISOString(),
                }],
              } : prev);
              break;
            case 'usage': {
              // Strip the `type` field so the downstream consumer only
              // sees the raw usage payload (prompt_tokens, etc.).
              const { type, ...usage } = event;
              setTokenUsage(usage);
              break;
            }
            case 'thinking':
              if (event.content !== undefined) {
                // We keep a running local copy in the handler closure via
                // the setState updater function so React schedules the
                // merge correctly under fast bursts.
                setStreamingThinking(prev => (prev || '') + event.content);
              }
              break;
            case 'delta':
              if (event.content !== undefined) {
                setStreamingPhase('streaming');
                setStreamingContent(prev => (prev || '') + event.content);
              }
              break;
            case 'done':
              resolve();
              break;
            case 'error':
              reject(new Error(event.error || 'chat error'));
              break;
          }
        }
      );
      abortRef.current = {
        abort() {
          aborted = true;
          try { stop(); } catch {}
          reject(Object.assign(new Error('aborted'), { name: 'AbortError' }));
        },
      };
    });

    try {
      await streamPromise;
      setStreamingContent(null);
      setStreamingThinking(null);
      setStreaming(false);
      setStreamingPhase(null);
      await loadConversation(convId);
      await refreshConvList();
    } catch (e) {
      if (e.name === 'AbortError') {
        // Cancelled by user — keep whatever was streamed
        setStreamingContent(null);
        setStreamingThinking(null);
        setStreaming(false);
        setStreamingPhase(null);
        await loadConversation(convId);
        await refreshConvList();
        return;
      }
      setStreamingContent(null);
      setStreamingThinking(null);
      setStreaming(false);
      setStreamingPhase(null);
      setActiveConv(prev => prev ? {
        ...prev,
        messages: [...prev.messages, { role: 'assistant', id: 'err_' + Date.now(), content: `Error: ${e.message}`, timestamp: new Date().toISOString() }]
      } : prev);
    } finally {
      abortRef.current = null;
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
      onNewConversation: () => { handleNewConversation(); if (chatBackend === 'claude_code') { setClaudeSessionId(null); claudePromptsRef.current = []; } },
      onDeleteConversation: handleDeleteConversation,
      onSend: chatBackend === 'claude_code' ? handleSendClaude : handleSend,
      onStop: handleStop,
      onDeleteMessage: handleDeleteMessage,
      onClose: closePanel, onMinimize: closePanel,
      bubblePos, isMobile, closing: panelClosing,
      streaming, streamingContent, streamingThinking, streamingPhase, tokenUsage, mentionData, llmConfigured,
      chatBackend, toolActivities,
      onSendToClaudeCode: chatBackend === 'claude_code' ? handleExportClaude : null,
      onApproveTools: chatBackend === 'claude_code' ? handleApproveTools : null,
      onDenyPermissions: chatBackend === 'claude_code' ? handleDenyPermissions : null,
      pendingPermissions,
    }),
  ]});
}

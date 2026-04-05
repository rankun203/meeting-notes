import { useState, useEffect, useCallback, useRef } from 'react';
import { createRoot } from 'react-dom/client';
import { jsx, jsxs, Fragment, api, PAGE_SIZE, useIsMobile, useWebSocket } from './utils.mjs';
import { parseRoute, buildPath } from './router.mjs';
import { SessionDetail } from './session.mjs';
import { PersonDetail } from './people.mjs';
import { SettingsPage } from './settings.mjs';
import { Sidebar } from './sidebar.mjs';
import { ChatBubble } from './chat.mjs';

function App() {
  // Initialize state from URL
  const initialRoute = parseRoute(window.location.pathname);

  const [sessions, setSessions] = useState([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [sources, setSources] = useState([]);
  const [fields, setFields] = useState({});
  const [selectedId, setSelectedId] = useState(initialRoute.selectedId ?? null);
  const [showNew, setShowNew] = useState(false);
  const [mobileView, setMobileView] = useState(initialRoute.selectedId ? 'detail' : 'list');
  const [currentView, setCurrentView] = useState(initialRoute.view);
  const [settingsCategory, setSettingsCategory] = useState(initialRoute.settingsCategory ?? 'services');
  const [people, setPeople] = useState([]);
  const [selectedPersonId, setSelectedPersonId] = useState(initialRoute.selectedPersonId ?? null);
  const isMobile = useIsMobile();
  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;
  // Track whether the URL already had a session ID on load (suppress auto-select)
  const hadInitialId = useRef(!!initialRoute.selectedId);

  // Central navigation function — updates state + pushes URL
  function navigateTo(path, replace) {
    if (replace) {
      history.replaceState(null, '', path);
    } else {
      history.pushState(null, '', path);
    }
    const r = parseRoute(path);
    setCurrentView(r.view);
    setSelectedId(r.selectedId ?? null);
    setSelectedPersonId(r.selectedPersonId ?? null);
    setSettingsCategory(r.settingsCategory ?? 'services');
    // Mobile view
    if (r.view === 'sessions' && r.selectedId) setMobileView('detail');
    else if (r.view === 'sessions') setMobileView('list');
  }

  // Handle browser back/forward
  useEffect(() => {
    function onPopState() {
      const r = parseRoute(window.location.pathname);
      setCurrentView(r.view);
      setSelectedId(r.selectedId ?? null);
      setSelectedPersonId(r.selectedPersonId ?? null);
      setSettingsCategory(r.settingsCategory ?? 'services');
      if (r.view === 'sessions' && r.selectedId) setMobileView('detail');
      else if (r.view === 'sessions') setMobileView('list');
    }
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  function handleWsEvent(event) {
    switch (event.type) {
      case 'init':
        setSessions(event.data.sessions || []);
        setTotal(event.data.total || 0);
        break;
      case 'session_created':
        setSessions(prev => [event.data, ...prev]);
        setTotal(prev => prev + 1);
        break;
      case 'session_updated':
        setSessions(prev => prev.map(s => s.id === event.data.id ? event.data : s));
        break;
      case 'session_deleted':
        setSessions(prev => prev.filter(s => s.id !== event.data.id));
        setTotal(prev => Math.max(0, prev - 1));
        if (selectedIdRef.current === event.data.id) navigateTo('/sessions');
        break;
      case 'file_sizes':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id ? { ...s, file_sizes: event.data.file_sizes, auto_stop_remaining_secs: event.data.auto_stop_remaining_secs ?? null } : s
        ));
        break;
      case 'session_notice':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, notices: [...(s.notices || []), event.data.notice] }
            : s
        ));
        break;
      case 'session_notices':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, notices: event.data.notices }
            : s
        ));
        break;
      case 'transcription_progress':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, processing_state: event.data.status }
            : s
        ));
        break;
      case 'transcription_completed':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, processing_state: null, transcript_available: true, unconfirmed_speakers: event.data.unconfirmed_speakers }
            : s
        ));
        break;
      case 'transcription_failed':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, processing_state: null }
            : s
        ));
        break;
      case 'summary_progress':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, summary_processing: event.data.status || true, summary_streaming: '' }
            : s
        ));
        break;
      case 'summary_delta':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, summary_streaming: (s.summary_streaming || '') + event.data.delta }
            : s
        ));
        break;
      case 'summary_completed':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, summary_available: true, summary_processing: false }
            : s
        ));
        break;
      case 'summary_failed':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id
            ? { ...s, summary_processing: false, summary_streaming: null }
            : s
        ));
        break;
    }
  }

  useWebSocket(handleWsEvent);

  useEffect(() => {
    api('/config').then(d => { setSources(d.sources || []); setFields(d.fields || {}); }).catch(() => {});
  }, []);

  const refreshPeople = useCallback(() => {
    api('/people').then(d => setPeople(d.people || [])).catch(() => {});
  }, []);

  useEffect(() => {
    if (currentView === 'people') refreshPeople();
  }, [currentView]);

  const refresh = useCallback(async (currentOffset) => {
    try {
      const data = await api(`/sessions?limit=${PAGE_SIZE}&offset=${currentOffset ?? offset}`);
      setSessions(data.sessions);
      setTotal(data.total);
    } catch (e) { /* ignore */ }
  }, [offset]);

  // Auto-select first session only if URL didn't specify one
  useEffect(() => {
    if (currentView === 'sessions' && sessions.length > 0 && !selectedId && !hadInitialId.current) {
      const recording = sessions.find(s => s.state === 'recording');
      const autoId = recording ? recording.id : sessions[0].id;
      navigateTo(buildPath('sessions', autoId), true); // replaceState, not pushState
    }
    hadInitialId.current = false; // only suppress once
  }, [sessions]);

  function handleSelect(id) {
    navigateTo(buildPath('sessions', id));
  }

  function handleBack() {
    navigateTo(buildPath('sessions'));
  }

  function handlePageChange(newOffset) {
    setOffset(newOffset);
    refresh(newOffset);
  }

  function handleDeleted() {
    navigateTo(buildPath('sessions'));
  }

  // Refresh session list when leaving settings (hidden tags may have changed)
  const prevViewRef = useRef(currentView);
  useEffect(() => {
    if (prevViewRef.current === 'settings' && currentView !== 'settings') refresh();
    prevViewRef.current = currentView;
  }, [currentView]);

  const selectedSession = sessions.find(s => s.id === selectedId) || null;

  const sidebarProps = {
    sessions, total, offset,
    selectedId,
    onSelect: handleSelect,
    onPageChange: handlePageChange,
    sources, fields,
    onCreated: async () => { setOffset(0); await refresh(0); },
    showNew, setShowNew,
    currentView,
    onViewChange: (v) => navigateTo(buildPath(v)),
    people,
    selectedPersonId,
    setSelectedPersonId: (id) => navigateTo(buildPath('people', id)),
    refreshPeople,
    settingsCategory,
    setSettingsCategory: (cat) => navigateTo(buildPath('settings', cat)),
  };

  function mainContent() {
    if (currentView === 'settings') return jsx(SettingsPage, { category: settingsCategory, onSelectSession: (id) => navigateTo(buildPath('sessions', id)) });
    if (currentView === 'people') {
      const selectedPerson = people.find(p => p.id === selectedPersonId) || people[0] || null;
      return jsx(PersonDetail, {
        person: selectedPerson,
        onRefresh: () => { refreshPeople(); navigateTo(buildPath('people')); },
        onSelectSession: (id) => navigateTo(buildPath('sessions', id)),
      });
    }
    return jsx(SessionDetail, {
      session: selectedSession,
      onRefresh: refresh,
      onDeleted: handleDeleted,
      onBack: handleBack,
      isMobile: isMobile,
      onSelectPerson: (personId) => navigateTo(buildPath('people', personId)),
      fields,
    });
  }

  const chatBubble = jsx(ChatBubble, {});

  if (isMobile) {
    if (currentView !== 'sessions') {
      return jsxs(Fragment, { children: [
        jsx('div', { className: 'h-full bg-gray-50 dark:bg-gray-950', children: mainContent() }),
        chatBubble,
      ]});
    }
    if (mobileView === 'detail' && selectedSession) {
      return jsxs(Fragment, { children: [
        jsx('div', { className: 'h-full bg-gray-50 dark:bg-gray-950', children: mainContent() }),
        chatBubble,
      ]});
    }
    return jsxs(Fragment, { children: [
      jsx('div', { className: 'h-full', children: jsx(Sidebar, sidebarProps) }),
      chatBubble,
    ]});
  }

  return jsxs(Fragment, { children: [
    jsxs('div', {
      className: 'h-full flex',
      children: [
        jsx('div', { className: 'w-72 flex-shrink-0 h-full', children: jsx(Sidebar, sidebarProps) }),
        jsx('div', { className: 'flex-1 h-full bg-gray-50 dark:bg-gray-950', children: mainContent() }),
      ],
    }),
    chatBubble,
  ]});
}

createRoot(document.getElementById('root')).render(jsx(App, {}));

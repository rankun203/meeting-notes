import { useState, useEffect, useCallback, useRef } from 'react';
import { createRoot } from 'react-dom/client';
import { jsx, jsxs, api, PAGE_SIZE, useIsMobile, useWebSocket } from './utils.mjs';
import { SessionDetail } from './session.mjs';
import { PersonDetail } from './people.mjs';
import { SettingsPage } from './settings.mjs';
import { Sidebar } from './sidebar.mjs';

function App() {
  const [sessions, setSessions] = useState([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [sources, setSources] = useState([]);
  const [fields, setFields] = useState({});
  const [selectedId, setSelectedId] = useState(null);
  const [showNew, setShowNew] = useState(false);
  const [mobileView, setMobileView] = useState('list');
  const [currentView, setCurrentView] = useState('sessions');
  const [settingsCategory, setSettingsCategory] = useState('services');
  const [people, setPeople] = useState([]);
  const [selectedPersonId, setSelectedPersonId] = useState(null);
  const isMobile = useIsMobile();
  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;

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
        if (selectedIdRef.current === event.data.id) setSelectedId(null);
        break;
      case 'file_sizes':
        setSessions(prev => prev.map(s =>
          s.id === event.data.id ? { ...s, file_sizes: event.data.file_sizes } : s
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

  useEffect(() => {
    if (sessions.length > 0 && !selectedId) {
      const recording = sessions.find(s => s.state === 'recording');
      setSelectedId(recording ? recording.id : sessions[0].id);
    }
  }, [sessions]);

  function handleSelect(id) {
    setSelectedId(id);
    if (isMobile) setMobileView('detail');
  }

  function handleBack() {
    setMobileView('list');
  }

  function handlePageChange(newOffset) {
    setOffset(newOffset);
    refresh(newOffset);
  }

  function handleDeleted() {
    setSelectedId(null);
    if (isMobile) setMobileView('list');
  }

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
    onViewChange: (v) => { setCurrentView(v); if (v !== 'sessions') setSelectedId(null); },
    people, selectedPersonId, setSelectedPersonId, refreshPeople,
    settingsCategory, setSettingsCategory,
  };

  function mainContent() {
    if (currentView === 'settings') return jsx(SettingsPage, { category: settingsCategory });
    if (currentView === 'people') {
      const selectedPerson = people.find(p => p.id === selectedPersonId) || people[0] || null;
      return jsx(PersonDetail, {
        person: selectedPerson,
        onRefresh: () => { refreshPeople(); setSelectedPersonId(null); },
        onSelectSession: (id) => { setCurrentView('sessions'); setSelectedId(id); },
      });
    }
    return jsx(SessionDetail, {
      session: selectedSession,
      onRefresh: refresh,
      onDeleted: handleDeleted,
      onBack: handleBack,
      isMobile: isMobile,
      fields,
    });
  }

  if (isMobile) {
    if (currentView !== 'sessions') {
      return jsx('div', { className: 'h-full bg-gray-50 dark:bg-gray-950', children: mainContent() });
    }
    if (mobileView === 'detail' && selectedSession) {
      return jsx('div', { className: 'h-full bg-gray-50 dark:bg-gray-950', children: mainContent() });
    }
    return jsx('div', { className: 'h-full', children: jsx(Sidebar, sidebarProps) });
  }

  return jsxs('div', {
    className: 'h-full flex',
    children: [
      jsx('div', { className: 'w-72 flex-shrink-0 h-full', children: jsx(Sidebar, sidebarProps) }),
      jsx('div', { className: 'flex-1 h-full bg-gray-50 dark:bg-gray-950', children: mainContent() }),
    ],
  });
}

createRoot(document.getElementById('root')).render(jsx(App, {}));

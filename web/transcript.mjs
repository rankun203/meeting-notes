import { useState, useEffect, useRef } from 'react';
import { jsx, jsxs, Fragment, api, speakerColor, fmtTimestamp } from './utils.mjs';
import { SourceIcon } from './icons.mjs';

// ── Transcript Viewer ──

export function TranscriptViewer({ sessionId, onSeek, onSpeakerUpdate, currentTime }) {
  const [transcript, setTranscript] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [people, setPeople] = useState([]);
  const [editingSpeaker, setEditingSpeaker] = useState(null);
  const [newName, setNewName] = useState('');
  const [hiddenSpeakers, setHiddenSpeakers] = useState({});
  const containerRef = useRef(null);
  const activeRef = useRef(null);
  const userScrolledRef = useRef(false);
  const scrollTimeoutRef = useRef(null);

  function reload() {
    api(`/sessions/${sessionId}/transcript`)
      .then(data => { setTranscript(data); })
      .catch(() => {});
    api('/people').then(d => setPeople(d.people || [])).catch(() => {});
  }

  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      api(`/sessions/${sessionId}/transcript`),
      api('/people'),
    ]).then(([t, p]) => {
      setTranscript(t);
      setPeople(p.people || []);
      setLoading(false);
    }).catch(e => { setError(e.message); setLoading(false); });
  }, [sessionId]);

  async function assignSpeaker(speaker, action, personId, name) {
    try {
      await api(`/sessions/${sessionId}/attribution`, {
        method: 'POST',
        body: JSON.stringify({ attributions: [{ speaker, action, person_id: personId, name }] }),
      });
      setEditingSpeaker(null);
      setNewName('');
      reload();
      if (onSpeakerUpdate) onSpeakerUpdate();
    } catch (e) { alert(`Failed: ${e.message}`); }
  }

  const segments = (transcript && transcript.segments) || [];

  // Find all active segment indices — segments whose time range spans currentTime.
  // A segment is active if currentTime >= seg.start and (currentTime < next_seg.start or it's the last).
  const activeSet = new Set();
  if (currentTime != null && segments.length > 0) {
    for (let i = 0; i < segments.length; i++) {
      const seg = segments[i];
      if (seg.start == null) continue;
      const end = seg.end != null ? seg.end : (i + 1 < segments.length ? segments[i + 1].start : Infinity);
      if (seg.start <= currentTime && currentTime < end) {
        activeSet.add(i);
      }
    }
    // If nothing matched (e.g. gap between segments), highlight the last segment before currentTime
    if (activeSet.size === 0) {
      for (let i = segments.length - 1; i >= 0; i--) {
        if (segments[i].start != null && segments[i].start <= currentTime) {
          activeSet.add(i);
          break;
        }
      }
    }
  }
  const firstActiveIdx = activeSet.size > 0 ? Math.min(...activeSet) : -1;

  // Auto-scroll to first active segment (pause auto-scroll briefly when user scrolls manually)
  useEffect(() => {
    if (firstActiveIdx < 0 || !activeRef.current || !containerRef.current || userScrolledRef.current) return;
    const container = containerRef.current;
    const el = activeRef.current;
    const containerRect = container.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();

    // Scroll so active line is roughly in the top third of the container
    const targetOffset = containerRect.height * 0.3;
    const elRelativeTop = elRect.top - containerRect.top;
    if (Math.abs(elRelativeTop - targetOffset) > 40) {
      container.scrollTo({
        top: container.scrollTop + elRelativeTop - targetOffset,
        behavior: 'smooth',
      });
    }
  }, [firstActiveIdx]);

  // Detect manual scroll — pause auto-scroll for 4 seconds after user scrolls
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    function onScroll() {
      userScrolledRef.current = true;
      if (scrollTimeoutRef.current) clearTimeout(scrollTimeoutRef.current);
      scrollTimeoutRef.current = setTimeout(() => { userScrolledRef.current = false; }, 4000);
    }
    container.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      container.removeEventListener('scroll', onScroll);
      if (scrollTimeoutRef.current) clearTimeout(scrollTimeoutRef.current);
    };
  }, []);

  if (loading) return jsx('div', { className: 'text-sm text-gray-400 py-4 text-center', children: 'Loading transcript...' });
  if (error) return jsx('div', { className: 'text-sm text-red-500 py-4', children: `Error: ${error}` });
  if (segments.length === 0) {
    return jsx('div', { className: 'text-sm text-gray-400 py-4 text-center', children: 'No transcript segments' });
  }

  // Collect unique speakers for filter chips (with source_type from first occurrence)
  const uniqueSpeakers = [...new Map(segments.map(seg => {
    const key = seg.speaker || 'Unknown';
    return [key, {
      label: seg.person_name || seg.speaker || 'Unknown',
      sourceType: seg.source_type || null,
    }];
  })).entries()];

  function toggleSpeaker(speakerKey) {
    setHiddenSpeakers(prev => ({ ...prev, [speakerKey]: !prev[speakerKey] }));
  }

  return jsxs('div', { className: 'space-y-2', children: [
    // Speaker filter chips
    jsx('div', {
      className: 'flex flex-wrap gap-1.5',
      children: uniqueSpeakers.map(([key, { label, sourceType }]) => {
        const hidden = !!hiddenSpeakers[key];
        return jsxs('button', {
          key,
          onClick: () => toggleSpeaker(key),
          className: [
            'text-[10px] font-medium px-2 py-0.5 rounded-full transition-all inline-flex items-center gap-1',
            hidden
              ? 'opacity-40 line-through bg-gray-100 text-gray-400 dark:bg-gray-800 dark:text-gray-500'
              : speakerColor(key),
          ].join(' '),
          title: hidden ? `Show ${label}` : `Hide ${label}`,
          children: [
            jsx(SourceIcon, { sourceType, className: 'w-3 h-3 flex-shrink-0' }),
            label.length > 20 ? label.slice(0, 20) + '...' : label,
          ],
        });
      }),
    }),

    // Scrollable transcript
    jsx('div', {
      ref: containerRef,
      className: 'overflow-y-auto scroll-smooth',
      style: { maxHeight: '400px' },
      children: jsx('div', { className: 'space-y-0.5 py-2', children:
        segments.map((seg, i) => {
          const speakerKey = seg.speaker || 'Unknown';
          if (hiddenSpeakers[speakerKey]) return null;
          const speaker = seg.person_name || seg.speaker || 'Unknown';
        const isUnconfirmed = !seg.person_id && seg.speaker;
        const isEditing = editingSpeaker === `${i}-${seg.speaker}`;
        const isActive = activeSet.has(i);

        return jsxs('div', {
          key: i,
          ref: i === firstActiveIdx ? activeRef : undefined,
          className: [
            'flex items-start gap-2 py-2 px-2 -mx-1 rounded-lg cursor-pointer transition-all duration-200',
            isActive
              ? 'bg-blue-50 dark:bg-blue-900/20'
              : 'hover:bg-gray-50 dark:hover:bg-gray-800/30 opacity-60',
          ].join(' '),
          onClick: () => onSeek && onSeek(seg.start),
          children: [
            jsx('span', {
              className: [
                'text-[11px] font-mono w-12 flex-shrink-0 text-right pt-0.5',
                isActive
                  ? 'text-blue-600 dark:text-blue-400 font-semibold'
                  : 'text-gray-400 dark:text-gray-500',
              ].join(' '),
              children: fmtTimestamp(seg.start),
            }),
            jsx('div', {
              className: 'w-28 flex-shrink-0',
              children: isEditing
                ? jsxs('div', {
                    className: 'flex flex-wrap items-center gap-1',
                    onClick: e => e.stopPropagation(),
                    children: [
                      jsx('input', {
                        type: 'text', placeholder: 'New name...', value: newName, autoFocus: true,
                        onChange: e => setNewName(e.target.value),
                        onKeyDown: e => {
                          if (e.key === 'Enter' && newName.trim()) assignSpeaker(seg.speaker, 'create', null, newName.trim());
                          if (e.key === 'Escape') setEditingSpeaker(null);
                        },
                        className: 'text-[11px] px-1.5 py-0.5 rounded border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 w-24',
                      }),
                      newName.trim() && jsx('button', {
                        onClick: () => assignSpeaker(seg.speaker, 'create', null, newName.trim()),
                        className: 'text-[10px] px-1.5 py-0.5 rounded bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 hover:bg-blue-200',
                        children: 'Create',
                      }),
                      people.length > 0 && jsx('select', {
                        onChange: e => { if (e.target.value) assignSpeaker(seg.speaker, 'correct', e.target.value); },
                        className: 'text-[10px] px-1 py-0.5 rounded border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800',
                        children: [
                          jsx('option', { key: '', value: '', children: 'Assign...' }),
                          ...people.map(p => jsx('option', { key: p.id, value: p.id, children: p.name })),
                        ],
                      }),
                      jsx('button', {
                        onClick: () => setEditingSpeaker(null),
                        className: 'text-[10px] text-gray-400 hover:text-gray-600',
                        children: 'Cancel',
                      }),
                    ],
                  })
                : jsx('button', {
                    onClick: (e) => { e.stopPropagation(); setEditingSpeaker(`${i}-${seg.speaker}`); setNewName(''); },
                    className: `text-[10px] font-medium px-1.5 py-0.5 rounded-full cursor-pointer hover:ring-2 hover:ring-blue-400 transition-all truncate max-w-full ${speakerColor(seg.speaker)} ${isUnconfirmed ? 'border border-dashed border-current' : ''}`,
                    title: `Click to assign: ${seg.speaker || 'unknown'}`,
                    children: speaker.length > 15 ? speaker.slice(0, 15) + '...' : speaker,
                  }),
            }),
            jsx('span', {
              className: [
                'text-sm flex-1 min-w-0 transition-all duration-200',
                isActive
                  ? 'text-gray-900 dark:text-gray-100 font-medium'
                  : 'text-gray-500 dark:text-gray-400',
              ].join(' '),
              children: seg.text,
            }),
          ],
        });
      }).filter(Boolean),
    }),
  }),
  ]});
}

// ── Speaker Attribution Panel ──

export function SpeakerAttribution({ sessionId, transcript, onUpdate }) {
  const [people, setPeople] = useState([]);
  const [newNames, setNewNames] = useState({});
  const [busy, setBusy] = useState({});

  useEffect(() => {
    api('/people').then(d => setPeople(d.people || [])).catch(() => {});
  }, []);

  if (!transcript || !transcript.speaker_embeddings) return null;
  const speakers = Object.entries(transcript.speaker_embeddings);
  if (speakers.length === 0) return null;

  async function submitAttribution(speaker, action, personId, name) {
    setBusy(prev => ({ ...prev, [speaker]: true }));
    try {
      await api(`/sessions/${sessionId}/attribution`, {
        method: 'POST',
        body: JSON.stringify({ attributions: [{ speaker, action, person_id: personId, name }] }),
      });
      if (onUpdate) onUpdate();
      api('/people').then(d => setPeople(d.people || [])).catch(() => {});
    } catch (e) {
      alert(`Attribution failed: ${e.message}`);
    } finally {
      setBusy(prev => ({ ...prev, [speaker]: false }));
    }
  }

  return jsxs('div', { className: 'space-y-2', children: [
    jsx('p', { className: 'text-[11px] uppercase tracking-wider text-gray-400 dark:text-gray-500', children: 'Speaker Attribution' }),
    ...speakers.map(([speaker, info]) => {
      const matched = info.person_id != null;
      const confidence = info.confidence != null ? Math.round(info.confidence * 100) : null;
      const isBusy = busy[speaker];

      return jsx('div', {
        key: speaker,
        className: 'flex items-center gap-2 py-1.5 px-2 rounded-lg bg-gray-50 dark:bg-gray-800/40',
        children: jsxs(Fragment, { children: [
          jsx('span', {
            className: `text-[10px] font-medium px-1.5 py-0.5 rounded-full flex-shrink-0 ${speakerColor(speaker)}`,
            children: speaker,
          }),
          matched
            ? jsxs(Fragment, { children: [
                jsx('span', { className: 'text-sm text-gray-700 dark:text-gray-300 flex-1', children: info.person_name }),
                confidence != null && jsx('span', { className: 'text-[11px] text-gray-400', children: `${confidence}%` }),
                jsx('button', {
                  disabled: isBusy,
                  onClick: () => submitAttribution(speaker, 'confirm', info.person_id),
                  className: 'text-[11px] px-2 py-0.5 rounded bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400 hover:bg-emerald-200 dark:hover:bg-emerald-900/50 disabled:opacity-40',
                  children: 'Confirm',
                }),
                jsx('button', {
                  disabled: isBusy,
                  onClick: () => submitAttribution(speaker, 'reject'),
                  className: 'text-[11px] px-2 py-0.5 rounded bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400 hover:bg-red-200 dark:hover:bg-red-900/50 disabled:opacity-40',
                  children: 'Reject',
                }),
              ]})
            : jsxs(Fragment, { children: [
                jsx('span', { className: 'text-sm text-gray-400 dark:text-gray-500 italic', children: 'Unknown' }),
                jsx('input', {
                  type: 'text',
                  placeholder: 'Name...',
                  value: newNames[speaker] || '',
                  onChange: e => setNewNames(prev => ({ ...prev, [speaker]: e.target.value })),
                  className: 'text-xs px-2 py-1 rounded border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300 w-32',
                }),
                jsx('button', {
                  disabled: isBusy || !(newNames[speaker] || '').trim(),
                  onClick: () => submitAttribution(speaker, 'create', null, newNames[speaker].trim()),
                  className: 'text-[11px] px-2 py-0.5 rounded bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 hover:bg-blue-200 dark:hover:bg-blue-900/50 disabled:opacity-40',
                  children: 'Create',
                }),
                people.length > 0 && jsx('select', {
                  onChange: e => { if (e.target.value) submitAttribution(speaker, 'correct', e.target.value); },
                  className: 'text-[11px] px-1 py-0.5 rounded border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-gray-600 dark:text-gray-400',
                  children: [
                    jsx('option', { key: '', value: '', children: 'Assign...' }),
                    ...people.map(p => jsx('option', { key: p.id, value: p.id, children: p.name })),
                  ],
                }),
              ]}),
        ]}),
      });
    }),
  ]});
}

// ── Speaker Attribution Wrapper ──

export function SpeakerAttributionWrapper({ sessionId, onUpdate }) {
  const [transcript, setTranscript] = useState(null);
  useEffect(() => {
    api(`/sessions/${sessionId}/transcript`)
      .then(data => setTranscript(data))
      .catch(() => {});
  }, [sessionId]);

  function handleUpdate() {
    api(`/sessions/${sessionId}/transcript`)
      .then(data => setTranscript(data))
      .catch(() => {});
    if (onUpdate) onUpdate();
  }

  if (!transcript) return null;

  const hasEmbeddings = transcript.speaker_embeddings && Object.keys(transcript.speaker_embeddings).length > 0;
  const segmentSpeakers = !hasEmbeddings
    ? [...new Set((transcript.segments || []).map(s => s.speaker).filter(Boolean))]
    : [];

  if (!hasEmbeddings && segmentSpeakers.length === 0) {
    return jsx('div', {
      className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4 md:p-5',
      children: jsx('p', { className: 'text-sm text-gray-400 dark:text-gray-500',
        children: 'No speakers identified in this recording.',
      }),
    });
  }

  return jsx('div', {
    className: 'rounded-xl border border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-4 md:p-5',
    children: jsx(SpeakerAttribution, { sessionId, transcript, onUpdate: handleUpdate }),
  });
}

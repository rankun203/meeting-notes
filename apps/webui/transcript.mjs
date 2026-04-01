import { useState, useEffect, useRef } from 'react';
import { jsx, jsxs, Fragment, api, speakerColor, fmtTimestamp } from './utils.mjs';
import { SourceIcon } from './icons.mjs';
import { SearchableList } from './searchable-list.mjs';

// ── Transcript Viewer ──

export function TranscriptViewer({ sessionId, onSeek, onSpeakerUpdate, currentTime }) {
  const [transcript, setTranscript] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [people, setPeople] = useState([]);
  const [speakerPicker, setSpeakerPicker] = useState(null); // { anchorPoint, speaker }
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
      setSpeakerPicker(null);
      reload();
      if (onSpeakerUpdate) onSpeakerUpdate();
    } catch (e) { alert(`Failed: ${e.message}`); }
  }

  const segments = (transcript && transcript.segments) || [];

  // Find all active segment indices — segments whose time range spans currentTime.
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

  // Auto-scroll to first active segment
  useEffect(() => {
    if (firstActiveIdx < 0 || !activeRef.current || !containerRef.current || userScrolledRef.current) return;
    const container = containerRef.current;
    const el = activeRef.current;
    const containerRect = container.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();
    const targetOffset = containerRect.height * 0.3;
    const elRelativeTop = elRect.top - containerRect.top;
    if (Math.abs(elRelativeTop - targetOffset) > 40) {
      container.scrollTo({
        top: container.scrollTop + elRelativeTop - targetOffset,
        behavior: 'smooth',
      });
    }
  }, [firstActiveIdx]);

  // Detect manual scroll — pause auto-scroll for 4 seconds
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

  // Collect unique speakers for filter chips
  const speakersByLabel = new Map();
  for (const seg of segments) {
    const label = seg.person_name || seg.speaker || 'Unknown';
    const rawKey = seg.speaker || 'Unknown';
    if (!speakersByLabel.has(label)) {
      speakersByLabel.set(label, { keys: new Set(), sourceType: seg.source_type || null });
    }
    speakersByLabel.get(label).keys.add(rawKey);
  }
  const uniqueSpeakers = [...speakersByLabel.entries()];

  function toggleSpeaker(label) {
    const entry = speakersByLabel.get(label);
    if (!entry) return;
    const keys = [...entry.keys];
    const allHidden = keys.every(k => hiddenSpeakers[k]);
    setHiddenSpeakers(prev => {
      const next = { ...prev };
      for (const k of keys) next[k] = !allHidden;
      return next;
    });
  }

  function openSpeakerPicker(e, speaker) {
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    setSpeakerPicker({
      anchorPoint: { x: rect.left, y: rect.bottom },
      speaker,
    });
  }

  return jsxs('div', { className: 'space-y-2', children: [
    // Speaker filter chips
    jsx('div', {
      className: 'flex flex-wrap gap-1.5',
      children: uniqueSpeakers.map(([label, { keys, sourceType }]) => {
        const hidden = [...keys].every(k => hiddenSpeakers[k]);
        const colorKey = [...keys][0];
        return jsxs('button', {
          key: label,
          onClick: () => toggleSpeaker(label),
          className: [
            'text-[10px] font-medium px-2 py-0.5 rounded-full transition-all inline-flex items-center gap-1',
            hidden
              ? 'opacity-40 line-through bg-gray-100 text-gray-400 dark:bg-gray-800 dark:text-gray-500'
              : speakerColor(colorKey),
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
      children: jsx('div', {
        className: 'grid py-2 items-baseline',
        style: { gridTemplateColumns: 'auto auto 1fr' },
        children:
          segments.map((seg, i) => {
            const speakerKey = seg.speaker || 'Unknown';
            if (hiddenSpeakers[speakerKey]) return null;
            const speaker = seg.person_name || seg.speaker || 'Unknown';
            const isUnconfirmed = !seg.person_id && seg.speaker;
            const isActive = activeSet.has(i);

            const words = seg.words || [];
            const wordScores = words.map(w => w.score).filter(s => s != null);
            const avgScore = wordScores.length > 0
              ? (wordScores.reduce((a, b) => a + b, 0) / wordScores.length)
              : null;

            const tooltipParts = [
              `${fmtTimestamp(seg.start)} – ${fmtTimestamp(seg.end)}`,
              `Speaker: ${speaker} (${seg.speaker || 'unknown'})`,
              seg.track ? `Track: ${seg.track}` : null,
              seg.source_type ? `Source: ${seg.source_type}` : null,
              avgScore != null ? `Transcription score: ${Math.round(avgScore * 100)}%` : null,
              seg.attribution_confidence != null && seg.person_id ? `Attribution confidence: ${Math.round(seg.attribution_confidence * 100)}%` : null,
              seg.person_id ? `Person: ${seg.person_name || seg.person_id}` : 'Unconfirmed speaker',
              `Click to seek · Click badge to assign`,
            ].filter(Boolean).join('\n');

            return jsxs('div', {
              key: i,
              ref: i === firstActiveIdx ? activeRef : undefined,
              title: tooltipParts,
              className: [
                'grid col-span-3 items-baseline cursor-pointer rounded-lg transition-all duration-200',
                isActive
                  ? 'bg-blue-50 dark:bg-blue-900/20'
                  : 'hover:bg-gray-50 dark:hover:bg-gray-800/30 opacity-60',
              ].join(' '),
              style: { gridTemplateColumns: 'subgrid' },
              onClick: () => onSeek && onSeek(seg.start),
              children: [
                // Col 1: timestamp
                jsx('span', {
                  className: [
                    'text-[11px] font-mono text-right py-1.5 pl-2 pr-2',
                    isActive
                      ? 'text-blue-600 dark:text-blue-400 font-semibold'
                      : 'text-gray-400 dark:text-gray-500',
                  ].join(' '),
                  children: fmtTimestamp(seg.start),
                }),
                // Col 2: speaker badge
                jsx('div', {
                  className: 'py-1.5 pr-2',
                  children: jsx('button', {
                    onClick: (e) => openSpeakerPicker(e, seg.speaker),
                    className: `text-[10px] font-medium px-1.5 py-0.5 rounded-full cursor-pointer hover:ring-2 hover:ring-blue-400 transition-all whitespace-nowrap ${speakerColor(seg.speaker)} ${isUnconfirmed ? 'border border-dashed border-current' : ''}`,
                    children: speaker.length > 15 ? speaker.slice(0, 15) + '...' : speaker,
                  }),
                }),
                // Col 3: text
                jsx('span', {
                  className: [
                    'text-sm min-w-0 py-1.5 pr-2',
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

    // Speaker picker (SearchableList portal)
    speakerPicker && jsx(SearchableList, {
      items: people.map(p => ({ id: p.id, label: p.name })),
      anchorPoint: speakerPicker.anchorPoint,
      placeholder: 'Search or create person...',
      onSelect: (item) => assignSpeaker(speakerPicker.speaker, 'correct', item.id),
      onCreateAndSelect: (name) => assignSpeaker(speakerPicker.speaker, 'create', null, name),
      onClose: () => setSpeakerPicker(null),
    }),
  ]});
}

// ── Speaker Attribution Panel ──

export function SpeakerAttribution({ sessionId, transcript, onUpdate, onSelectPerson }) {
  const [people, setPeople] = useState([]);
  const [busy, setBusy] = useState({});
  const [picker, setPicker] = useState(null); // { anchorPoint, speaker }

  useEffect(() => {
    api('/people').then(d => setPeople(d.people || [])).catch(() => {});
  }, []);

  if (!transcript || !transcript.speaker_embeddings) return null;
  const speakers = Object.entries(transcript.speaker_embeddings);
  if (speakers.length === 0) return null;

  async function submitAttribution(speaker, action, personId, name) {
    setPicker(null);
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

  function openPicker(e, speaker) {
    const rect = e.currentTarget.getBoundingClientRect();
    setPicker({
      anchorPoint: { x: rect.left, y: rect.bottom },
      speaker,
    });
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
                jsx('button', {
                  onClick: () => onSelectPerson && onSelectPerson(info.person_id),
                  className: 'text-sm text-blue-600 dark:text-blue-400 hover:underline flex-1 text-left',
                  children: info.person_name,
                }),
                confidence != null && jsx('span', { className: 'text-[11px] text-gray-400', children: `${confidence}%` }),
                jsx('button', {
                  disabled: isBusy,
                  onClick: () => submitAttribution(speaker, 'confirm', info.person_id),
                  className: 'text-[11px] px-2 py-0.5 rounded bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400 hover:bg-emerald-200 dark:hover:bg-emerald-900/50 disabled:opacity-40',
                  children: 'Confirm',
                }),
                jsx('button', {
                  disabled: isBusy,
                  onClick: (e) => openPicker(e, speaker),
                  className: 'text-[11px] px-2 py-0.5 rounded bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-600 disabled:opacity-40',
                  children: 'Reassign',
                }),
                jsx('button', {
                  disabled: isBusy,
                  onClick: () => submitAttribution(speaker, 'reject'),
                  className: 'text-[11px] px-2 py-0.5 rounded bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400 hover:bg-red-200 dark:hover:bg-red-900/50 disabled:opacity-40',
                  children: 'Reject',
                }),
              ]})
            : jsxs(Fragment, { children: [
                jsx('span', { className: 'text-sm text-gray-400 dark:text-gray-500 italic flex-1', children: 'Unknown' }),
                jsx('button', {
                  disabled: isBusy,
                  onClick: (e) => openPicker(e, speaker),
                  className: 'text-[11px] px-2 py-0.5 rounded bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 hover:bg-blue-200 dark:hover:bg-blue-900/50 disabled:opacity-40',
                  children: 'Assign Person',
                }),
              ]}),
        ]}),
      });
    }),

    // Speaker picker (SearchableList portal)
    picker && jsx(SearchableList, {
      items: people.map(p => ({ id: p.id, label: p.name })),
      anchorPoint: picker.anchorPoint,
      placeholder: 'Search or create person...',
      onSelect: (item) => submitAttribution(picker.speaker, 'correct', item.id),
      onCreateAndSelect: (name) => submitAttribution(picker.speaker, 'create', null, name),
      onClose: () => setPicker(null),
    }),
  ]});
}

// ── Speaker Attribution Wrapper ──

export function SpeakerAttributionWrapper({ sessionId, onUpdate, onSelectPerson }) {
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
    children: jsx(SpeakerAttribution, { sessionId, transcript, onUpdate: handleUpdate, onSelectPerson }),
  });
}

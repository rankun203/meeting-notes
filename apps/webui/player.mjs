import { useState, useEffect, useRef, useImperativeHandle, forwardRef } from 'react';
import { jsx, jsxs, Fragment, API, fmtTime } from './utils.mjs';
import { SourceIcon, PlayIcon, PauseIcon, StopSquareIcon, FastForwardIcon } from './icons.mjs';

// ── Waveform Display ──

function WaveformTrack({ sessionId, file, duration, currentTime, muted, onSeek }) {
  const canvasRef = useRef(null);
  const containerRef = useRef(null);
  const [waveform, setWaveform] = useState(null);
  const [width, setWidth] = useState(0);

  // Fetch waveform data
  useEffect(() => {
    if (!sessionId || !file) return;
    fetch(`${API}/sessions/${sessionId}/waveform/${encodeURIComponent(file.name)}`)
      .then(r => r.ok ? r.json() : null)
      .then(data => { if (data) setWaveform(data); })
      .catch(() => {});
  }, [sessionId, file.name]);

  // Observe container width
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(entries => {
      for (const entry of entries) setWidth(Math.floor(entry.contentRect.width));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Draw waveform
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !waveform || !width || !duration) return;

    const dpr = window.devicePixelRatio || 1;
    const height = 48;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;

    const ctx = canvas.getContext('2d');
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const bins = waveform.data;
    const numBins = bins.length / 2; // alternating min, max
    const mid = height / 2;

    // Map bins to pixels
    const binsPerPx = numBins / width;

    // Waveform color
    const baseColor = muted ? 'rgba(156, 163, 175, 0.25)' : 'rgba(59, 130, 246, 0.5)';
    const playedColor = muted ? 'rgba(156, 163, 175, 0.35)' : 'rgba(59, 130, 246, 0.8)';
    const playedX = duration > 0 ? (currentTime / duration) * width : 0;

    for (let px = 0; px < width; px++) {
      const binStart = Math.floor(px * binsPerPx);
      const binEnd = Math.min(Math.ceil((px + 1) * binsPerPx), numBins);

      // Aggregate bins for this pixel: min of mins, max of maxes
      let minVal = 0, maxVal = 0;
      for (let b = binStart; b < binEnd; b++) {
        const mn = bins[b * 2];
        const mx = bins[b * 2 + 1];
        if (mn < minVal) minVal = mn;
        if (mx > maxVal) maxVal = mx;
      }

      // Scale to canvas height (values are -1..1)
      const top = mid - maxVal * mid;
      const bottom = mid - minVal * mid;
      const barHeight = Math.max(bottom - top, 1);

      ctx.fillStyle = px <= playedX ? playedColor : baseColor;
      ctx.fillRect(px, top, 1, barHeight);
    }

    // Playhead line
    if (currentTime > 0 && playedX > 0) {
      ctx.fillStyle = muted ? 'rgba(156, 163, 175, 0.6)' : 'rgba(37, 99, 235, 0.9)';
      ctx.fillRect(Math.round(playedX), 0, 1, height);
    }
  }, [waveform, width, duration, currentTime, muted]);

  function handleClick(e) {
    if (!duration || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const t = (x / rect.width) * duration;
    if (onSeek) onSeek(Math.max(0, Math.min(t, duration)));
  }

  return jsxs('div', {
    className: `cursor-pointer ${muted ? 'opacity-40' : ''}`,
    children: [
      jsx('span', {
        className: `text-[10px] font-medium px-1 ${muted ? 'text-gray-400 dark:text-gray-600 line-through' : 'text-gray-500 dark:text-gray-400'}`,
        children: file.label,
      }),
      jsx('div', {
        ref: containerRef,
        onClick: handleClick,
        className: 'relative rounded overflow-hidden',
        style: { height: '48px' },
        children: jsx('canvas', {
          ref: canvasRef,
          className: 'absolute inset-0',
        }),
      }),
    ],
  });
}

// ── Synced Audio Player ──

export const SyncedPlayer = forwardRef(function SyncedPlayer({ files, sessionId, onTimeUpdate }, ref) {
  const audioRefs = useRef([]);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [mutedTracks, setMutedTracks] = useState({});
  const [speed, setSpeed] = useState(1);
  const rafRef = useRef(null);
  const playingRef = useRef(false);

  const SPEED_STEPS = [1, 1.25, 1.5, 2, 4];

  // Expose seekTo and play for external callers (e.g. transcript click)
  useImperativeHandle(ref, () => ({
    seekTo,
    seekAndPlay(t) {
      seekTo(t);
      const audios = getAudios();
      if (audios.length > 0 && !playingRef.current) {
        audios.forEach(a => a.play());
        setPlaying(true);
        playingRef.current = true;
        rafRef.current = requestAnimationFrame(updateTime);
      }
    },
  }), []);

  // Keep refs array sized to files
  useEffect(() => {
    audioRefs.current = audioRefs.current.slice(0, files.length);
    setPlaying(false);
    playingRef.current = false;
    setCurrentTime(0);
    setDuration(0);
  }, [files.length, sessionId]);

  function getAudios() {
    return audioRefs.current.filter(Boolean);
  }

  function applySpeed(rate) {
    getAudios().forEach(a => { a.playbackRate = rate; });
  }

  function cycleSpeed() {
    const idx = SPEED_STEPS.indexOf(speed);
    const next = SPEED_STEPS[(idx + 1) % SPEED_STEPS.length];
    setSpeed(next);
    applySpeed(next);
  }

  function onLoadedMetadata() {
    const maxDur = Math.max(...getAudios().map(a => a.duration || 0));
    if (maxDur > 0) setDuration(maxDur);
  }

  function updateTime() {
    const audios = getAudios();
    if (audios.length > 0) {
      const t = audios[0].currentTime || 0;
      setCurrentTime(t);
      if (onTimeUpdate) onTimeUpdate(t);
    }
    if (playingRef.current) rafRef.current = requestAnimationFrame(updateTime);
  }

  function togglePlay() {
    const audios = getAudios();
    if (playing) {
      audios.forEach(a => { a.pause(); a.playbackRate = 1; });
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      setPlaying(false);
      playingRef.current = false;
      setSpeed(1);
      if (onTimeUpdate) onTimeUpdate(audios[0]?.currentTime || 0);
    } else {
      const t = audios[0]?.currentTime || 0;
      audios.forEach(a => { a.playbackRate = speed; a.currentTime = t; a.play(); });
      setPlaying(true);
      playingRef.current = true;
      rafRef.current = requestAnimationFrame(updateTime);
    }
  }

  function seekTo(t) {
    setCurrentTime(t);
    getAudios().forEach(a => { a.currentTime = t; });
    if (onTimeUpdate) onTimeUpdate(t);
  }

  function onSeekInput(e) {
    seekTo(parseFloat(e.target.value));
  }

  function stopAll() {
    const audios = getAudios();
    audios.forEach(a => { a.pause(); a.currentTime = 0; a.playbackRate = 1; });
    if (rafRef.current) cancelAnimationFrame(rafRef.current);
    setPlaying(false);
    playingRef.current = false;
    setCurrentTime(0);
    setSpeed(1);
    if (onTimeUpdate) onTimeUpdate(0);
  }

  function toggleMute(idx) {
    setMutedTracks(prev => {
      const next = { ...prev, [idx]: !prev[idx] };
      if (audioRefs.current[idx]) audioRefs.current[idx].muted = next[idx];
      return next;
    });
  }

  function onEnded() {
    const audios = getAudios();
    if (audios.every(a => a.ended || a.currentTime >= a.duration - 0.1)) {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      audios.forEach(a => { a.pause(); a.currentTime = 0; });
      setPlaying(false);
      playingRef.current = false;
      setCurrentTime(0);
    }
  }

  useEffect(() => {
    return () => { if (rafRef.current) cancelAnimationFrame(rafRef.current); };
  }, []);

  return jsxs('div', { className: 'space-y-2', children: [
    // Hidden audio elements
    ...files.map((f, i) => {
      const src = `${API}/sessions/${sessionId}/files/${encodeURIComponent(f.name)}`;
      return jsx('audio', {
        key: f.name,
        ref: el => { audioRefs.current[i] = el; },
        src,
        preload: 'metadata',
        muted: !!mutedTracks[i],
        onLoadedMetadata,
        onEnded,
        className: 'hidden',
      });
    }),

    // Waveform tracks (layered, one per file)
    jsx('div', {
      className: 'space-y-0.5 rounded-lg overflow-hidden bg-gray-50 dark:bg-gray-800/30 p-1',
      children: files.map((f, i) =>
        jsx(WaveformTrack, {
          key: f.name,
          sessionId,
          file: f,
          duration,
          currentTime,
          muted: !!mutedTracks[i],
          onSeek: seekTo,
        })
      ),
    }),

    // Controls bar
    jsxs('div', { className: 'flex items-center gap-3', children: [
      // Play/Pause button
      jsx('button', {
        onClick: togglePlay,
        className: 'w-9 h-9 flex items-center justify-center rounded-full bg-blue-600 hover:bg-blue-700 text-white transition-colors flex-shrink-0',
        title: playing ? 'Pause all' : 'Play all',
        children: playing
          ? jsx(PauseIcon, {})
          : jsx('span', { className: 'ml-0.5', children: jsx(PlayIcon, {}) }),
      }),
      // Speed button
      jsx('button', {
        onClick: cycleSpeed,
        className: 'relative w-7 h-7 flex items-center justify-center rounded-full text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors flex-shrink-0',
        title: `Playback speed: ${speed}x (click to change)`,
        children: jsxs(Fragment, { children: [
          jsx(FastForwardIcon, { className: 'w-3.5 h-3.5' }),
          speed !== 1 && jsx('span', {
            className: 'absolute -bottom-0.5 -right-0.5 text-[8px] font-bold text-blue-600 dark:text-blue-400 leading-none',
            children: `${speed}x`,
          }),
        ]}),
      }),
      // Stop button
      jsx('button', {
        onClick: stopAll,
        className: 'w-7 h-7 flex items-center justify-center rounded-full text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors flex-shrink-0',
        title: 'Stop and reset',
        children: jsx(StopSquareIcon, { className: 'w-3.5 h-3.5' }),
      }),
      // Time
      jsx('span', { className: 'text-xs font-mono text-gray-500 dark:text-gray-400 w-10 text-right flex-shrink-0', children: fmtTime(currentTime) }),
      // Seek bar
      jsx('input', {
        type: 'range', min: 0, max: duration || 1, step: 0.1, value: currentTime,
        onInput: onSeekInput,
        className: 'flex-1 h-1.5 rounded-full appearance-none bg-gray-200 dark:bg-gray-700 cursor-pointer accent-blue-600',
      }),
      jsx('span', { className: 'text-xs font-mono text-gray-500 dark:text-gray-400 w-10 flex-shrink-0', children: fmtTime(duration) }),
    ]}),

    // Track list with mute toggles
    jsx('div', { className: 'space-y-1', children:
      files.map((f, i) => jsx('button', {
        key: f.name,
        onClick: () => toggleMute(i),
        title: mutedTracks[i] ? `Unmute ${f.label}` : `Mute ${f.label}`,
        className: `flex items-center gap-1 px-1 py-0.5 w-full rounded transition-colors cursor-pointer ${mutedTracks[i] ? 'text-gray-300 dark:text-gray-600' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'}`,
        children: jsxs(Fragment, { children: [
          jsx('span', { className: 'w-6 h-6 flex items-center justify-center flex-shrink-0', children:
            jsx(SourceIcon, { sourceType: f.sourceType, className: 'w-4 h-4' }),
          }),
          jsx('span', { className: `text-xs capitalize flex-1 text-left ${mutedTracks[i] ? 'text-gray-300 dark:text-gray-600 line-through' : 'text-gray-600 dark:text-gray-400'}`, children: f.label }),
        ]}),
      })),
    }),
  ]});
});

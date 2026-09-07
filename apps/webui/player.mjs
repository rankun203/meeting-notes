import { track } from './analytics.mjs';
import { useState, useEffect, useRef, useImperativeHandle, forwardRef } from 'react';
import { jsx, jsxs, API, fmtTime } from './utils.mjs';
import { PlayIcon, PauseIcon } from './icons.mjs';

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
    const baseColor = muted ? 'rgba(156, 163, 175, 0.25)' : 'rgba(131, 172, 164, 0.55)';
    const playedColor = muted ? 'rgba(156, 163, 175, 0.35)' : 'rgba(198, 244, 141, 0.95)';
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
      ctx.fillStyle = muted ? 'rgba(156, 163, 175, 0.6)' : 'rgba(198, 244, 141, 1)';
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
  const [error, setError] = useState('');
  const [overviewTrack, setOverviewTrack] = useState(0);
  const playingRef = useRef(false);
  const mountedRef = useRef(true);
  const lastUpdateRef = useRef(0);
  const timeRef = useRef(0);
  const pendingPlayRef = useRef(0);
  const getAudios = () => audioRefs.current.filter(Boolean);
  const clockAudio = () => getAudios().reduce((a,b) => (a?.duration || 0) > (b.duration || 0) ? a : b, null);

  function updateTime(force = false) {
    if (!force && performance.now() - lastUpdateRef.current < 200) return;
    lastUpdateRef.current = performance.now();
    const t = clockAudio()?.currentTime || 0;
    timeRef.current = t;
    setCurrentTime(t);
    onTimeUpdate?.(t);
  }

  function pause() {
    pendingPlayRef.current++;
    playingRef.current = false;
    getAudios().forEach(a => a.pause());
    setPlaying(false);
    updateTime(true);
  }

  async function play() {
    const audios = getAudios();
    if (!audios.length) return;
    const attempt = ++pendingPlayRef.current;
    setError('');
    const t = duration > 0 && timeRef.current >= duration ? 0 : timeRef.current;
    try {
      await Promise.all(audios.map(a => {
        a.playbackRate = speed;
        a.currentTime = Math.min(t, Number.isFinite(a.duration) ? a.duration : t);
        if (Number.isFinite(a.duration) && t >= a.duration) { a.pause(); return; }
        return a.play();
      }));
      if (!mountedRef.current || attempt !== pendingPlayRef.current) return;
      playingRef.current = true;
      setPlaying(true);
      track('playback_started', { source:'player', position_seconds:t, duration_seconds:duration });
    } catch {
      if (!mountedRef.current || attempt !== pendingPlayRef.current) return;
      pause();
      setError('Audio could not play. Check your output device and try again.');
    }
  }

  function seekTo(value) {
    const t = Math.max(0, Math.min(Number.isFinite(value) ? value : 0, duration || Infinity));
    getAudios().forEach(a => {
      a.currentTime = Math.min(t, Number.isFinite(a.duration) ? a.duration : t);
      if (Number.isFinite(a.duration) && t >= a.duration) a.pause();
      else if (playingRef.current && a.paused) void a.play().catch(() => { pause(); setError('Audio could not resume. Please try again.'); });
    });
    timeRef.current = t;
    setCurrentTime(t);
    onTimeUpdate?.(t);
  }

  useImperativeHandle(ref, () => ({
    seekTo,
    seekAndPlay(t) { track('playback_seeked', { source:'link', position_seconds:t }); seekTo(t); if (!playingRef.current) void play(); },
  }));

  useEffect(() => {
    mountedRef.current = true;
    const audios = getAudios();
    return () => { mountedRef.current = false; pendingPlayRef.current++; playingRef.current = false; audios.forEach(a => a.pause()); };
  }, [sessionId]);

  function onEnded() {
    const audios = getAudios();
    if (audios.every(a => a.ended || a.currentTime >= a.duration - 0.1)) {
      track('playback_completed', { duration_seconds:duration });
      pause(); seekTo(0);
    }
  }

  function togglePlay() {
    if (playingRef.current) { track('playback_paused', { position_seconds:currentTime }); pause(); }
    else void play();
  }

  function skip(seconds) {
    const position = Math.max(0, Math.min(duration, currentTime + seconds));
    seekTo(position);
    track('playback_seeked', { source:'player', position_seconds:position });
  }

  return jsxs('div', { className:'synced-player', children:[
    ...files.map((f,i) => jsx('audio', {
      key:f.name, ref:el => { audioRefs.current[i] = el; },
      src:`${API}/sessions/${sessionId}/files/${encodeURIComponent(f.name)}`,
      preload:'metadata', muted:!!mutedTracks[i],
      onLoadedMetadata:() => { const max = Math.max(...getAudios().map(a => Number.isFinite(a.duration) ? a.duration : 0)); setDuration(max); setOverviewTrack(Math.max(0, audioRefs.current.indexOf(clockAudio()))); },
      onTimeUpdate:() => updateTime(), onEnded,
      onPause:() => { if (playingRef.current && getAudios().every(a => a.paused)) { playingRef.current = false; setPlaying(false); updateTime(true); } },
      className:'hidden',
    })),
    jsxs('div', { className:'player-transport', children:[
      jsx('button', { className:'skip-button', onClick:() => skip(-15), title:'Back 15 seconds', 'aria-label':'Back 15 seconds', children:'−15' }),
      jsx('button', { className:'main-play-button', onClick:togglePlay, 'aria-label':playing?'Pause meeting':'Play meeting', title:playing?'Pause meeting':'Play meeting', children:jsx(playing?PauseIcon:PlayIcon,{}) }),
      jsx('button', { className:'skip-button', onClick:() => skip(15), title:'Forward 15 seconds', 'aria-label':'Forward 15 seconds', children:'+15' }),
    ]}),
    jsxs('div', { className:'player-timeline', children:[
      files[overviewTrack] && jsx(WaveformTrack, { sessionId, file:files[overviewTrack], duration, currentTime, muted:!!mutedTracks[overviewTrack], onSeek:t => { seekTo(t); track('playback_seeked',{source:'waveform',position_seconds:t}); } }),
      jsxs('div', { className:'player-progress', children:[
        jsx('span', { children:fmtTime(currentTime) }),
        jsx('input', { type:'range', min:0, max:duration || 1, step:0.1, value:Math.min(currentTime,duration || 1), disabled:!duration, 'aria-label':'Playback position', 'aria-valuetext':`${fmtTime(currentTime)} of ${fmtTime(duration)}`, onChange:e => seekTo(Number(e.target.value)), onPointerUp:e => track('playback_seeked',{source:'player',position_seconds:Number(e.currentTarget.value)}), onKeyUp:e => { if (['ArrowLeft','ArrowRight','Home','End'].includes(e.key)) track('playback_seeked',{source:'player',position_seconds:Number(e.currentTarget.value)}); } }),
        jsx('span', { children:fmtTime(duration) }),
      ]}),
    ]}),
    jsxs('div', { className:'player-options', children:[
      jsx('select', { value:speed, 'aria-label':'Playback speed', onChange:e => { const rate=Number(e.target.value); setSpeed(rate); getAudios().forEach(a => {a.playbackRate=rate;}); track('playback_speed_changed',{speed:rate}); }, children:[0.75,1,1.25,1.5,2,4].map(rate => jsx('option',{key:rate,value:rate,children:`${rate}×`})) }),
      jsxs('details', { className:'track-menu', children:[
        jsx('summary', { children:`${files.length} track${files.length===1?'':'s'}` }),
        jsxs('div', { className:'track-popover', children:[
          jsx('p',{children:'Audio sources'}),
          ...files.map((file,i) => jsxs('label',{key:file.name,children:[
            jsx('input',{type:'checkbox',checked:!mutedTracks[i],onChange:e => { const muted=!e.target.checked; setMutedTracks(prev=>({...prev,[i]:muted})); if(audioRefs.current[i]) audioRefs.current[i].muted=muted; track('playback_track_toggled'); }}), file.label,
          ]})),
          jsx('button',{onClick:() => {pause();seekTo(0);},children:'Stop & reset'}),
        ]}),
      ]}),
    ]}),
    error && jsx('p',{className:'player-error',role:'alert',children:error}),
  ]});
});

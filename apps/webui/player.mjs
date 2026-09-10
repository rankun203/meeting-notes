import { track } from './analytics.mjs';
import { mergeWaveformPeaks } from './waveform.mjs';
import { useState, useEffect, useMemo, useRef, useImperativeHandle, forwardRef } from 'react';
import { jsx, jsxs, API, fmtTime } from './utils.mjs';
import { PlayIcon, PauseIcon } from './icons.mjs';

// One waveform and one native seek target for every audible source.
function WaveformSeek({ sessionId, files, mutedTracks, duration, currentTime, onSeek }) {
  const canvasRef = useRef(null);
  const containerRef = useRef(null);
  const [waveforms, setWaveforms] = useState({});
  const [loaded, setLoaded] = useState(false);
  const [width, setWidth] = useState(0);
  const namesKey = JSON.stringify(files.map(file => file.name));

  useEffect(() => {
    const controller = new AbortController();
    setWaveforms({});
    setLoaded(false);
    void Promise.all(JSON.parse(namesKey).map(async name => {
      try {
        const response = await fetch(`${API}/sessions/${sessionId}/waveform/${encodeURIComponent(name)}`, { signal: controller.signal });
        return [name, response.ok ? await response.json() : null];
      } catch { return [name, null]; }
    })).then(entries => {
      if (controller.signal.aborted) return;
      setWaveforms(Object.fromEntries(entries));
      setLoaded(true);
    });
    return () => controller.abort();
  }, [sessionId, namesKey]);

  useEffect(() => {
    const observer = new ResizeObserver(entries => setWidth(Math.floor(entries[0].contentRect.width)));
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  const peaks = useMemo(() => mergeWaveformPeaks(
    JSON.parse(namesKey).filter((_, i) => !mutedTracks[i]).map(name => waveforms[name]),
    duration, Math.ceil(width / 3),
  ), [waveforms, namesKey, mutedTracks, duration, width]);
  const state = !loaded ? 'loading' : !Object.values(waveforms).some(w => w?.data?.length) ? 'unavailable' : peaks.some(value => value !== 0) ? 'ready' : 'silent';

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !width) return;
    const dpr = window.devicePixelRatio || 1;
    const height = 38;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);
    const playedX = Math.max(0, Math.min(width, duration ? currentTime / duration * width : 0));
    const unplayed = '#7f9e90', played = '#d2ef9c';
    // Silence and failed waveform requests still leave a useful single timeline.
    ctx.fillStyle = unplayed;
    ctx.fillRect(0, height / 2, width, 1);
    ctx.fillStyle = played;
    ctx.fillRect(0, height / 2, playedX, 1);
    for (let col = 0; col < peaks.length / 2; col++) {
      const x = col * 3;
      const top = height / 2 - peaks[col * 2 + 1] * (height / 2 - 3);
      const bottom = height / 2 - peaks[col * 2] * (height / 2 - 3);
      ctx.fillStyle = x <= playedX ? played : unplayed;
      ctx.fillRect(x, top, 2, Math.max(1, bottom - top));
    }
    // The sole visible playhead belongs to the waveform, not a second progress bar.
    ctx.fillStyle = played;
    ctx.fillRect(Math.min(width - 2, playedX), 0, 2, height);
  }, [peaks, width, duration, currentTime]);

  return jsxs('div', {
    ref: containerRef, className: 'waveform-seek', 'data-waveform-state': state,
    title: state === 'loading' ? 'Loading waveform — seeking is available' : state === 'unavailable' ? 'Waveform unavailable — seeking is available' : state === 'silent' ? 'Silent or muted audio — seek to a position' : 'Combined audio waveform — drag to seek',
    children: [
      jsx('canvas', { ref: canvasRef, 'aria-hidden': true }),
      jsx('input', {
        type: 'range', min: 0, max: duration || 1, step: 0.1,
        value: Math.min(currentTime, duration || 1), disabled: !duration,
        'aria-label': 'Playback position', 'aria-valuetext': `${fmtTime(currentTime)} of ${fmtTime(duration)}`,
        onChange: e => onSeek(Number(e.target.value)),
        onPointerUp: e => track('playback_seeked', { source: 'waveform', position_seconds: Number(e.currentTarget.value) }),
        onKeyUp: e => { if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) track('playback_seeked', { source: 'waveform', position_seconds: Number(e.currentTarget.value) }); },
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
      onLoadedMetadata:() => { const max = Math.max(...getAudios().map(a => Number.isFinite(a.duration) ? a.duration : 0)); setDuration(max); },
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
      jsx('span', { className:'player-time', children:fmtTime(currentTime) }),
      jsx(WaveformSeek, { sessionId, files, mutedTracks, duration, currentTime, onSeek:seekTo }),
      jsx('span', { className:'player-time', children:fmtTime(duration) }),
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

import { jsx } from './utils.mjs';

export function ChevronIcon({ open }) {
  return jsx('svg', {
    xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
    className: `w-3.5 h-3.5 transition-transform ${open ? 'rotate-90' : ''}`,
    children: jsx('path', {
      fillRule: 'evenodd', clipRule: 'evenodd',
      d: 'M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z',
    }),
  });
}

export const PlusIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { d: 'M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z' }),
});

export const CloseIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { d: 'M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z' }),
});

export const MenuIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-5 h-5',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zm0 5A.75.75 0 012.75 9h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 9.75zm0 5a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75z',
  }),
});

export const BackIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-5 h-5',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M17 10a.75.75 0 01-.75.75H5.612l4.158 3.96a.75.75 0 11-1.04 1.08l-5.5-5.25a.75.75 0 010-1.08l5.5-5.25a.75.75 0 011.04 1.08L5.612 9.25H16.25A.75.75 0 0117 10z',
  }),
});

export const PlayIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { d: 'M6.3 2.841A1.5 1.5 0 004 4.11V15.89a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z' }),
});

export const StopIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M2 10a8 8 0 1116 0 8 8 0 01-16 0zm5-2.25A.75.75 0 017.75 7h4.5a.75.75 0 01.75.75v4.5a.75.75 0 01-.75.75h-4.5a.75.75 0 01-.75-.75v-4.5z',
  }),
});

// Microphone icon (for mic sources)
export const MicIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 16 16', fill: 'currentColor',
  className: className || 'w-3 h-3',
  children: jsx('path', { d: 'M8 1a3.25 3.25 0 00-3.25 3.25v3.5a3.25 3.25 0 006.5 0v-3.5A3.25 3.25 0 008 1zM5 8.75a.75.75 0 00-1.5 0A4.5 4.5 0 007.25 13.2v1.05h-1.5a.75.75 0 000 1.5h4.5a.75.75 0 000-1.5h-1.5V13.2A4.5 4.5 0 0012.5 8.75a.75.75 0 00-1.5 0 3 3 0 01-6 0z' }),
});

// Speaker/audio output icon (for system_mix sources)
export const SpeakerIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 16 16', fill: 'currentColor',
  className: className || 'w-3 h-3',
  children: jsx('path', { d: 'M7.557 2.066A.75.75 0 018.5 2.75v10.5a.75.75 0 01-1.264.546L4.203 11H2.75A.75.75 0 012 10.25v-4.5A.75.75 0 012.75 5h1.453l3.033-2.796a.75.75 0 01.321-.138zM10.78 4.22a.75.75 0 011.06 0 5.5 5.5 0 010 7.78.75.75 0 01-1.06-1.06 4 4 0 000-5.66.75.75 0 010-1.06z' }),
});

// Record circle icon
export const RecordIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 24 24', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('circle', { cx: '12', cy: '12', r: '8' }),
});

// Pause icon
export const PauseIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { d: 'M5.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75A.75.75 0 007.25 3h-1.5zM12.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75a.75.75 0 00-.75-.75h-1.5z' }),
});

// Square stop icon (no circle)
export const StopSquareIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
  className: className || 'w-3.5 h-3.5',
  children: jsx('rect', { x: '4', y: '4', width: '12', height: '12', rx: '1' }),
});

// Fast-forward icon (double triangle >>)
export const FastForwardIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
  className: className || 'w-3.5 h-3.5',
  children: jsx('path', { d: 'M3 4.5a.5.5 0 01.832-.374l5.336 4.715a.75.75 0 010 1.024L3.832 14.78A.5.5 0 013 14.406V4.5zm7 0a.5.5 0 01.832-.374l5.336 4.715a.75.75 0 010 1.024l-5.336 4.914A.5.5 0 0110 14.406V4.5z' }),
});

// Transcript/lines icon
export const TranscriptIcon = () => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor', className: 'w-4 h-4',
  children: jsx('path', { fillRule: 'evenodd', clipRule: 'evenodd', d: 'M2 3.75A.75.75 0 012.75 3h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 3.75zm0 4.167a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75zm0 4.166a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75zm0 4.167a.75.75 0 01.75-.75h7.5a.75.75 0 010 1.5h-7.5a.75.75 0 01-.75-.75z' }),
});

// Returns the appropriate source icon component for a given source_type
export function SourceIcon({ sourceType, className }) {
  if (sourceType === 'mic') return jsx(MicIcon, { className });
  if (sourceType === 'system_mix') return jsx(SpeakerIcon, { className });
  return jsx(MicIcon, { className });
}

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

// Tag icon (45-degree rotated tag shape)
export const TagIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
  className: className || 'w-3 h-3',
  style: { transform: 'rotate(-45deg)' },
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M5.5 3A2.5 2.5 0 003 5.5v2.879a2.5 2.5 0 00.732 1.767l6.5 6.5a2.5 2.5 0 003.536 0l2.878-2.878a2.5 2.5 0 000-3.536l-6.5-6.5A2.5 2.5 0 008.38 3H5.5zM6 7a1 1 0 100-2 1 1 0 000 2z',
  }),
});

// Chat bubble icon (speech bubble)
export const ChatIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 24 24', fill: 'currentColor',
  className: className || 'w-6 h-6',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M4.804 21.644A6.707 6.707 0 006 21.75a6.721 6.721 0 003.583-1.029c.774.182 1.584.279 2.417.279 5.322 0 9.75-3.97 9.75-8.5S17.322 4 12 4s-9.75 3.97-9.75 8.5c0 2.012.738 3.87 1.985 5.37a.75.75 0 01.088.585l-.573 2.189zM6 24a9.204 9.204 0 01-2.746-.414l.87-3.328C2.825 18.476 1.5 16.11 1.5 12.5 1.5 7.253 6.203 3 12 3s10.5 4.253 10.5 9.5S17.797 22 12 22c-.905 0-1.786-.1-2.63-.29A9.218 9.218 0 016 24z',
  }),
});

// Send icon (paper plane)
export const SendIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
  className: className || 'w-4 h-4',
  children: jsx('path', {
    d: 'M3.105 2.289a.75.75 0 00-.826.95l1.414 4.925A1.5 1.5 0 005.135 9.25h6.115a.75.75 0 010 1.5H5.135a1.5 1.5 0 00-1.442 1.086l-1.414 4.926a.75.75 0 00.826.95 28.896 28.896 0 0015.293-7.154.75.75 0 000-1.115A28.897 28.897 0 003.105 2.289z',
  }),
});

// Minimize icon (chevron down)
export const MinimizeIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
  className: className || 'w-4 h-4',
  children: jsx('path', {
    fillRule: 'evenodd', clipRule: 'evenodd',
    d: 'M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z',
  }),
});

// Sparkle/agent icon
export const SparkleIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
  className: className || 'w-4 h-4',
  children: jsx('path', {
    d: 'M10 1a.75.75 0 01.75.75v1.5a.75.75 0 01-1.5 0v-1.5A.75.75 0 0110 1zM5.05 3.05a.75.75 0 011.06 0l1.062 1.06a.75.75 0 11-1.06 1.06L5.05 4.11a.75.75 0 010-1.06zm9.9 0a.75.75 0 010 1.06l-1.06 1.06a.75.75 0 01-1.06-1.06l1.06-1.06a.75.75 0 011.06 0zM10 7a3 3 0 100 6 3 3 0 000-6zm-6.25 3a.75.75 0 01-.75-.75h-1.5a.75.75 0 010 1.5h1.5A.75.75 0 013.75 10zm14.5 0a.75.75 0 01-.75.75h-1.5a.75.75 0 010-1.5h1.5a.75.75 0 01.75.75zm-12.14 3.89a.75.75 0 010 1.06l-1.06 1.06a.75.75 0 01-1.06-1.06l1.06-1.06a.75.75 0 011.06 0zm8.78 0a.75.75 0 011.06 0l1.06 1.06a.75.75 0 01-1.06 1.06l-1.06-1.06a.75.75 0 010-1.06zM10 16a.75.75 0 01.75.75v1.5a.75.75 0 01-1.5 0v-1.5A.75.75 0 0110 16z',
  }),
});

// New conversation icon (pencil + square)
export const NewChatIcon = ({ className }) => jsx('svg', {
  xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 20 20', fill: 'currentColor',
  className: className || 'w-4 h-4',
  children: jsx('path', {
    d: 'M5.433 13.917l1.262-3.155A4 4 0 017.58 9.42l6.92-6.918a2.121 2.121 0 013 3l-6.92 6.918c-.383.383-.84.685-1.343.886l-3.154 1.262a.5.5 0 01-.65-.65z',
  }),
});

// Returns the appropriate source icon component for a given source_type
export function SourceIcon({ sourceType, className }) {
  if (sourceType === 'mic') return jsx(MicIcon, { className });
  if (sourceType === 'system_mix') return jsx(SpeakerIcon, { className });
  return jsx(MicIcon, { className });
}

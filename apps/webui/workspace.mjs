import { useEffect, useRef } from 'react';
import {
  jsx,
  jsxs,
  API,
  formatFileSize,
  PlayIcon,
  CloseIcon,
} from './utils.mjs';
import { NewSessionPanel } from './session.mjs';

export function Glyph({ name, size = 18 }) {
  const paths = {
    library: 'M4 4h6v16H4z M14 4h6v16h-6z M7 8v5 M17 8v5',
    people:
      'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8 M20 21v-2a4 4 0 0 0-3-3.87 M16 3.13a4 4 0 0 1 0 7.75',
    settings: 'M4 7h16 M4 17h16 M8 4v6 M16 14v6',
    search: 'M21 21l-5-5 M18 10a8 8 0 1 0-16 0 8 8 0 0 0 16 0',
    file: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M8 13h8 M8 17h5',
    download: 'M12 3v12 M7 10l5 5 5-5 M5 16v5h14v-5',
    sound: 'M4 10v4 M8 5v14 M12 2v20 M16 7v10 M20 10v4',
    arrow: 'M4 12h16 M14 6l6 6-6 6',
    spark: 'M12 2l3 7 7 3-7 3-3 7-3-7-7-3 7-3z',
  };
  return jsx('svg', {
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.6,
    strokeLinecap: 'round',
    strokeLinejoin: 'round',
    'aria-hidden': true,
    children: jsx('path', { d: paths[name] || paths.file }),
  });
}

export function RecordingDialog({
  sources,
  fields,
  onCreated,
  onSelect,
  onClose,
}) {
  const ref = useRef(null);
  useEffect(() => {
    const dialog = ref.current;
    dialog.showModal();
    return () => dialog.close();
  }, []);
  return jsx('dialog', {
    ref,
    className: 'record-dialog',
    onCancel: onClose,
    onClick: (e) => {
      if (e.target === e.currentTarget) onClose();
    },
    children: jsxs('div', {
      className: 'record-dialog-body',
      children: [
        jsxs('div', {
          className: 'section-heading',
          children: [
            jsx('span', {
              className: 'eyebrow',
              children: 'CAPTURE THE CONVERSATION',
            }),
            jsx('button', {
              'aria-label': 'Close recording setup',
              onClick: onClose,
              className: 'icon-button',
              children: jsx(CloseIcon, {}),
            }),
          ],
        }),
        jsx('h2', { children: 'Record or import' }),
        jsx('p', {
          className: 'muted',
          children:
            'Record your next meeting or bring in an existing recording.',
        }),
        jsx(NewSessionPanel, { sources, fields, onCreated, onSelect }),
      ],
    }),
  });
}

export function FilesPanel({ session, onPlay }) {
  const files = session.files || [];
  const supporting = files.filter(
    (name) =>
      ['metadata.json', 'summary.json', 'extraction_raw.json'].includes(name) ||
      name.endsWith('.waveform.json'),
  );
  const primary = files
    .filter((name) => !supporting.includes(name))
    .sort(
      (a, b) =>
        Number(/\.(mp3|wav|opus)$/i.test(b)) -
        Number(/\.(mp3|wav|opus)$/i.test(a)),
    );
  const fileRow = (name) => {
    const audio = /\.(mp3|wav|opus)$/i.test(name);
    const ext = name.split('.').pop().toUpperCase();
    return jsxs('div', {
      className: 'file-row',
      key: name,
      children: [
        jsx('span', {
          className: `file-type ${audio ? 'is-audio' : ''}`,
          children: jsx(Glyph, { name: audio ? 'sound' : 'file', size: 17 }),
        }),
        jsxs('div', {
          className: 'file-description',
          children: [
            jsx('span', {
              className: 'file-name',
              title: name,
              children: name,
            }),
            jsx('span', {
              className: 'file-meta',
              children: [ext, formatFileSize(session.file_sizes?.[name])]
                .filter(Boolean)
                .join(' · '),
            }),
          ],
        }),
        audio &&
          session.state === 'stopped' &&
          jsx('button', {
            className: 'file-action',
            onClick: onPlay,
            'aria-label': `Play meeting audio from ${name}`,
            title: 'Play meeting audio',
            children: jsx(PlayIcon, {}),
          }),
        jsx('a', {
          className: 'file-action',
          href: `${API}/sessions/${session.id}/files/${encodeURIComponent(name)}`,
          download: name,
          'aria-label': `Download ${name}`,
          title: `Download ${name}`,
          children: jsx(Glyph, { name: 'download', size: 15 }),
        }),
      ],
    });
  };
  return jsxs('section', {
    className: 'context-card files-card',
    'aria-label': 'Meeting files',
    children: [
      jsxs('div', {
        className: 'section-heading',
        children: [
          jsx('h3', { children: 'Meeting files' }),
          jsx('span', { className: 'count-badge', children: files.length }),
        ],
      }),
      ...primary.map(fileRow),
      supporting.length > 0 &&
        jsxs('details', {
          className: 'supporting-files',
          children: [
            jsx('summary', {
              children: `Supporting files · ${supporting.length}`,
            }),
            ...supporting.map(fileRow),
          ],
        }),
      !files.length &&
        jsx('p', {
          className: 'muted text-sm',
          children: 'Files will appear here after recording.',
        }),
    ],
  });
}

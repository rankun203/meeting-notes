import { useState } from 'react';
import { track } from './analytics.mjs';
import {
  jsx,
  jsxs,
  Fragment,
  PAGE_SIZE,
  RecordIcon,
  PlayIcon,
  formatDuration,
  formatTime,
} from './utils.mjs';
import { PeopleSidebar } from './people.mjs';
import { SettingsSidebar } from './settings.mjs';
import { Glyph } from './workspace.mjs';

export function Sidebar({
  sessions,
  total,
  offset,
  selectedId,
  onSelect,
  onQuickPlay,
  onPageChange,
  setShowNew,
  currentView,
  onViewChange,
  people,
  selectedPersonId,
  setSelectedPersonId,
  refreshPeople,
  settingsCategory,
  setSettingsCategory,
}) {
  const [query, setQuery] = useState('');
  const recording = sessions.find((s) => s.state === 'recording');
  const visible = sessions.filter((s) => {
    const text = [s.name, ...(s.tags || [])]
      .filter(Boolean)
      .join(' ')
      .toLowerCase();
    return text.includes(query.toLowerCase());
  });
  return jsxs('aside', {
    className: 'library-sidebar',
    'aria-label': 'Workspace navigation',
    children: [
      jsxs('div', {
        className: 'brand',
        children: [
          jsx('span', {
            className: 'brand-symbol',
            children: jsx(Glyph, { name: 'sound', size: 25 }),
          }),
          jsxs('div', {
            children: [jsx('strong', { children: 'Meeting Notes' })],
          }),
        ],
      }),
      jsx('button', {
        className: 'new-recording',
        'aria-label': 'Record a meeting',
        onClick: () => {
          track('recording_form_opened');
          setShowNew(true);
        },
        children: jsxs(Fragment, {
          children: [
            jsx(RecordIcon, {}),
            'Record a meeting',
            jsx('span', { children: '+' }),
          ],
        }),
      }),
      recording &&
        jsx('button', {
          className: 'live-recording-link',
          onClick: () => {
            onViewChange('sessions');
            onSelect(recording.id);
          },
          children: '● Recording in progress →',
        }),
      jsx('nav', {
        className: 'workspace-nav',
        children: [
          ['sessions', 'library', 'Library'],
          ['people', 'people', 'People'],
          ['settings', 'settings', 'Settings'],
        ].map(([id, icon, label]) =>
          jsxs('button', {
            key: id,
            className: currentView === id ? 'nav-active' : '',
            onClick: () => onViewChange(id),
            'aria-current': currentView === id ? 'page' : undefined,
            children: [jsx(Glyph, { name: icon, size: 17 }), label],
          }),
        ),
      }),
      currentView === 'sessions' &&
        jsxs(Fragment, {
          children: [
            jsxs('div', {
              className: 'library-heading',
              children: [
                jsx('h2', { children: 'Meetings' }),
                jsx('span', { children: total }),
              ],
            }),
            jsxs('label', {
              className: 'library-search',
              children: [
                jsx(Glyph, { name: 'search', size: 16 }),
                jsx('input', {
                  value: query,
                  onChange: (e) => setQuery(e.target.value),
                  placeholder:
                    total > PAGE_SIZE ? 'Search this page…' : 'Find a meeting…',
                  'aria-label':
                    total > PAGE_SIZE
                      ? 'Search meetings on this page'
                      : 'Search meetings',
                }),
              ],
            }),
            jsx('div', {
              className: 'meeting-list',
              children: visible.length
                ? visible.map((s) =>
                    jsxs('div', {
                      key: s.id,
                      className: `meeting-list-item ${selectedId === s.id ? 'is-selected' : ''}`,
                      children: [
                        jsx('button', {
                          className: 'meeting-select',
                          title: [
                            s.name || 'Untitled meeting',
                            ...(s.tags || []),
                          ].join(' · '),
                          onClick: () => onSelect(s.id),
                          'aria-current':
                            selectedId === s.id ? 'true' : undefined,
                          children: jsxs(Fragment, {
                            children: [
                              jsx('span', {
                                className: 'meeting-list-date',
                                children: formatTime(s.created_at),
                              }),
                              jsx('strong', {
                                children: s.name || 'Untitled meeting',
                              }),
                              jsxs('span', {
                                className: 'meeting-list-meta',
                                children: [
                                  jsx(Glyph, {
                                    name:
                                      s.state === 'recording'
                                        ? 'sound'
                                        : s.summary_available
                                          ? 'spark'
                                          : 'file',
                                    size: 12,
                                  }),
                                  s.state === 'recording'
                                    ? 'Recording'
                                    : s.duration_secs != null
                                      ? formatDuration(s.duration_secs)
                                      : s.summary_available
                                        ? 'Summary'
                                        : s.transcript_available
                                          ? 'Transcript'
                                          : 'Audio',
                                ],
                              }),
                            ],
                          }),
                        }),
                        s.state === 'stopped' &&
                          s.files?.some((f) => /\.(mp3|wav|opus)$/i.test(f)) &&
                          jsx('button', {
                            className: 'quick-play',
                            'aria-label': `Play ${s.name || 'meeting'}`,
                            title: 'Play meeting',
                            onClick: () => onQuickPlay(s.id),
                            children: jsx(PlayIcon, {}),
                          }),
                      ],
                    }),
                  )
                : jsx('div', {
                    className: 'library-empty',
                    children:
                      query
                        ? 'No matching meetings. Try another search.'
                        : 'No meetings yet. Record or import a meeting to get started.',
                  }),
            }),
            total > PAGE_SIZE &&
              jsxs('div', {
                className: 'library-pagination',
                children: [
                  jsx('button', {
                    disabled: offset === 0,
                    onClick: () =>
                      onPageChange(Math.max(0, offset - PAGE_SIZE)),
                    children: 'Previous',
                  }),
                  jsx('span', {
                    children: `${Math.floor(offset / PAGE_SIZE) + 1} / ${Math.ceil(total / PAGE_SIZE)}`,
                  }),
                  jsx('button', {
                    disabled: offset + PAGE_SIZE >= total,
                    onClick: () => onPageChange(offset + PAGE_SIZE),
                    children: 'Next',
                  }),
                ],
              }),
          ],
        }),
      currentView === 'people' &&
        jsx('div', {
          className: 'sidebar-secondary',
          children: jsx(PeopleSidebar, {
            selectedId: selectedPersonId,
            onSelect: setSelectedPersonId,
            people,
            onRefresh: refreshPeople,
          }),
        }),
      currentView === 'settings' &&
        jsx('div', {
          className: 'sidebar-secondary',
          children: jsx(SettingsSidebar, {
            selected: settingsCategory,
            onSelect: setSettingsCategory,
          }),
        }),
    ],
  });
}

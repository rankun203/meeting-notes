import { jsx, jsxs, Fragment, PAGE_SIZE, CloseIcon } from './utils.mjs';
import { NewSessionPanel, SidebarItem } from './session.mjs';
import { PeopleSidebar } from './people.mjs';
import { SettingsSidebar } from './settings.mjs';

export function Sidebar({ sessions, total, offset, selectedId, onSelect, onPageChange, sources, fields, onCreated, showNew, setShowNew, currentView, onViewChange, people, selectedPersonId, setSelectedPersonId, refreshPeople, settingsCategory, setSettingsCategory }) {
  const header = jsx('div', {
    key: 'header',
    className: 'flex-shrink-0 px-4 py-3 md:py-4 border-b border-gray-100 dark:border-gray-800',
    children: jsxs('div', { className: 'flex flex-col gap-2', children: [
      jsxs('div', { className: 'flex items-center justify-between', children: [
        jsx('h1', { className: 'text-sm font-semibold tracking-tight', children: 'Meeting Notes' }),
        jsx('button', {
          onClick: () => setShowNew(!showNew),
          className: showNew
            ? 'w-7 h-7 flex items-center justify-center rounded-lg bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 transition-colors'
            : 'w-7 h-7 flex items-center justify-center rounded-full bg-red-500 hover:bg-red-600 text-white transition-colors',
          title: showNew ? 'Close' : 'Record',
          children: showNew
            ? jsx(CloseIcon, {})
            : jsx('svg', {
                xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 24 24', fill: 'currentColor', className: 'w-4 h-4',
                children: jsx('circle', { cx: '12', cy: '12', r: '8' }),
              }),
        }),
      ]}),
      // Nav tabs
      jsxs('div', { className: 'flex gap-1', children: [
        jsx('button', {
          onClick: () => onViewChange('sessions'),
          className: `px-2 py-1 rounded text-[11px] font-medium transition-colors ${currentView === 'sessions' ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-gray-100' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'}`,
          children: 'Sessions',
        }),
        jsx('button', {
          onClick: () => onViewChange('people'),
          className: `px-2 py-1 rounded text-[11px] font-medium transition-colors ${currentView === 'people' ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-gray-100' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'}`,
          children: 'People',
        }),
        jsx('button', {
          onClick: () => onViewChange('settings'),
          className: `px-2 py-1 rounded text-[11px] font-medium transition-colors ${currentView === 'settings' ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-gray-100' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'}`,
          children: 'Settings',
        }),
      ]}),
    ]}),
  });

  const sidebarChildren = [header];

  if (currentView === 'sessions') {
    if (showNew) {
      sidebarChildren.push(jsx('div', {
        key: 'new-form',
        className: 'px-3 py-3 border-b border-gray-100 dark:border-gray-800',
        children: jsx(NewSessionPanel, {
          sources, fields,
          onCreated: async () => { await onCreated(); },
          onSelect: (id) => { setShowNew(false); onSelect(id); },
        }),
      }));
    }

    sidebarChildren.push(jsx('div', {
      key: 'list',
      className: 'flex-1 overflow-y-auto sidebar-scroll px-2 py-2 space-y-0.5',
      children: sessions.length === 0
        ? jsx('p', { className: 'text-xs text-gray-400 dark:text-gray-600 text-center py-8', children: 'No sessions yet' })
        : sessions.map(s => jsx(SidebarItem, {
            key: s.id, session: s,
            selected: s.id === selectedId,
            onClick: () => onSelect(s.id),
          })),
    }));

    if (total > PAGE_SIZE) {
      sidebarChildren.push(jsxs('div', {
        key: 'pagination',
        className: 'flex-shrink-0 flex items-center justify-between px-3 py-2 border-t border-gray-100 dark:border-gray-800 text-[11px] text-gray-400 dark:text-gray-500',
        children: [
          jsx('button', {
            disabled: offset === 0,
            onClick: () => onPageChange(Math.max(0, offset - PAGE_SIZE)),
            className: 'hover:text-gray-700 dark:hover:text-gray-300 disabled:opacity-30 transition-colors',
            children: 'Prev',
          }),
          jsx('span', { children: `${Math.floor(offset / PAGE_SIZE) + 1} / ${Math.ceil(total / PAGE_SIZE)}` }),
          jsx('button', {
            disabled: offset + PAGE_SIZE >= total,
            onClick: () => onPageChange(offset + PAGE_SIZE),
            className: 'hover:text-gray-700 dark:hover:text-gray-300 disabled:opacity-30 transition-colors',
            children: 'Next',
          }),
        ],
      }));
    }
  } else if (currentView === 'people') {
    sidebarChildren.push(jsx('div', {
      key: 'people-list',
      className: 'flex-1 overflow-y-auto sidebar-scroll',
      children: jsx(PeopleSidebar, {
        selectedId: selectedPersonId,
        onSelect: (id) => setSelectedPersonId(id),
        people,
        onRefresh: refreshPeople,
      }),
    }));
  } else if (currentView === 'settings') {
    sidebarChildren.push(jsx('div', {
      key: 'settings-nav',
      className: 'flex-1 overflow-y-auto sidebar-scroll',
      children: jsx(SettingsSidebar, { selected: settingsCategory, onSelect: setSettingsCategory }),
    }));
  }

  return jsx('div', {
    className: 'h-full flex flex-col bg-white dark:bg-gray-900 md:border-r border-gray-200 dark:border-gray-800',
    children: sidebarChildren,
  });
}

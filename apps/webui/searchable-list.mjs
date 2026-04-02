import { useState, useEffect, useRef, useCallback } from 'react';
import { jsx, jsxs } from './utils.mjs';

/**
 * SearchableList — a generic searchable dropdown component.
 *
 * Props:
 *   items: Array<{ id: string, label: string, detail?: string }>
 *     The list of selectable items.
 *   onSelect: (item) => void
 *     Called when an existing item is selected.
 *   onCreateAndSelect?: (query: string) => void
 *     If provided, enables "Create: xxx" as the first item when the query
 *     doesn't match any existing item. Called with the raw query string.
 *   onClose: () => void
 *     Called when the popover should close (Escape, click outside).
 *   anchorPoint: { x: number, y: number }
 *     The viewport pixel coordinates to anchor the popover near.
 *   placeholder?: string
 *     Input placeholder text.
 *   renderItem?: (item, highlighted: boolean) => ReactNode
 *     Optional custom renderer for each item row.
 */
export function SearchableList({ items, onSelect, onCreateAndSelect, onClose, anchorPoint, placeholder, renderItem, width }) {
  const [query, setQuery] = useState('');
  const [highlightIdx, setHighlightIdx] = useState(0);
  const inputRef = useRef(null);
  const containerRef = useRef(null);
  const listRef = useRef(null);

  // Filter items by query
  const lowerQuery = query.toLowerCase().trim();
  const filtered = lowerQuery
    ? items.filter(item => item.label.toLowerCase().includes(lowerQuery))
    : items;

  // Only show "Create: xxx" when there are no matching candidates at all
  const showCreate = onCreateAndSelect && lowerQuery && filtered.length === 0;
  const createItem = showCreate ? { id: '__create__', label: `Create: ${query.trim()}` } : null;
  const visibleItems = createItem ? [createItem, ...filtered] : filtered;

  // Clamp highlight index
  useEffect(() => {
    setHighlightIdx(0);
  }, [query]);

  // Focus input once positioned (style becomes non-null)
  const style = usePopoverPosition(anchorPoint);
  useEffect(() => {
    if (!style) return;
    const t = setTimeout(() => inputRef.current?.focus(), 0);
    return () => clearTimeout(t);
  }, [style !== null]);

  // Click outside to close
  useEffect(() => {
    function handler(e) {
      if (containerRef.current && !containerRef.current.contains(e.target)) {
        onClose();
      }
    }
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [onClose]);

  // Scroll highlighted item into view
  useEffect(() => {
    if (!listRef.current) return;
    const el = listRef.current.children[highlightIdx];
    if (el) el.scrollIntoView({ block: 'nearest' });
  }, [highlightIdx]);

  function selectItem(item) {
    if (item.id === '__create__') {
      onCreateAndSelect(query.trim());
    } else {
      onSelect(item);
    }
  }

  function onKeyDown(e) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlightIdx(prev => Math.min(prev + 1, visibleItems.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlightIdx(prev => Math.max(prev - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (visibleItems.length > 0) {
        selectItem(visibleItems[highlightIdx]);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  if (!style) return null;

  return jsx('div', {
    ref: containerRef,
    className: 'fixed z-50',
    style,
    children: jsxs('div', {
      className: 'bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-xl overflow-hidden',
      style: { width: width ? `${width}px` : '220px' },
      children: [
        // Search input
        jsx('div', {
          className: 'p-1.5 border-b border-gray-100 dark:border-gray-700',
          children: jsx('input', {
            ref: inputRef,
            type: 'text',
            value: query,
            onChange: e => setQuery(e.target.value),
            onKeyDown,
            placeholder: placeholder || 'Search...',
            className: 'w-full text-xs px-2 py-1.5 rounded border border-gray-200 dark:border-gray-600 bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-1 focus:ring-blue-500',
          }),
        }),
        // Items list
        jsx('div', {
          ref: listRef,
          className: 'overflow-y-auto',
          style: { maxHeight: '200px' },
          children: visibleItems.length === 0
            ? jsx('div', { className: 'px-3 py-3 text-[11px] text-gray-400 text-center', children: 'No results' })
            : visibleItems.map((item, idx) => {
                const highlighted = idx === highlightIdx;
                if (renderItem && item.id !== '__create__') {
                  return jsx('div', {
                    key: item.id,
                    className: [
                      'cursor-pointer transition-colors',
                      highlighted ? 'bg-blue-50 dark:bg-blue-900/30' : 'hover:bg-gray-50 dark:hover:bg-gray-800/50',
                    ].join(' '),
                    onMouseEnter: () => setHighlightIdx(idx),
                    onClick: () => selectItem(item),
                    children: renderItem(item, highlighted),
                  });
                }
                return jsx('div', {
                  key: item.id,
                  className: [
                    'px-3 py-1.5 text-xs cursor-pointer transition-colors',
                    item.id === '__create__'
                      ? highlighted
                        ? 'bg-blue-50 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400 font-medium'
                        : 'text-blue-600 dark:text-blue-400 font-medium hover:bg-blue-50 dark:hover:bg-blue-900/20'
                      : highlighted
                        ? 'bg-blue-50 dark:bg-blue-900/30 text-gray-900 dark:text-gray-100'
                        : 'text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800/50',
                  ].join(' '),
                  onMouseEnter: () => setHighlightIdx(idx),
                  onClick: () => selectItem(item),
                  children: item.detail
                    ? jsxs('div', { className: 'flex items-center justify-between gap-2', children: [
                        jsx('span', { className: 'truncate', children: item.label }),
                        jsx('span', { className: 'text-[10px] text-gray-400 flex-shrink-0', children: item.detail }),
                      ]})
                    : item.label,
                });
              }),
        }),
      ],
    }),
  });
}

/**
 * Compute popover position: tries below-right of anchor, flips if near edges.
 */
function usePopoverPosition(anchorPoint) {
  const [pos, setPos] = useState(null);

  useEffect(() => {
    if (!anchorPoint) return;
    const pad = 8;
    const popW = 220;
    const popH = 260;
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    let left = anchorPoint.x;
    let top = anchorPoint.y + 4;

    if (left + popW + pad > vw) {
      left = Math.max(pad, anchorPoint.x - popW);
    }
    if (top + popH + pad > vh) {
      top = Math.max(pad, anchorPoint.y - popH - 4);
    }

    setPos({ top, left });
  }, [anchorPoint]);

  return pos;
}

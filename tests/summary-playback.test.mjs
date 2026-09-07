import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  citationRange,
  highlightSummary,
} from '../apps/webui/summary-playback.mjs';

test('uses explicit citation ranges and limits point citations to the next timestamp or 90 seconds', () => {
  assert.deepEqual(citationRange('/sessions/demo?jump=80&jump_end=130'), {
    start: 80,
    end: 130,
  });
  assert.deepEqual(citationRange('/sessions/demo?jump=80', 100), {
    start: 80,
    end: 100,
  });
  assert.deepEqual(citationRange('/sessions/demo?jump=80'), {
    start: 80,
    end: 170,
  });
  assert.equal(citationRange('/sessions/demo'), null);
  assert.equal(citationRange('/sessions/demo?jump=-1'), null);
  assert.equal(citationRange('/sessions/demo?jump=NaN'), null);
});

test('highlights cited passages at the current time and clears them outside their ranges', () => {
  const active = new Set();
  const block = {
    classList: { add: (k) => active.add(k), remove: (k) => active.delete(k) },
    setAttribute() {},
    removeAttribute() {},
  };
  const link = {
    getAttribute: () => '/sessions/demo?jump=80&jump_end=130',
    closest: () => block,
  };
  const container = {
    querySelectorAll: (selector) =>
      selector.startsWith('a[')
        ? [link]
        : active.has('summary-playing')
          ? [block]
          : [],
  };
  highlightSummary(container, 79);
  assert.equal(active.size, 0);
  highlightSummary(container, 80);
  assert.ok(active.has('summary-playing'));
  highlightSummary(container, 130);
  assert.equal(active.size, 0);
});

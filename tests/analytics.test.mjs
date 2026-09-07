import { test } from 'node:test';
import assert from 'node:assert/strict';
import { apiFeature, refreshAnalytics, track } from '../apps/webui/analytics.mjs';

test('maps feature actions without recording identifiers, read polling, or query text', () => {
  assert.equal(apiFeature('/sessions/private-id/recording/start', 'POST'), 'recording_start');
  assert.equal(apiFeature('/sessions/private-id/summary', 'GET'), null);
  assert.equal(apiFeature('/sessions?search=private', 'GET'), null);
  assert.equal(apiFeature('/tags/private-name', 'PATCH'), 'tags_manage');
});

test('tracking is disabled without config; failures and blocked storage do not affect actions', async () => {
  const calls = [];
  globalThis.fetch = async (url, options) => { calls.push({url, options}); return {ok:true, json:async () => ({enabled:true})}; };
  track('app_opened');
  assert.equal(calls.length, 0);
  await refreshAnalytics();
  globalThis.localStorage = {getItem() { throw new Error('blocked'); }};
  track('app_opened');
  track('content_tab_opened', {tab:'summary'});
  const events = calls.filter(c => c.options).map(c => JSON.parse(c.options.body));
  assert.equal(events.length, 2);
  assert.match(events[0].distinct_id, /^[0-9a-f-]{36}$/);
  assert.equal(events[0].distinct_id, events[1].distinct_id);
  assert.equal(events[0].session_id, events[1].session_id);
  globalThis.fetch = async () => { throw new Error('offline'); };
  assert.doesNotThrow(() => track('app_opened'));
  await refreshAnalytics();
  globalThis.fetch = async () => { assert.fail('disabled analytics must not send'); };
  track('app_opened');
});

// Browser regression check for unequal audio tracks and rejected playback.
import assert from 'node:assert/strict';
const { chromium } = await import(
  process.env.MEETING_NOTES_PLAYWRIGHT_MODULE || 'playwright'
);
const base = process.env.MEETING_NOTES_DEMO_URL || 'http://127.0.0.1:33490';
function wav(seconds) {
  const dataSize = 8000 * 2 * seconds,
    data = Buffer.alloc(44 + dataSize);
  data.write('RIFF');
  data.writeUInt32LE(36 + dataSize, 4);
  data.write('WAVEfmt ', 8);
  data.writeUInt32LE(16, 16);
  data.writeUInt16LE(1, 20);
  data.writeUInt16LE(1, 22);
  data.writeUInt32LE(8000, 24);
  data.writeUInt32LE(16000, 28);
  data.writeUInt16LE(2, 32);
  data.writeUInt16LE(16, 34);
  data.write('data', 36);
  data.writeUInt32LE(dataSize, 40);
  return data;
}
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage();
  await page.route('**/player-test', (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><div id="root"></div><script type="importmap">{"imports":{"react":"https://esm.sh/react@19","react-dom/client":"https://esm.sh/react-dom@19/client","react/jsx-runtime":"https://esm.sh/react@19/jsx-runtime"}}</script><script type="module">import React from 'react';import{createRoot}from'react-dom/client';import{SyncedPlayer}from'/player.mjs';window.playerRef=React.createRef();createRoot(document.getElementById('root')).render(React.createElement(SyncedPlayer,{ref:window.playerRef,sessionId:'player-test',files:[{name:'short.wav',label:'Short track'},{name:'long.wav',label:'Long track'}]}));</script>`,
    }),
  );
  await page.route('**/api/sessions/player-test/files/*', (route) =>
    route.fulfill({
      contentType: 'audio/wav',
      body: wav(route.request().url().endsWith('short.wav') ? 1 : 8),
    }),
  );
  await page.route('**/api/sessions/player-test/waveform/*', (route) =>
    route.fulfill({ json: { data: [0, 0], duration_secs: 8 } }),
  );
  await page.goto(`${base}/player-test`);
  await page.waitForFunction(
    () => document.querySelector('input[type=range]')?.max === '8',
  );
  await page.evaluate(() => window.playerRef.current.seekAndPlay(3));
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .waitFor();
  assert.equal(
    await page
      .locator('audio')
      .first()
      .evaluate((a) => a.paused),
    true,
    'Short track must not restart from zero after seeking past its end.',
  );
  await page.waitForFunction(
    () => document.querySelectorAll('audio')[1].currentTime > 3.2,
  );
  await page.evaluate(() => window.playerRef.current.seekTo(0.1));
  await page.waitForFunction(() => !document.querySelector('audio').paused);
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .click();
  await page.evaluate(() => {
    HTMLMediaElement.prototype.play = () =>
      Promise.reject(new Error('device unavailable'));
  });
  await page.getByRole('button', { name: 'Play meeting', exact: true }).click();
  await page.getByRole('alert').waitFor();
  assert.ok(
    await page
      .getByRole('button', { name: 'Play meeting', exact: true })
      .isVisible(),
  );
  console.log(
    'Passed unequal-duration tracks, backward seek resumption, and playback failure recovery.',
  );
} finally {
  await browser.close();
}

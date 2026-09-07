// Run against scripts/seed-design-demo.py fixtures, never your working data.
import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
const { chromium } = await import(
  process.env.MEETING_NOTES_PLAYWRIGHT_MODULE || 'playwright'
);
const base = process.env.MEETING_NOTES_DEMO_URL || 'http://127.0.0.1:33490';
const output =
  process.env.MEETING_NOTES_PREVIEW_DIR || '/tmp/meeting-notes-preview';
await mkdir(output, { recursive: true });
const config = await (await fetch(`${base}/api/analytics/config`)).json();
assert.equal(
  config.enabled,
  false,
  'Use the isolated demo with analytics disabled.',
);
const fixture = await (
  await fetch(`${base}/api/sessions/demo-meeting-1`)
).json();
assert.equal(fixture.name, 'Monday product sync', 'Run the demo seeder first.');
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`${base}/sessions/demo-meeting-1`);
  await page.locator('.summary-scroll .md-content h2').first().waitFor();
  await page.getByRole('slider', { name: 'Playback position' }).waitFor();
  await page.screenshot({ path: `${output}/desktop.png` });
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  await page.getByRole('group', { name: 'Export formats' }).waitFor();
  await page.screenshot({ path: `${output}/toolbar-export.png` });
  const downloadReady = page.waitForEvent('download');
  await page.getByRole('button', { name: /Markdown Editable notes/ }).click();
  const download = await downloadReady;
  assert.equal(download.suggestedFilename(), 'Monday product sync_summary.md');
  assert.match(
    await readFile(await download.path(), 'utf8'),
    /The direction is clear/,
  );
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#export-options').count(), 0);
  assert.equal(
    await page
      .locator('.export-trigger')
      .evaluate((el) => el === document.activeElement),
    true,
  );
  await page
    .getByRole('button', { name: 'Regenerate summary', exact: true })
    .click();
  const instructions = page.getByPlaceholder(
    'Additional instructions (optional). Press Enter to generate, Esc to cancel...',
  );
  await instructions.waitFor();
  await instructions.press('Escape');
  await page.getByRole('button', { name: 'Play meeting', exact: true }).click();
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .waitFor();
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .click();
  await page.locator('.summary-scroll a[href*="jump=80&"]').first().click();
  await page.waitForFunction(() =>
    document
      .querySelector('.summary-playing')
      ?.textContent.includes('Make listening effortless'),
  );
  assert.equal(
    await page
      .getByRole('tab', { name: 'Summary', exact: true })
      .getAttribute('aria-selected'),
    'true',
  );
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .click();
  assert.ok(
    Number(
      await page
        .getByRole('slider', { name: 'Playback position' })
        .inputValue(),
    ) >= 80,
  );
  await page.screenshot({ path: `${output}/summary-playing.png` });
  await page.locator('.summary-scroll').evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  let dock = await page.locator('.playback-dock').boundingBox();
  assert.ok(dock.y + dock.height <= 1001);
  assert.ok(
    await page
      .getByRole('button', { name: 'Play meeting', exact: true })
      .isVisible(),
  );
  // Keyboard tabs and native slider, without interrupting playback.
  await page.getByRole('tab', { name: 'Summary', exact: true }).focus();
  await page.keyboard.press('ArrowLeft');
  await page
    .getByRole('button', { name: 'Play from 2:00', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .waitFor();
  await page
    .getByRole('combobox', { name: 'Playback speed' })
    .selectOption('1.5');
  await page.screenshot({ path: `${output}/transcript.png` });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(
    await page
      .getByRole('tab', { name: 'Transcript', exact: true })
      .getAttribute('aria-selected'),
    'true',
  );
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .waitFor();
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .click();
  assert.equal(
    await page.getByRole('combobox', { name: 'Playback speed' }).inputValue(),
    '1.5',
  );
  await page.screenshot({ path: `${output}/mobile.png` });
  // Both toolbar variants fit on narrow phones; mobile icons retain accessible names.
  await page.getByRole('tab', { name: 'Summary', exact: true }).click();
  for (const width of [320, 390]) {
    await page.setViewportSize({ width, height: 844 });
    await page.getByRole('button', { name: 'Export', exact: true }).click();
    const menu = await page.locator('#export-options').boundingBox();
    assert.ok(
      menu.x >= 0 && menu.x + menu.width <= width,
      'Export formats must fit the phone width.',
    );
    const tabs = await page.locator('.reader-tabs').boundingBox();
    const actions = await page.locator('.reader-actions').boundingBox();
    assert.ok(
      tabs.x + tabs.width <= actions.x,
      'Toolbar actions must not crowd the tabs.',
    );
    await page.keyboard.press('Escape');
  }
  await page
    .getByRole('button', { name: 'Regenerate summary', exact: true })
    .click();
  await instructions.waitFor();
  await instructions.press('Escape');
  await page.getByRole('tab', { name: 'Transcript', exact: true }).click();
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  await page
    .getByRole('button', { name: /Lyrics Timestamped transcript/ })
    .waitFor();
  await page.keyboard.press('Escape');
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
  );
  dock = await page.locator('.playback-dock').boundingBox();
  const launcher = await page
    .getByRole('button', { name: 'Open meeting assistant' })
    .boundingBox();
  assert.ok(
    launcher.y + launcher.height < dock.y,
    'Chat launcher must not cover playback.',
  );
  await page
    .getByRole('button', { name: 'Files & details', exact: true })
    .click();
  await page
    .getByRole('region', { name: 'Meeting files' })
    .waitFor({ state: 'visible' });
  await page.screenshot({ path: `${output}/mobile-files.png` });
  await page
    .getByRole('button', { name: 'Close details', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Record a meeting', exact: true })
    .click();
  await page.locator('dialog[open]').waitFor();
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('dialog').count(), 0);
  // Mobile settings navigation remains available.
  await page
    .getByRole('button', { name: 'Back to library', exact: true })
    .click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('button', { name: 'Usage analytics', exact: true })
    .click();
  await page
    .getByRole('heading', { name: 'PostHog usage analytics' })
    .waitFor();
  await page
    .getByRole('button', { name: '← Meeting library', exact: true })
    .click();
  await page
    .getByRole('textbox', { name: 'Search meetings', exact: true })
    .fill('onboarding');
  await page.waitForFunction(
    () => document.querySelectorAll('.meeting-list-item').length === 1,
  );
  assert.equal(await page.locator('.meeting-list-item').count(), 1);
  await page
    .getByRole('button', {
      name: 'Play A better first five minutes',
      exact: true,
    })
    .click();
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .waitFor();
  await page
    .getByRole('button', { name: 'Pause meeting', exact: true })
    .click();
  for (const [width, height] of [
    [768, 1024],
    [1024, 768],
    [1440, 1000],
  ]) {
    await page.setViewportSize({ width, height });
    assert.equal(
      await page.evaluate(
        () => document.documentElement.scrollWidth > innerWidth,
      ),
      false,
    );
    assert.ok(
      await page
        .getByRole('button', { name: 'Play meeting', exact: true })
        .isVisible(),
    );
  }
  // Notes save through the real daemon; restore the seeded text afterward.
  await page.goto(`${base}/sessions/demo-meeting-1`);
  await page
    .getByRole('textbox', { name: 'Meeting notes', exact: true })
    .fill('A synthetic browser verification note.');
  await page.waitForResponse(
    (r) =>
      r.url().endsWith('/api/sessions/demo-meeting-1') &&
      r.request().method() === 'PATCH',
  );
  assert.equal(
    (await (await fetch(`${base}/api/sessions/demo-meeting-1`)).json()).notes,
    'A synthetic browser verification note.',
  );
  await page
    .getByRole('textbox', { name: 'Meeting notes', exact: true })
    .fill(fixture.notes);
  await page.waitForResponse(
    (r) =>
      r.url().endsWith('/api/sessions/demo-meeting-1') &&
      r.request().method() === 'PATCH',
  );
  assert.deepEqual(errors, []);
  // Stress the library with a full page of meetings, without changing demo data.
  // Supply the same list through initial WebSocket snapshots and pagination.
  const library = await browser.newPage();
  const originals = (await (await fetch(`${base}/api/sessions`)).json())
    .sessions;
  const densityData = {
    sessions: Array.from({ length: 50 }, (_, i) => ({
      ...originals[i % originals.length],
      id: i < originals.length ? originals[i].id : `density-meeting-${i}`,
    })),
    total: 75,
  };
  await library.routeWebSocket('**/api/ws', (socket) => {
    socket.send(JSON.stringify({ type: 'init', data: densityData }));
  });
  await library.route('**/api/sessions?*', (route) =>
    route.fulfill({ json: densityData }),
  );
  for (const [width, height, minimumRows] of [
    [1366, 768, 8],
    [1280, 720, 7],
    [390, 844, 8],
  ]) {
    await library.setViewportSize({ width, height });
    await library.goto(
      `${base}/${width < 768 ? '' : 'sessions/demo-meeting-1'}`,
    );
    await library.waitForFunction(
      () => document.querySelectorAll('.meeting-list-item').length === 50,
    );
    if (width >= 768)
      await library.locator('.summary-scroll .md-content h2').first().waitFor();
    const metrics = await library.locator('.meeting-list').evaluate((list) => {
      const box = list.getBoundingClientRect();
      return {
        height: box.height,
        visibleRows: [...list.children].filter((row) => {
          const r = row.getBoundingClientRect();
          return r.top >= box.top && r.bottom <= box.bottom;
        }).length,
        scrolls: list.scrollHeight > list.clientHeight,
      };
    });
    assert.ok(
      metrics.visibleRows >= minimumRows,
      `${width}×${height}: only ${metrics.visibleRows} complete meetings fit`,
    );
    assert.ok(
      metrics.height >= height * 0.55,
      'The meeting list needs most of the sidebar height.',
    );
    assert.ok(metrics.scrolls, 'The long library must scroll.');
    await library.screenshot({ path: `${output}/library-${width}.png` });
    await library.locator('.meeting-list').evaluate((list) => {
      list.scrollTop = list.scrollHeight;
    });
    assert.ok(
      await library
        .getByRole('button', { name: 'Record a meeting', exact: true })
        .isVisible(),
    );
    assert.ok(
      await library
        .getByRole('button', { name: 'Next', exact: true })
        .isVisible(),
    );
    if (width >= 768) {
      const player = await library.locator('.playback-dock').boundingBox();
      assert.ok(player.y + player.height <= height + 1);
    }
    console.log(
      `${width}×${height}: ${metrics.visibleRows} complete meetings; ${Math.round(metrics.height)}px list height.`,
    );
  }
  await library.close();
  console.log(
    'Passed: export download, toolbar menus and regeneration setup, narrow phone toolbar, playback, seeking, highlights, keyboard tabs, resize continuity, files, recording setup, search, quick play, mobile admin, notes persistence.',
  );
  console.log(`Screenshots: ${output}`);
} finally {
  await browser.close();
}

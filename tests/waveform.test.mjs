import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mergeWaveformPeaks } from '../apps/webui/waveform.mjs';

test('combines source activity without stretching a short track to the meeting duration', () => {
  const short = { duration_secs: 2, data: [-0.8, 0.8, -0.6, 0.6] };
  const long = {
    duration_secs: 4,
    data: [-0.2, 0.2, -0.3, 0.3, -0.4, 0.4, -0.5, 0.5],
  };
  const peaks = mergeWaveformPeaks([short, long], 4, 4);
  assert.deepEqual(
    Array.from(peaks, (n) => Math.round(n * 10)),
    [-8, 8, -6, 6, -4, 4, -5, 5],
  );
  assert.deepEqual(
    Array.from(mergeWaveformPeaks([short], 4, 4), (n) => Math.round(n * 10)),
    [-8, 8, -6, 6, 0, 0, 0, 0],
  );
});

test('preserves a brief peak when reducing resolution and handles absent or silent data', () => {
  assert.deepEqual(
    Array.from(
      mergeWaveformPeaks(
        [{ duration_secs: 4, data: [0, 0, -1, 1, 0, 0, 0, 0] }],
        4,
        1,
      ),
    ),
    [-1, 1],
  );
  assert.deepEqual(
    Array.from(
      mergeWaveformPeaks([null, { duration_secs: 2, data: [0, 0] }], 4, 2),
    ),
    [0, 0, 0, 0],
  );
  assert.deepEqual(Array.from(mergeWaveformPeaks([], 0, 2)), [0, 0, 0, 0]);
});

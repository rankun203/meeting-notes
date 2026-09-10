// Combine source activity on the meeting timeline. Short recordings keep their
// original timing; this is a peak envelope, not an audio mix.
export function mergeWaveformPeaks(tracks, duration, columns) {
  const peaks = new Float32Array(Math.max(0, columns) * 2);
  if (!(duration > 0) || !columns) return peaks;
  for (const waveform of tracks) {
    const bins = Math.floor((waveform?.data?.length || 0) / 2);
    const seconds = waveform?.duration_secs;
    if (!bins || !(seconds > 0)) continue;
    for (let col = 0; col < columns; col++) {
      const start = (col / columns) * duration;
      if (start >= seconds) break;
      const first = Math.floor((start / seconds) * bins);
      const last = Math.min(
        bins,
        Math.ceil(((((col + 1) / columns) * duration) / seconds) * bins),
      );
      for (let bin = first; bin < last; bin++) {
        const low = waveform.data[bin * 2];
        const high = waveform.data[bin * 2 + 1];
        if (Number.isFinite(low))
          peaks[col * 2] = Math.min(peaks[col * 2], Math.max(-1, low));
        if (Number.isFinite(high))
          peaks[col * 2 + 1] = Math.max(peaks[col * 2 + 1], Math.min(1, high));
      }
    }
  }
  return peaks;
}

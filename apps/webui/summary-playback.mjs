// Citations, not guessed semantic alignment, connect summary text to audio.
export function citationRange(href, nextStart = Infinity) {
  try {
    const url = new URL(href, 'http://localhost');
    const start = Number(url.searchParams.get('jump'));
    if (!url.searchParams.has('jump') || !Number.isFinite(start) || start < 0)
      return null;
    const explicitEnd = url.searchParams.has('jump_end')
      ? Number(url.searchParams.get('jump_end'))
      : NaN;
    const end =
      Number.isFinite(explicitEnd) && explicitEnd > start
        ? explicitEnd
        : Math.min(nextStart, start + 90);
    return { start, end };
  } catch {
    return null;
  }
}

export function highlightSummary(container, time) {
  if (!container) return;
  const links = [...container.querySelectorAll('a[href*="jump="]')];
  const starts = links
    .map((a) => citationRange(a.getAttribute('href'))?.start)
    .filter(Number.isFinite)
    .sort((a, b) => a - b);
  const active = new Set();
  for (const link of links) {
    const start = citationRange(link.getAttribute('href'))?.start;
    const range = citationRange(
      link.getAttribute('href'),
      starts.find((t) => t > start) ?? Infinity,
    );
    const block = link.closest('li, p, blockquote');
    if (
      block &&
      range &&
      Number.isFinite(time) &&
      time >= range.start &&
      time < range.end
    )
      active.add(block);
  }
  for (const el of container.querySelectorAll('.summary-playing')) {
    if (!active.has(el)) {
      el.classList.remove('summary-playing');
      el.removeAttribute('aria-current');
    }
  }
  for (const el of active) {
    el.classList.add('summary-playing');
    el.setAttribute('aria-current', 'true');
  }
}

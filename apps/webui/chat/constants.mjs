export const BUBBLE_SNAP_KEY = 'chat-bubble-position';
export const MARGIN = 20;
export const BUBBLE_SIZE = 56;
export const BUBBLE_SIZE_MOBILE = 48;

export function getSnapPoints(w, h, size) {
  const m = MARGIN;
  return [
    { x: m, y: m },
    { x: (w - size) / 2, y: m },
    { x: w - size - m, y: m },
    { x: m, y: (h - size) / 2 },
    { x: w - size - m, y: (h - size) / 2 },
    { x: m, y: h - size - m },
    { x: (w - size) / 2, y: h - size - m },
    { x: w - size - m, y: h - size - m },
  ];
}

export function nearestSnap(x, y, w, h, size) {
  const points = getSnapPoints(w, h, size);
  let best = 7, bestDist = Infinity;
  for (let i = 0; i < points.length; i++) {
    const dx = x - points[i].x, dy = y - points[i].y;
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = i; }
  }
  return best;
}

export function loadSnap() {
  try { const v = localStorage.getItem(BUBBLE_SNAP_KEY); return v ? JSON.parse(v) : 7; }
  catch { return 7; }
}

export function panelPosition(bx, by, bSize, isMobile) {
  if (isMobile) return { top: 8, left: 0, right: 0, bottom: 0 };
  const w = window.innerWidth, h = window.innerHeight;
  const pw = 460, ph = Math.round(h * 0.9);
  const centerX = bx + bSize / 2, centerY = by + bSize / 2;
  let left = centerX > w / 2 ? bx - pw - 12 : bx + bSize + 12;
  let top = centerY > h / 2 ? by + bSize - ph : by;
  left = Math.max(8, Math.min(left, w - pw - 8));
  top = Math.max(8, Math.min(top, h - ph - 8));
  return { top, left, width: pw, height: ph };
}

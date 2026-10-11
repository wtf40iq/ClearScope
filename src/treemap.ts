import type { TmNode } from "./types";

export interface TmRect {
  node: TmNode;
  x: number;
  y: number;
  w: number;
  h: number;
  depth: number;
  header: boolean;
}

interface Item {
  v: number;
  n: TmNode | null;
}

function worst(row: Item[], side: number, scale: number): number {
  let min = Infinity, max = 0, sum = 0;
  for (const r of row) {
    const a = r.v * scale;
    sum += a;
    if (a < min) min = a;
    if (a > max) max = a;
  }
  const s2 = side * side, sum2 = sum * sum;
  return Math.max((s2 * max) / sum2, sum2 / (s2 * min));
}

/** Squarified treemap (Bruls et al.). Items must be sorted desc. */
function squarify(items: Item[], x: number, y: number, w: number, h: number, cb: (it: Item, x: number, y: number, w: number, h: number) => void) {
  const total = items.reduce((s, i) => s + i.v, 0);
  if (total <= 0 || w <= 0 || h <= 0) return;
  const scale = (w * h) / total;
  let i = 0;
  while (i < items.length) {
    const side = Math.min(w, h);
    const row: Item[] = [items[i]];
    let j = i + 1;
    while (j < items.length) {
      const next = [...row, items[j]];
      if (worst(next, side, scale) > worst(row, side, scale)) break;
      row.push(items[j]);
      j++;
    }
    const rowArea = row.reduce((s, r) => s + r.v * scale, 0);
    if (w >= h) {
      const rw = rowArea / h;
      let cy = y;
      for (const r of row) {
        const rh = (r.v * scale) / rw;
        cb(r, x, cy, rw, rh);
        cy += rh;
      }
      x += rw;
      w -= rw;
    } else {
      const rh = rowArea / w;
      let cx = x;
      for (const r of row) {
        const rw = (r.v * scale) / rh;
        cb(r, cx, y, rw, rh);
        cx += rw;
      }
      y += rh;
      h -= rh;
    }
    i = j;
  }
}

export const HEADER = 16;

export function layout(root: TmNode, w: number, h: number): TmRect[] {
  const out: TmRect[] = [];
  const rec = (n: TmNode, x: number, y: number, rw: number, rh: number, depth: number) => {
    const header = depth > 0 && n.isDir && rw > 56 && rh > 34;
    out.push({ node: n, x, y, w: rw, h: rh, depth, header });
    if (!n.children || n.children.length === 0) return;
    const pad = depth === 0 ? 0 : rw > 8 && rh > 8 ? 2 : 0;
    const ix = x + pad, iy = y + pad + (header ? HEADER : 0);
    const iw = rw - pad * 2, ih = rh - pad * 2 - (header ? HEADER : 0);
    if (iw < 2 || ih < 2) return;
    const items: Item[] = n.children.filter((c) => c.size > 0).map((c) => ({ v: c.size, n: c }));
    const sum = items.reduce((s, i) => s + i.v, 0);
    const rest = n.size - sum;
    if (rest > n.size * 0.002) items.push({ v: rest, n: null }); // small files not loaded: leave honest empty space
    items.sort((a, b) => b.v - a.v);
    squarify(items, ix, iy, iw, ih, (it, cx, cy, cw, ch) => {
      if (it.n && cw >= 1 && ch >= 1) rec(it.n, cx, cy, cw, ch, depth + 1);
    });
  };
  rec(root, 0, 0, w, h, 0);
  return out;
}

import { useEffect, useMemo, useRef, useState } from "react";
import type { TmNode } from "../types";
import { api } from "../api";
import { layout, HEADER, type TmRect } from "../treemap";
import { CATEGORIES, colorOf } from "../categories";

export function Legend() {
  return <div className="legend">{CATEGORIES.map((c) => <span key={c.key}><i style={{ background: c.color }} />{c.label}</span>)}</div>;
}
import { fmtSize, fmtPct, pct } from "../format";

interface Props {
  focusId: number;
  selectedId: number | null;
  version: number;
  onSelect: (id: number) => void;
  onZoom: (id: number) => void;
  onContext: (id: number, x: number, y: number) => void;
  theme: string;
}

function shade(hex: string, k: number): string {
  const n = parseInt(hex.slice(1), 16);
  const f = (c: number) => Math.max(0, Math.min(255, Math.round(k > 0 ? c + (255 - c) * k : c * (1 + k))));
  return `rgb(${f(n >> 16)},${f((n >> 8) & 255)},${f(n & 255)})`;
}

export default function Treemap({ focusId, selectedId, version, onSelect, onZoom, onContext, theme }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const overlay = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [data, setData] = useState<TmNode | null>(null);
  const [hover, setHover] = useState<{ r: TmRect; x: number; y: number } | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setSize({ w: Math.floor(e.contentRect.width), h: Math.floor(e.contentRect.height) }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    let alive = true;
    // only show a spinner if loading is noticeably slow (avoids flicker on live updates)
    const t = setTimeout(() => alive && setLoading(true), 350);
    api.getTreemap(focusId, 9000).then((d) => alive && setData(d)).catch(() => {}).finally(() => { clearTimeout(t); if (alive) setLoading(false); });
    return () => { alive = false; clearTimeout(t); };
  }, [focusId, version]);

  const rects = useMemo(() => (data && size.w > 0 && size.h > 0 ? layout(data, size.w, size.h) : []), [data, size]);

  // main render — colours come from CSS variables so both themes look right
  useEffect(() => {
    const c = canvas.current;
    if (!c || size.w === 0) return;
    const css = getComputedStyle(document.documentElement);
    const v = (k: string) => css.getPropertyValue(k).trim();
    const light = document.documentElement.dataset.theme === "light";
    const bg = v("--tm-bg"), dirFill = v("--tm-dir"), dirLine = v("--tm-dir-line"), head = v("--tm-head"), headText = v("--tm-head-text"), txt = v("--tm-text");
    const dpr = window.devicePixelRatio || 1;
    c.width = size.w * dpr;
    c.height = size.h * dpr;
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = bg;
    g.fillRect(0, 0, size.w, size.h);
    g.textBaseline = "middle";
    const font = "'Segoe UI Variable Text','Segoe UI',system-ui,sans-serif";
    for (const r of rects) {
      if (r.depth === 0) continue;
      const { x, y, w, h, node } = r;
      if (node.isDir) {
        g.fillStyle = dirFill;
        g.fillRect(x, y, w, h);
        g.strokeStyle = dirLine;
        g.lineWidth = 1;
        g.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
        if (r.header) {
          g.fillStyle = head;
          g.fillRect(x + 1, y + 1, w - 2, HEADER);
          g.fillStyle = headText;
          g.font = `600 11px ${font}`;
          g.save();
          g.beginPath();
          g.rect(x + 4, y, w - 8, HEADER + 2);
          g.clip();
          g.fillText(`${node.name}  ${fmtSize(node.size)}`, x + 6, y + 1 + HEADER / 2);
          g.restore();
        }
      } else {
        const col = colorOf(node.ext);
        if (w >= 4 && h >= 4) {
          // soft, flat tile with a gentle top-left highlight (no glossy cushions)
          const gr = g.createLinearGradient(x, y, x, y + h);
          gr.addColorStop(0, shade(col, light ? 0.12 : 0.08));
          gr.addColorStop(1, shade(col, light ? -0.06 : -0.14));
          g.fillStyle = gr;
          g.fillRect(x + 0.5, y + 0.5, w - 1, h - 1);
        } else {
          g.fillStyle = col;
          g.fillRect(x, y, Math.max(w, 0.6), Math.max(h, 0.6));
        }
        if (w > 74 && h > 30) {
          g.save();
          g.beginPath();
          g.rect(x + 3, y + 2, w - 6, h - 4);
          g.clip();
          g.fillStyle = txt;
          g.font = `600 11px ${font}`;
          g.fillText(node.name, x + 7, y + 13);
          g.globalAlpha = 0.8;
          g.font = `11px ${font}`;
          g.fillText(fmtSize(node.size), x + 7, y + 27);
          g.restore();
        }
      }
    }
  }, [rects, size, theme]);

  // overlay: hover + selection
  useEffect(() => {
    const c = overlay.current;
    if (!c || size.w === 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = size.w * dpr;
    c.height = size.h * dpr;
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, size.w, size.h);
    const sel = rects.find((r) => r.node.id === selectedId && r.depth > 0);
    if (sel) {
      g.fillStyle = document.documentElement.dataset.theme === "light" ? "rgba(255,255,255,0.5)" : "rgba(0,0,0,0.3)";
      g.fillRect(0, 0, size.w, size.h);
      g.clearRect(sel.x, sel.y, sel.w, sel.h);
      g.strokeStyle = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#4c8dff";
      g.lineWidth = 2;
      g.strokeRect(sel.x + 1, sel.y + 1, sel.w - 2, sel.h - 2);
    }
    if (hover && hover.r.depth > 0) {
      g.strokeStyle = document.documentElement.dataset.theme === "light" ? "rgba(0,0,0,0.6)" : "rgba(255,255,255,0.8)";
      g.lineWidth = 1.5;
      g.strokeRect(hover.r.x + 0.75, hover.r.y + 0.75, hover.r.w - 1.5, hover.r.h - 1.5);
    }
  }, [rects, size, selectedId, hover]);

  const hit = (e: React.MouseEvent): TmRect | null => {
    const b = canvas.current!.getBoundingClientRect();
    const x = e.clientX - b.left, y = e.clientY - b.top;
    for (let i = rects.length - 1; i >= 0; i--) {
      const r = rects[i];
      if (x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h) {
        // prefer folder when pointing at its header strip
        return r;
      }
    }
    return null;
  };

  const rootSize = data?.size ?? 1;
  return (
    <div className="treemap" ref={wrap}>
      <canvas ref={canvas} style={{ width: size.w, height: size.h }} />
      <canvas
        ref={overlay}
        className="tm-overlay"
        style={{ width: size.w, height: size.h }}
        onMouseMove={(e) => {
          const r = hit(e);
          const b = wrap.current!.getBoundingClientRect();
          setHover(r ? { r, x: e.clientX - b.left, y: e.clientY - b.top } : null);
        }}
        onMouseLeave={() => setHover(null)}
        onClick={(e) => { const r = hit(e); if (r && r.depth > 0) onSelect(r.node.id); }}
        onDoubleClick={(e) => { const r = hit(e); if (r && r.depth > 0) onZoom(r.node.id); }}
        onContextMenu={(e) => { e.preventDefault(); const r = hit(e); if (r && r.depth > 0) { onSelect(r.node.id); onContext(r.node.id, e.clientX, e.clientY); } }}
      />
      {hover && hover.r.depth > 0 && (
        <div className="tm-tip" style={{ left: Math.max(4, Math.min(hover.x + 16, size.w - 260)), top: Math.max(4, Math.min(hover.y + 16, size.h - 70)) }}>
          <div className="tm-tip-name">
            <span className="dot" style={{ background: hover.r.node.isDir ? "var(--folder)" : colorOf(hover.r.node.ext) }} />
            {hover.r.node.name}
          </div>
          <div className="tm-tip-meta">
            {fmtSize(hover.r.node.size)} · {fmtPct(pct(hover.r.node.size, rootSize))} {hover.r.node.isDir ? "· папка" : hover.r.node.ext ? `· .${hover.r.node.ext}` : ""}
          </div>
        </div>
      )}
      {loading && <div className="tm-loading"><div className="spinner" /></div>}
    </div>
  );
}

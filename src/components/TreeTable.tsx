import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronRight } from "lucide-react";
import type { NodeInfo } from "../types";
import { api } from "../api";
import { fmtDate, fmtDelta, fmtNum, fmtPct, fmtSize, pct } from "../format";
import FileIcon from "./FileIcon";

const ROW = 30;
const HEAD = 32;
const NAME_MIN = 220;

type ColKey = "name" | "size" | "alloc" | "pct" | "files" | "modified" | "delta";
const COLS: { key: ColKey; label: string; w: number; sort?: SortKey }[] = [
  { key: "name", label: "Имя", w: NAME_MIN, sort: "name" },
  { key: "size", label: "Размер", w: 88, sort: "size" },
  { key: "alloc", label: "На диске", w: 88, sort: "alloc" },
  { key: "pct", label: "Доля", w: 116 },
  { key: "files", label: "Файлов", w: 76, sort: "files" },
  { key: "modified", label: "Изменён", w: 94, sort: "modified" },
  { key: "delta", label: "Изменение", w: 100 },
];
function loadWidths(): Partial<Record<ColKey, number>> {
  try { return JSON.parse(localStorage.getItem("ss.cols") ?? "{}"); } catch { return {}; }
}

interface Props {
  focusId: number;
  selectedId: number | null;
  version: number;
  hasPrev: boolean;
  /** null = show everything; otherwise hide items smaller than this (and empty folders) */
  minSize: number | null;
  onSelect: (id: number) => void;
  onZoom: (id: number) => void;
  onContext: (id: number, x: number, y: number) => void;
  onDelete: (ids: number[], permanent: boolean) => void;
}

type SortKey = "size" | "name" | "files" | "modified" | "alloc";

interface Row {
  node: NodeInfo;
  depth: number;
  parentSize: number;
  /** summary row for items hidden by the "hide small" filter */
  hidden?: { parent: number; count: number; size: number };
  /** summary row: folder has more children than were loaded */
  trunc?: { parent: number; shown: number; total: number };
}

const PAGE = 3000;

export default function TreeTable({ focusId, selectedId, version, hasPrev, minSize, onSelect, onZoom, onContext, onDelete }: Props) {
  const [root, setRoot] = useState<NodeInfo | null>(null);
  const [kids, setKids] = useState<Map<number, NodeInfo[]>>(new Map());
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "size", desc: true });
  const [scroll, setScroll] = useState(0);
  const [widths, setWidths] = useState<Partial<Record<ColKey, number>>>(loadWidths);
  const [height, setHeight] = useState(400);
  const body = useRef<HTMLDivElement>(null);
  const pending = useRef<Set<number>>(new Set());
  const full = useRef<Set<number>>(new Set());
  const lim = (id: number) => (full.current.has(id) ? 1_000_000 : PAGE);

  const load = useCallback(async (id: number, force = false) => {
    if (!force && (kids.has(id) || pending.current.has(id))) return;
    pending.current.add(id);
    try {
      const c = await api.getChildren(id, lim(id));
      setKids((m) => new Map(m).set(id, c));
    } finally {
      pending.current.delete(id);
    }
  }, [kids]);

  // (re)load on focus/version change
  useEffect(() => {
    let alive = true;
    api.getNode(focusId).then((n) => alive && setRoot(n));
    const keep = new Set([focusId, ...expanded]);
    Promise.all([...keep].map((id) => api.getChildren(id, lim(id)).then((c) => [id, c] as const).catch(() => null))).then((res) => {
      if (!alive) return;
      const m = new Map<number, NodeInfo[]>();
      res.forEach((r) => r && m.set(r[0], r[1]));
      setKids(m);
    });
    setExpanded((e) => new Set([...e, focusId]));
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusId, version]);

  useEffect(() => { if (body.current) body.current.scrollTop = 0; }, [focusId]);

  useEffect(() => {
    const el = body.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setHeight(e.contentRect.height));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const sorted = useCallback((list: NodeInfo[]) => {
    if (sort.key === "size" && sort.desc) return list;
    const k = sort.key;
    const arr = [...list].sort((a, b) => {
      const r = k === "name" ? a.name.localeCompare(b.name, "ru", { numeric: true }) : (a[k] as number) - (b[k] as number);
      return sort.desc ? -r : r;
    });
    return arr;
  }, [sort]);

  const [showAll, setShowAll] = useState<Set<number>>(new Set());
  useEffect(() => setShowAll(new Set()), [minSize, focusId]);
  const isSmall = useCallback((n: NodeInfo) => minSize != null && (n.size === 0 || n.size < minSize || (n.isDir && n.files === 0)), [minSize]);

  const rows = useMemo(() => {
    const out: Row[] = [];
    if (!root) return out;
    const walk = (n: NodeInfo, depth: number, parentSize: number) => {
      out.push({ node: n, depth, parentSize });
      if (n.isDir && expanded.has(n.id)) {
        const c = kids.get(n.id);
        if (!c) return;
        let hc = 0, hs = 0;
        const all = showAll.has(n.id);
        for (const ch of sorted(c)) {
          if (!all && isSmall(ch)) { hc++; hs += ch.size; continue; }
          walk(ch, depth + 1, n.size);
        }
        if (hc) out.push({ node: n, depth: depth + 1, parentSize: n.size, hidden: { parent: n.id, count: hc, size: hs } });
        if (c.length < n.childCount && !full.current.has(n.id)) out.push({ node: n, depth: depth + 1, parentSize: n.size, trunc: { parent: n.id, shown: c.length, total: n.childCount } });
      }
    };
    walk(root, 0, root.size);
    return out;
  }, [root, kids, expanded, sorted, isSmall, showAll]);

  // Scrolling policy:
  //  - click inside the tree: never scroll (the row is already under the cursor);
  //  - keyboard navigation: minimal scroll to keep the row visible;
  //  - selection from outside (treemap, search, details): expand ancestors and scroll once.
  // Expanding/collapsing folders or live updates never move the viewport.
  const internalSel = useRef<number | null>(null);
  const reveal = useRef<{ id: number; mode: "edge" | "center" } | null>(null);

  const scrollTo = (idx: number, mode: "edge" | "center") => {
    const el = body.current;
    if (!el) return;
    const top = idx * ROW, view = el.clientHeight - HEAD;
    if (mode === "center") { if (top < el.scrollTop || top + ROW > el.scrollTop + view) el.scrollTop = Math.max(0, top - view / 3); }
    else if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW > el.scrollTop + view) el.scrollTop = top + ROW - view;
  };
  const clickSelect = (id: number) => { internalSel.current = id; onSelect(id); };
  const keySelect = (id: number) => { internalSel.current = id; reveal.current = { id, mode: "edge" }; onSelect(id); };

  useEffect(() => {
    if (selectedId == null) return;
    if (internalSel.current === selectedId) { internalSel.current = null; return; }
    internalSel.current = null;
    reveal.current = { id: selectedId, mode: "center" };
    if (rows.some((r) => !r.hidden && !r.trunc && r.node.id === selectedId)) return;
    let alive = true;
    api.getAncestors(selectedId).then(async (anc) => {
      const ids = anc.map((a) => a.id);
      const fi = ids.indexOf(focusId);
      if (fi < 0) return;
      const chain = ids.slice(fi, -1);
      const loaded = await Promise.all(chain.filter((id) => !kids.has(id)).map((id) => api.getChildren(id, lim(id)).then((c) => [id, c] as const).catch(() => [id, [] as NodeInfo[]] as const)));
      if (!alive) return;
      if (loaded.length) setKids((m) => { const n = new Map(m); loaded.forEach(([id, c]) => n.set(id, c)); return n; });
      setExpanded((e) => new Set([...e, ...chain]));
    });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  useEffect(() => {
    const r = reveal.current;
    if (!r) return;
    const idx = rows.findIndex((x) => !x.hidden && !x.trunc && x.node.id === r.id);
    if (idx >= 0) { scrollTo(idx, r.mode); reveal.current = null; }
  }, [rows, selectedId]);

  const navRows = useMemo(() => rows.filter((r) => !r.hidden && !r.trunc), [rows]);

  const toggle = (n: NodeInfo) => {
    if (!n.isDir || n.childCount === 0) return;
    setExpanded((e) => {
      const s = new Set(e);
      if (s.has(n.id)) s.delete(n.id);
      else { s.add(n.id); load(n.id); }
      return s;
    });
  };

  const onKey = (e: React.KeyboardEvent) => {
    const rows = navRows;
    const idx = rows.findIndex((r) => r.node.id === selectedId);
    const cur = rows[idx]?.node;
    if (e.key === "ArrowDown") { e.preventDefault(); const r = rows[Math.min(rows.length - 1, idx + 1)]; r && keySelect(r.node.id); }
    else if (e.key === "ArrowUp") { e.preventDefault(); const r = rows[Math.max(0, idx - 1)]; r && keySelect(r.node.id); }
    else if (e.key === "ArrowRight" && cur?.isDir && !expanded.has(cur.id)) { e.preventDefault(); toggle(cur); }
    else if (e.key === "ArrowLeft" && cur) {
      e.preventDefault();
      if (cur.isDir && expanded.has(cur.id) && cur.id !== focusId) toggle(cur);
      else if (cur.parent != null && cur.id !== focusId) keySelect(cur.parent);
    } else if (e.key === "Enter" && cur?.isDir) onZoom(cur.id);
    else if (e.key === "Delete" && cur && cur.id !== focusId) onDelete([cur.id], e.shiftKey);
    else if (e.key === "Home") { e.preventDefault(); rows[0] && keySelect(rows[0].node.id); }
    else if (e.key === "End") { e.preventDefault(); const r = rows[rows.length - 1]; r && keySelect(r.node.id); }
    else if (e.key === "PageDown" || e.key === "PageUp") {
      e.preventDefault();
      const step = Math.max(1, Math.floor(height / ROW) - 1);
      const r = rows[Math.max(0, Math.min(rows.length - 1, idx + (e.key === "PageDown" ? step : -step)))];
      r && keySelect(r.node.id);
    }
  };

  const start = Math.max(0, Math.floor(scroll / ROW) - 8);
  const end = Math.min(rows.length, Math.ceil((scroll + height) / ROW) + 8);

  const keys = COLS.filter((c) => c.key !== "delta" || hasPrev);
  const tmpl = keys.map((c) => (c.key === "name" ? (widths.name ? `${widths.name}px` : `minmax(${NAME_MIN}px, 1fr)`) : `${widths[c.key] ?? c.w}px`)).join(" ");
  const minW = keys.reduce((s2, c) => s2 + (c.key === "name" ? widths.name ?? NAME_MIN : widths[c.key] ?? c.w), 0);

  const startResize = (e: React.MouseEvent, key: ColKey) => {
    e.preventDefault(); e.stopPropagation();
    const th = (e.currentTarget as HTMLElement).parentElement!;
    const startX = e.clientX, startW = th.getBoundingClientRect().width;
    const min = key === "name" ? 140 : 48;
    document.body.classList.add("col-resizing");
    const move = (ev: MouseEvent) => setWidths((w) => ({ ...w, [key]: Math.max(min, Math.round(startW + ev.clientX - startX)) }));
    const up = () => {
      document.body.classList.remove("col-resizing");
      window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up);
      setWidths((w) => { localStorage.setItem("ss.cols", JSON.stringify(w)); return w; });
    };
    window.addEventListener("mousemove", move); window.addEventListener("mouseup", up);
  };
  const resetWidth = (key: ColKey) => setWidths((w) => { const n = { ...w }; delete n[key]; localStorage.setItem("ss.cols", JSON.stringify(n)); return n; });

  const head = (c: (typeof COLS)[number]) => {
    const k = c.sort;
    return (
      <div key={c.key} className={`th ${c.key === "name" ? "" : "num"} ${k && sort.key === k ? "active" : ""} ${k ? "" : "nosort"}`}
        onClick={() => k && setSort((s2) => ({ key: k, desc: s2.key === k ? !s2.desc : k !== "name" }))}>
        <span className="th-label">{c.label}</span>{k && sort.key === k && <span className="sort">{sort.desc ? "↓" : "↑"}</span>}
        <span className="col-rs" title="Потяните, чтобы изменить ширину. Двойной клик — сбросить" onMouseDown={(e) => startResize(e, c.key)} onClick={(e) => e.stopPropagation()} onDoubleClick={(e) => { e.stopPropagation(); resetWidth(c.key); }} />
      </div>
    );
  };

  return (
    <div className="tree" tabIndex={0} onKeyDown={onKey} style={{ ["--cols" as string]: tmpl }}>
      <div className="tree-body" ref={body} onScroll={(e) => setScroll(Math.max(0, e.currentTarget.scrollTop))}>
        <div className="tree-inner" style={{ minWidth: minW }}>
          <div className="tree-head">{keys.map(head)}</div>
          <div style={{ height: rows.length * ROW, position: "relative" }}>
            {rows.slice(start, end).map((r, i) => {
              if (r.trunc) {
                const tr = r.trunc;
                return (
                  <div key={`t${tr.parent}`} className="tr more-row" style={{ top: (start + i) * ROW, height: ROW }}
                    onClick={() => { full.current.add(tr.parent); load(tr.parent, true); }} title="Загрузить все объекты папки">
                    <div className="td name" style={{ paddingLeft: 4 + r.depth * 18 + 26 }}>
                      <span className="nm">Показаны крупнейшие {fmtNum(tr.shown)} из {fmtNum(tr.total)}</span><span className="more-link">показать все</span>
                    </div>
                  </div>
                );
              }
              if (r.hidden) {
                const h = r.hidden;
                return (
                  <div key={`h${h.parent}`} className="tr more-row" style={{ top: (start + i) * ROW, height: ROW }}
                    onClick={() => setShowAll((s2) => new Set(s2).add(h.parent))} title="Показать скрытые в этой папке">
                    <div className="td name" style={{ paddingLeft: 4 + r.depth * 18 + 26 }}>
                      <span className="nm">Скрыто мелких: {fmtNum(h.count)} · {fmtSize(h.size)}</span><span className="more-link">показать</span>
                    </div>
                  </div>
                );
              }
              const n = r.node;
              const p = pct(n.size, r.parentSize);
              const open = expanded.has(n.id);
              const canOpen = n.isDir && n.childCount > 0;
              return (
                <div
                  key={n.id}
                  className={`tr ${n.id === selectedId ? "sel" : ""} ${r.depth === 0 ? "root" : ""}`}
                  style={{ top: (start + i) * ROW, height: ROW }}
                  onClick={() => clickSelect(n.id)}
                  onDoubleClick={(e) => {
                    if ((e.target as Element).closest(".chev")) return;
                    if (n.isDir) onZoom(n.id); else api.openItem(n.id);
                  }}
                  onContextMenu={(e) => { e.preventDefault(); clickSelect(n.id); onContext(n.id, e.clientX, e.clientY); }}
                >
                  <div className="td name" style={{ paddingLeft: 4 + r.depth * 18 }}>
                    <span
                      className={`chev ${open ? "open" : ""} ${canOpen ? "" : "hidden"}`}
                      onMouseDown={(e) => e.stopPropagation()}
                      onClick={(e) => { e.stopPropagation(); toggle(n); }}
                      onDoubleClick={(e) => { e.stopPropagation(); e.preventDefault(); }}
                    >
                      <ChevronRight size={14} />
                    </span>
                    <FileIcon isDir={n.isDir} ext={n.ext} open={open} />
                    <span className="nm" title={n.name}>{n.name}</span>
                  </div>
                  <div className="td num strong">{fmtSize(n.size)}</div>
                  <div className="td num dim">{fmtSize(n.alloc)}</div>
                  <div className="td num">
                    <div className="pctcell"><div className="bar"><i style={{ width: `${p}%` }} /></div><span>{fmtPct(p)}</span></div>
                  </div>
                  <div className="td num dim">{n.isDir ? fmtNum(n.files) : ""}</div>
                  <div className="td num dim">{fmtDate(n.modified)}</div>
                  {hasPrev && (
                    <div className={`td num delta ${n.delta == null ? "" : n.delta > 0 ? "up" : n.delta < 0 ? "down" : ""}`}>
                      {n.delta == null || Math.abs(n.delta) < 1 << 20 ? "" : fmtDelta(n.delta)}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

import { useEffect, useMemo, useState } from "react";
import { PieChart, ListOrdered, Sparkles, Copy, History, Search, ShieldCheck, AlertTriangle, ChevronDown, Trash2, Loader2 } from "lucide-react";
import type { Change, CleanupGroup, DupGroup, ExtStat, NodeInfo, SearchResult, Summary } from "../types";
import { api } from "../api";
import { CATEGORIES, categoryOf } from "../categories";
import { fmtDateTime, fmtDelta, fmtNum, fmtPct, fmtSize, pct } from "../format";
import FileRow, { splitPath } from "./FileRow";
import FileIcon from "./FileIcon";
import { Empty } from "./Common";

export interface ViewProps {
  focusId: number;
  version: number;
  selectedId: number | null;
  summary: Summary;
  onSelect: (id: number) => void;
  onLocate: (id: number) => void;
  onContext: (id: number, x: number, y: number) => void;
  onDelete: (ids: number[], permanent: boolean) => void;
}

function useLoad<T>(fn: () => Promise<T>, deps: unknown[]): [T | null, boolean] {
  const [d, setD] = useState<T | null>(null);
  const [loading, setL] = useState(true);
  useEffect(() => {
    let alive = true;
    setL(true);
    fn().then((r) => alive && (setD(r), setL(false))).catch(() => alive && setL(false));
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return [d, loading];
}

function Header({ title, sub, children }: { icon?: React.ReactNode; title: string; sub?: string; children?: React.ReactNode }) {
  return (
    <div className="vhead">
      <div className="vhead-txt"><h2>{title}</h2>{sub && <p>{sub}</p>}</div>
      <div className="vhead-act">{children}</div>
    </div>
  );
}

const Loading = () => <div className="loading"><div className="spinner" /></div>;

// ------------------------------------------------------------------ types
export function TypesView({ focusId, version }: ViewProps) {
  const [stats, loading] = useLoad<ExtStat[]>(() => api.getExtStats(focusId, 300), [focusId, version]);
  const cats = useMemo(() => {
    const m = new Map<string, { size: number; count: number }>();
    for (const s of stats ?? []) {
      const c = categoryOf(s.ext).key;
      const v = m.get(c) ?? { size: 0, count: 0 };
      v.size += s.size; v.count += s.count;
      m.set(c, v);
    }
    return CATEGORIES.map((c) => ({ ...c, ...(m.get(c.key) ?? { size: 0, count: 0 }) })).filter((c) => c.size > 0).sort((a, b) => b.size - a.size);
  }, [stats]);
  const total = cats.reduce((s, c) => s + c.size, 0);
  if (loading && !stats) return <Loading />;
  let acc = 0;
  const R = 70, C = 2 * Math.PI * R;
  const maxExt = stats?.[0]?.size ?? 1;
  return (
    <div className="view">
      <Header icon={<PieChart size={20} />} title="Типы файлов" sub={`${fmtNum(stats?.length ?? 0)} расширений · ${fmtSize(total)}`} />
      <div className="types-grid">
        <div className="card donut-card">
          <svg viewBox="0 0 200 200" className="donut">
            {cats.map((c) => {
              const len = (c.size / total) * C;
              const el = <circle key={c.key} cx="100" cy="100" r={R} stroke={c.color} strokeDasharray={`${Math.max(len - 1.5, 0.5)} ${C}`} strokeDashoffset={-acc} />;
              acc += len;
              return el;
            })}
            <text x="100" y="96" className="donut-big">{fmtSize(total)}</text>
            <text x="100" y="116" className="donut-sub">всего в файлах</text>
          </svg>
          <div className="cat-list">
            {cats.map((c) => (
              <div key={c.key} className="cat-row">
                <i style={{ background: c.color }} />
                <span className="cat-name">{c.label}</span>
                <span className="cat-count">{fmtNum(c.count)}</span>
                <span className="cat-pct">{fmtPct(pct(c.size, total))}</span>
                <b>{fmtSize(c.size)}</b>
              </div>
            ))}
          </div>
        </div>
        <div className="card ext-card">
          <div className="ext-head"><span>Расширение</span><span>Файлов</span><span>Размер</span></div>
          <div className="ext-list">
            {stats?.map((s) => {
              const c = categoryOf(s.ext);
              return (
                <div key={s.ext} className="ext-row">
                  <span className="ext-name"><i style={{ background: c.color }} />{s.ext ? `.${s.ext}` : "без расширения"}<em>{c.label}</em></span>
                  <span className="dim">{fmtNum(s.count)}</span>
                  <span className="ext-size"><span className="bar"><i style={{ width: `${(s.size / maxExt) * 100}%`, background: c.color }} /></span><b>{fmtSize(s.size)}</b></span>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ top files
export function TopFilesView({ focusId, version, selectedId, onSelect, onLocate, onContext }: ViewProps) {
  const [files, loading] = useLoad<NodeInfo[]>(() => api.getTopFiles(focusId, 500), [focusId, version]);
  if (loading && !files) return <Loading />;
  const max = files?.[0]?.size ?? 1;
  const sum = files?.reduce((s, f) => s + f.size, 0) ?? 0;
  return (
    <div className="view">
      <Header icon={<ListOrdered size={20} />} title="Самые крупные файлы" sub={`Топ-${files?.length ?? 0} · вместе ${fmtSize(sum)}`} />
      <div className="card list-card">
        {files?.map((f, i) => (
          <FileRow key={f.id} n={f} max={max} rank={i + 1} selected={f.id === selectedId} onSelect={() => onSelect(f.id)} onOpen={() => onLocate(f.id)} onContext={(x, y) => onContext(f.id, x, y)} />
        ))}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ cleanup
export function CleanupView({ focusId, version, onContext, onDelete, onSelect, onLocate }: ViewProps) {
  const [groups, loading] = useLoad<CleanupGroup[]>(() => api.getCleanup(focusId), [focusId, version]);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [open, setOpen] = useState<Set<string>>(new Set());
  // keep the user's selection across refreshes (drop only items that no longer exist)
  useEffect(() => {
    if (!groups) return;
    const present = new Set<number>();
    groups.forEach((g) => g.items.forEach((i) => present.add(i.id)));
    setChecked((c) => { const s2 = new Set([...c].filter((id) => present.has(id))); return s2.size === c.size ? c : s2; });
  }, [groups]);
  if (loading && !groups) return <Loading />;
  const sizeOf = new Map<number, number>();
  groups?.forEach((g) => g.items.forEach((i) => sizeOf.set(i.id, i.size)));
  const selSize = [...checked].reduce((s, id) => s + (sizeOf.get(id) ?? 0), 0);
  const total = groups?.reduce((s, g) => s + g.total, 0) ?? 0;
  const toggleGroup = (g: CleanupGroup, v: boolean) => setChecked((c) => { const s = new Set(c); g.items.forEach((i) => (v ? s.add(i.id) : s.delete(i.id))); return s; });
  return (
    <div className="view">
      <Header icon={<Sparkles size={20} />} title="Умная очистка" sub={total ? `Можно освободить до ${fmtSize(total)}` : "Ничего лишнего не найдено"}>
        <button className="btn danger" disabled={!checked.size} onClick={() => onDelete([...checked], false)}>
          <Trash2 size={15} /> В корзину {checked.size ? `· ${fmtSize(selSize)}` : ""}
        </button>
      </Header>
      {!groups?.length && <Empty icon={<ShieldCheck size={36} />} title="Чисто!" text="Временных файлов, кэшей и прочего мусора почти нет." />}
      <div className="clean-list">
        {groups?.map((g) => {
          const all = g.items.every((i) => checked.has(i.id));
          const isOpen = open.has(g.key);
          return (
            <div key={g.key} className={`card clean-card ${isOpen ? "open" : ""}`}>
              <div className="clean-head" onClick={() => setOpen((o) => { const s = new Set(o); s.has(g.key) ? s.delete(g.key) : s.add(g.key); return s; })}>
                <input type="checkbox" className="cb" checked={all && g.items.length > 0} onClick={(e) => e.stopPropagation()} onChange={(e) => toggleGroup(g, e.target.checked)} />
                <div className="clean-txt">
                  <div className="clean-title">{g.title} <span className={`chip ${g.risk === "safe" ? "ok" : "warn"}`}>{g.risk === "safe" ? <><ShieldCheck size={11} /> безопасно</> : <><AlertTriangle size={11} /> проверьте</>}</span></div>
                  <div className="clean-desc">{g.description}</div>
                </div>
                <div className="clean-size"><b>{fmtSize(g.total)}</b><span>{fmtNum(g.count)} объектов</span></div>
                <ChevronDown size={18} className="clean-chev" />
              </div>
              {isOpen && (
                <div className="clean-items">
                  {g.items.map((i) => {
                    const [d, n] = splitPath(i.path);
                    return (
                      <label key={i.id} className="clean-item" onDoubleClick={(e) => { e.preventDefault(); onLocate(i.id); }} onContextMenu={(e) => { e.preventDefault(); onSelect(i.id); onContext(i.id, e.clientX, e.clientY); }}>
                        <input type="checkbox" className="cb" checked={checked.has(i.id)} onChange={(e) => setChecked((c) => { const s = new Set(c); e.target.checked ? s.add(i.id) : s.delete(i.id); return s; })} />
                        <FileIcon isDir={i.isDir} ext={n.split(".").pop() ?? ""} />
                        <span className="ci-path"><span className="dim">{d}</span>{n}</span>
                        <b>{fmtSize(i.size)}</b>
                      </label>
                    );
                  })}
                  {g.count > g.items.length && <div className="dim more">…и ещё {fmtNum(g.count - g.items.length)}</div>}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ duplicates
export function DupesView({ focusId, version, onDelete, onContext, onSelect }: ViewProps) {
  const [minSize, setMinSize] = useState(10 << 20);
  const [groups, setGroups] = useState<DupGroup[] | null>(null);
  const [running, setRunning] = useState(false);
  const [prog, setProg] = useState<{ done: number; total: number; bytes: number } | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { setGroups(null); setChecked(new Set()); }, [focusId, version]);
  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => api.getDupeProgress().then(setProg), 250);
    return () => clearInterval(t);
  }, [running]);
  const run = async () => {
    setRunning(true); setErr(null); setProg(null); setChecked(new Set());
    try { setGroups(await api.findDuplicates(focusId, minSize)); }
    catch (e) { setErr(String(e)); }
    finally { setRunning(false); }
  };
  const autoSelect = () => {
    const s = new Set<number>();
    groups?.forEach((g) => g.items.slice(1).forEach((i) => s.add(i.id)));
    setChecked(s);
  };
  const sizeOf = new Map<number, number>();
  groups?.forEach((g) => g.items.forEach((i) => sizeOf.set(i.id, g.size)));
  const selSize = [...checked].reduce((s, id) => s + (sizeOf.get(id) ?? 0), 0);
  const wasted = groups?.reduce((s, g) => s + g.wasted, 0) ?? 0;
  return (
    <div className="view">
      <Header icon={<Copy size={20} />} title="Дубликаты" sub={groups ? `${fmtNum(groups.length)} групп · лишнее место ${fmtSize(wasted)}` : "Поиск одинаковых файлов по содержимому (xxHash3)"}>
        <select className="select" value={minSize} disabled={running} onChange={(e) => setMinSize(+e.target.value)}>
          <option value={1 << 20}>от 1 МБ</option>
          <option value={10 << 20}>от 10 МБ</option>
          <option value={100 << 20}>от 100 МБ</option>
          <option value={1 << 30}>от 1 ГБ</option>
        </select>
        {running ? <button className="btn" onClick={() => api.cancelDuplicates()}>Отменить</button> : <button className="btn primary" onClick={run}>Найти дубликаты</button>}
        {groups && groups.length > 0 && <button className="btn" onClick={autoSelect}>Выбрать лишние копии</button>}
        <button className="btn danger" disabled={!checked.size} onClick={() => onDelete([...checked], false)}><Trash2 size={15} /> В корзину {checked.size ? `· ${fmtSize(selSize)}` : ""}</button>
      </Header>
      {running && (
        <div className="card dup-progress">
          <Loader2 size={18} className="spin" />
          <span>Проверено {fmtNum(prog?.done ?? 0)} из {fmtNum(prog?.total ?? 0)} кандидатов · прочитано {fmtSize(prog?.bytes ?? 0)}</span>
          <div className="progress"><i style={{ width: `${prog && prog.total ? (prog.done / prog.total) * 100 : 0}%` }} /></div>
        </div>
      )}
      {err && <div className="card error">{err}</div>}
      {!running && !groups && <Empty icon={<Copy size={36} />} title="Найдите копии файлов" text="Сначала сравниваем размеры, затем хэш начала и конца файла, и только потом полный хэш. Так быстро даже на терабайтах." />}
      {!running && groups?.length === 0 && <Empty icon={<ShieldCheck size={36} />} title="Дубликатов не найдено" />}
      <div className="dup-list">
        {groups?.map((g, gi) => (
          <div key={gi} className="card dup-card">
            <div className="dup-head"><b>{fmtSize(g.size)}</b> × {g.items.length} копии <span className="dim">· лишнее {fmtSize(g.wasted)}</span></div>
            {g.items.map((i, k) => {
              const [d, n] = splitPath(i.path);
              return (
                <label key={i.id} className="clean-item" onContextMenu={(e) => { e.preventDefault(); onSelect(i.id); onContext(i.id, e.clientX, e.clientY); }}>
                  <input type="checkbox" className="cb" checked={checked.has(i.id)} onChange={(e) => setChecked((c) => { const s = new Set(c); e.target.checked ? s.add(i.id) : s.delete(i.id); return s; })} />
                  <FileIcon isDir={false} ext={n.split(".").pop()?.toLowerCase() ?? ""} />
                  <span className="ci-path"><span className="dim">{d}</span>{n}</span>
                  {k === 0 && <span className="chip accent">оставить</span>}
                </label>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ changes
export function ChangesView({ version, summary, onLocate }: ViewProps) {
  const [changes, loading] = useLoad<Change[] | null>(() => api.getChanges(300), [version]);
  if (loading && !changes) return <Loading />;
  if (!changes || summary.prevTime == null)
    return (
      <div className="view">
        <Header icon={<History size={20} />} title="Что изменилось" />
        <Empty icon={<History size={36} />} title="Это первое сканирование" text="ClearScope запомнил размеры папок. В следующий раз здесь будет видно, что выросло, а что уменьшилось." />
      </div>
    );
  const totalDelta = summary.size - (summary.prevTotal ?? summary.size);
  const max = Math.max(1, ...changes.map((c) => Math.abs(c.delta)));
  return (
    <div className="view">
      <Header icon={<History size={20} />} title="Что изменилось" sub={`С ${fmtDateTime(summary.prevTime)} · итого ${fmtDelta(totalDelta)}`} />
      <div className="card list-card">
        {changes.map((c) => {
          const [d, n] = splitPath(c.path.replace(/[\\/]$/, ""));
          return (
            <div key={c.path} className="chg-row" onDoubleClick={() => c.id != null && onLocate(c.id)}>
              <FileIcon isDir ext="" size={18} />
              <div className="frow-main"><div className="frow-name">{n || c.path}</div><div className="frow-path">{d}</div></div>
              <div className="chg-sizes dim">{fmtSize(c.oldSize)} → {fmtSize(c.newSize)}</div>
              <div className="bar chg-bar"><i className={c.delta > 0 ? "up" : "down"} style={{ width: `${(Math.abs(c.delta) / max) * 100}%` }} /></div>
              <div className={`chg-delta ${c.delta > 0 ? "up" : "down"}`}>{fmtDelta(c.delta)}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ search
export function SearchView({ focusId, version, selectedId, onSelect, onLocate, onContext, query }: ViewProps & { query: string }) {
  const [res, setRes] = useState<SearchResult | null>(null);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (!query.trim()) { setRes(null); return; }
    let alive = true;
    setLoading(true);
    const t = setTimeout(() => api.search(focusId, query, 1000).then((r) => alive && (setRes(r), setLoading(false))).catch(() => alive && setLoading(false)), 160);
    return () => { alive = false; clearTimeout(t); };
  }, [query, focusId, version]);
  const max = res?.items[0]?.size ?? 1;
  return (
    <div className="view">
      <Header icon={<Search size={20} />} title="Поиск" sub={res ? `Найдено ${fmtNum(res.total)} за ${res.tookMs} мс${res.total > res.items.length ? ` · показаны ${res.items.length} крупнейших` : ""}` : "Введите запрос в строке поиска сверху"}>
        {loading && <Loader2 size={18} className="spin" />}
      </Header>
      {!query.trim() && (
        <Empty icon={<Search size={36} />} title="Мгновенный поиск по всему диску" text="Ищет по имени среди миллионов файлов прямо в памяти.">
          <div className="hints">
            <code>отчёт 2024</code><span>все слова в имени</span>
            <code>*.mp4</code><span>по расширению</span>
            <code>*.iso &gt;1gb</code><span>расширение и размер</span>
            <code>node_modules</code><span>папки тоже ищутся</span>
          </div>
        </Empty>
      )}
      {res && res.items.length === 0 && <Empty icon={<Search size={36} />} title="Ничего не найдено" />}
      {res && res.items.length > 0 && (
        <div className="card list-card">
          {res.items.map((f) => (
            <FileRow key={f.id} n={f} max={max} selected={f.id === selectedId} onSelect={() => onSelect(f.id)} onOpen={() => onLocate(f.id)} onContext={(x, y) => onContext(f.id, x, y)} />
          ))}
        </div>
      )}
    </div>
  );
}

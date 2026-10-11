import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowUp, ChevronRight, Download, RefreshCw, Search, Shield, Zap, Footprints, X, Settings as Gear, PanelRight, LayoutDashboard, EyeOff, AlertTriangle, History } from "lucide-react";
import { api, isTauri } from "./api";
import { SMALL_STEPS, type CacheMeta, type DeleteResult, type DoneEvent, type DriveInfo, type NodeInfo, type PlatformInfo, type ProgressEvent, type Settings, type Summary, type View } from "./types";
import { fmtAgo, fmtDelta, fmtMs, fmtNum, fmtSize } from "./format";
import Sidebar, { NAV_KEYS } from "./components/Sidebar";
import Welcome from "./components/Welcome";
import ScanProgress from "./components/ScanProgress";
import TreeTable from "./components/TreeTable";
import Treemap, { Legend } from "./components/Treemap";
import DetailsPanel from "./components/DetailsPanel";
import { ChangesView, CleanupView, DupesView, SearchView, TopFilesView, TypesView, type ViewProps } from "./components/Views";
import { useStartupUpdateCheck, useUpdater } from "./updater";
import { ContextMenu, DeleteDialog, SettingsPop, Toasts, type DeleteReq, type MenuAction, type MenuState, type Toast } from "./components/Common";

const DEFAULTS: Settings = { theme: "system", live: true, openLast: true, details: true, hideSmall: false, smallStep: 3, alwaysAdmin: false, autoUpdate: true };
function loadSettings(): Settings {
  try { return { ...DEFAULTS, ...JSON.parse(localStorage.getItem("ss.settings") ?? "{}") }; } catch { return DEFAULTS; }
}
const norm = (p: string) => p.toLowerCase().replace(/[\\/]+$/, "");

export default function App() {
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [cached, setCached] = useState<CacheMeta[]>([]);
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const [booted, setBooted] = useState(false);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanFromCache, setScanFromCache] = useState(false);
  const [scanPath, setScanPath] = useState("");
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [view, setView] = useState<View>("overview");
  const [focusId, setFocusId] = useState(0);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [version, setVersion] = useState(0);
  // live file-system changes bump liveTick: the tree follows immediately,
  // the (heavier) treemap is refreshed at most every few seconds
  const [liveTick, setLiveTick] = useState(0);
  const [mapTick, setMapTick] = useState(0);
  const mapLast = useRef(0);
  const [crumbs, setCrumbs] = useState<NodeInfo[]>([]);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [del, setDel] = useState<DeleteReq | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [query, setQuery] = useState("");
  const [split, setSplit] = useState(() => Math.min(0.85, Math.max(0.15, +(localStorage.getItem("ss.split") ?? 0.5) || 0.5)));
  const [mapOpen, setMapOpen] = useState(() => localStorage.getItem("ss.map") !== "0");
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [settingsAt, setSettingsAt] = useState<DOMRect | null>(null);
  const [sysDark, setSysDark] = useState(() => window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? true);
  const searchRef = useRef<HTMLInputElement>(null);
  const splitRef = useRef<HTMLDivElement>(null);
  const t0 = useRef(0);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  const theme = settings.theme === "system" ? (sysDark ? "dark" : "light") : settings.theme;
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    api.setWindowTheme(settings.theme === "system" ? null : settings.theme);
  }, [theme, settings.theme]);
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!mq) return;
    const h = (e: MediaQueryListEvent) => setSysDark(e.matches);
    mq.addEventListener("change", h);
    return () => mq.removeEventListener("change", h);
  }, []);

  const updateSettings = (p: Partial<Settings>) => {
    const next = { ...settings, ...p };
    setSettings(next);
    localStorage.setItem("ss.settings", JSON.stringify(next));
    if (p.live !== undefined && summary) api.setLive(p.live).then(() => api.getSummary()).then((s) => s && setSummary((o) => ({ ...s, fallbackReason: o?.fallbackReason ?? null })));
  };

  const toast = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4500);
  }, []);
  const updater = useUpdater(toast);
  useStartupUpdateCheck(settings.autoUpdate, booted, updater.check);

  const refreshDrives = useCallback(() => {
    api.listDrives().then(setDrives).catch(() => {});
    api.listCached().then(setCached).catch(() => {});
  }, []);

  const scan = useCallback(async (path: string) => {
    setScanPath(path); setProgress(null); setScanFromCache(false); setScanning(true);
    t0.current = performance.now();
    try { await api.startScan(path, true); }
    catch (e) { setScanning(false); toast(String(e), "err"); }
  }, [toast]);

  const openCached = useCallback(async (path: string) => {
    setScanPath(path); setProgress(null); setScanFromCache(true); setScanning(true);
    t0.current = performance.now();
    try { await api.openCached(path); }
    catch (e) { setScanning(false); toast(String(e), "err"); }
  }, [toast]);

  const scanRef = useRef(scan);
  scanRef.current = scan;

  // startup + backend events
  useEffect(() => {
    api.listDrives().then(setDrives).catch(() => {});
    (async () => {
      const pf = await api.platformInfo().catch(() => null);
      setPlatform(pf);
      if (settingsRef.current.alwaysAdmin && pf?.canElevate && !pf.elevated) {
        const t = await api.startupTarget().catch(() => null);
        try { await api.restartAsAdmin(t ?? undefined); return; } catch { /* UAC declined: continue normally */ }
      }
      setBooted(true);
      const [list, target, cur] = await Promise.all([
        api.listCached().catch(() => [] as CacheMeta[]),
        api.startupTarget().catch(() => null),
        api.getSummary().catch(() => null),
      ]);
      setCached(list);
      if (cur) { setSummary(cur); setFocusId(cur.rootId); return; }
      if (target) { scanRef.current(target); return; }
      if (settingsRef.current.openLast && list.length) {
        const last = [...list].sort((a, b) => b.savedAt - a.savedAt)[0];
        openCached(last.rootPath);
      }
    })();
    const un: Promise<() => void>[] = [
      api.onProgress((p) => setProgress(p)),
      api.onDone((d: DoneEvent) => {
        setScanning(false);
        if (d.ok && d.summary) {
          const s = d.summary;
          setSummary(s);
          setFocusId(s.rootId);
          setSelectedId(null);
          setView("overview");
          setVersion((v) => v + 1);
          const took = fmtMs(Math.round(performance.now() - t0.current));
          if (s.fallbackReason) toast(s.fallbackReason, "info");
          else if (s.fromCache) toast(`Открыто за ${took} · данные ${s.stale ? `от ${fmtAgo(s.capturedAt)}` : "актуальны"}`, "ok");
          else toast(`Готово за ${fmtMs(s.scanMs)}: ${fmtNum(s.files)} файлов`, "ok");
          if (!settingsRef.current.live) api.setLive(false).then(() => api.getSummary()).then((x) => x && setSummary({ ...x, fallbackReason: s.fallbackReason }));
        } else if (d.error) toast(`Не удалось: ${d.error}`, "err");
        refreshDrives();
      }),
      api.onTreeChanged((s) => {
        setSummary((o) => ({ ...s, fallbackReason: o?.fallbackReason ?? null }));
        setLiveTick((v) => v + 1);
      }),
      api.onLiveStatus((live) => setSummary((o) => (o ? { ...o, live: live as Summary["live"] } : o))),
      api.onLiveResync(() => {
        api.getSummary().then((s) => {
          if (!s) return;
          toast("Слишком много изменений на диске — обновляю результаты целиком", "info");
          scanRef.current(s.rootPath);
        });
      }),
    ];
    return () => { un.forEach((p) => p.then((f) => f())); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!liveTick) return;
    const wait = Math.max(0, 4000 - (Date.now() - mapLast.current));
    const t = setTimeout(() => { mapLast.current = Date.now(); setMapTick(liveTick); }, wait);
    return () => clearTimeout(t);
  }, [liveTick]);
  const treeVersion = version + liveTick;
  const mapVersion = version + mapTick;

  useEffect(() => {
    if (!summary) return;
    api.getAncestors(focusId).then(setCrumbs).catch(() => {
      // focused folder vanished (live update) — fall back to root
      setCrumbs([]); setFocusId(summary.rootId);
    });
  }, [focusId, summary?.rootId, treeVersion]); // eslint-disable-line react-hooks/exhaustive-deps

  // selected node removed by a live update → drop selection
  useEffect(() => {
    if (selectedId == null) return;
    api.getNode(selectedId).catch(() => setSelectedId(null));
  }, [treeVersion]); // eslint-disable-line react-hooks/exhaustive-deps

  const openDrive = (path: string) => {
    if (scanning) return;
    if (summary && norm(summary.rootPath) === norm(path)) { setView("overview"); setFocusId(summary.rootId); return; }
    if (cached.some((c) => norm(c.rootPath) === norm(path))) openCached(path); else scan(path);
  };
  const pickFolder = async () => { const p = await api.pickFolder(); if (p) scan(p); };
  const elevate = (target?: string) => api.restartAsAdmin(target).catch((e) => toast(String(e), "err"));

  const askDelete = useCallback((ids: number[], permanent: boolean, contents?: boolean) => { if (ids.length) setDel({ ids, permanent, contents }); }, []);
  const onDeleted = (r: DeleteResult) => {
    const n = r.deleted.length + r.cleared;
    if (n) toast(`Освобождено ${fmtSize(r.freed)} (${fmtNum(n)} шт.)`, "ok");
    if (r.left > 0) toast(`Часть файлов занята другими программами, осталось ${fmtSize(r.left)}. Закройте игры и браузеры и повторите.`, "info");
    else if (r.failed.length) toast(`Не удалось удалить ${r.failed.length}: ${r.failed[0].error}`, "err");
    if (selectedId != null && r.deleted.includes(selectedId)) setSelectedId(null);
    setVersion((v) => v + 1);
    api.getSummary().then((s) => s && setSummary((o) => ({ ...s, fallbackReason: o?.fallbackReason ?? null })));
    api.listDrives().then(setDrives).catch(() => {});
  };

  const openMenu = useCallback((id: number, x: number, y: number) => {
    api.getNode(id).then((n) => setMenu({ id, x, y, isDir: n.isDir, isRoot: id === focusId }));
  }, [focusId]);

  const locate = useCallback((id: number) => {
    if (!summary) return;
    api.getAncestors(id).then((anc) => {
      if (!anc.some((a) => a.id === focusId)) setFocusId(summary.rootId);
      setView("overview");
      setSelectedId(id);
    });
  }, [focusId, summary]);

  const zoom = (id: number) => { setFocusId(id); setSelectedId(null); };
  const goUp = () => { if (crumbs.length > 1) { setSelectedId(focusId); setFocusId(crumbs[crumbs.length - 2].id); } };
  const copyPath = async (id: number) => {
    const n = await api.getNode(id);
    await navigator.clipboard.writeText(n.path ?? n.name).catch(() => {});
    toast("Путь скопирован", "ok");
  };

  const onMenu = (a: MenuAction) => {
    if (!menu) return;
    const id = menu.id;
    switch (a) {
      case "open": api.openItem(id); break;
      case "reveal": api.revealItem(id); break;
      case "copy": copyPath(id); break;
      case "locate": locate(id); break;
      case "zoom": zoom(id); break;
      case "up": goUp(); break;
      case "trash": askDelete([id], false); break;
      case "delete": askDelete([id], true); break;
    }
  };

  const exportCsv = async () => {
    const p = await api.pickSaveCsv();
    if (!p) { if (!isTauri) toast("Экспорт доступен в приложении", "info"); return; }
    try { const n = await api.exportCsv(focusId, p); toast(`Экспортировано ${fmtNum(n)} строк`, "ok"); }
    catch (e) { toast(String(e), "err"); }
  };

  const toggleLive = () => updateSettings({ live: summary?.live === "paused" });

  // keyboard shortcuts
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      const inInput = tag === "INPUT" || tag === "SELECT";
      if (e.key === "F5") e.preventDefault();
      if (scanning) { if (e.key === "Escape") api.cancelScan(); return; }
      if (del || menu || updater.open) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") { e.preventDefault(); searchRef.current?.focus(); searchRef.current?.select(); }
      else if (e.key === "F5" && summary) { e.preventDefault(); scan(summary.rootPath); }
      else if (e.key === "Escape" && inInput) { setQuery(""); (e.target as HTMLElement).blur(); if (view === "search") setView("overview"); }
      else if (inInput) return;
      else if (e.key === "Backspace" && summary) { e.preventDefault(); goUp(); }
      else if (e.altKey && /^[1-6]$/.test(e.key) && summary) { e.preventDefault(); setView(NAV_KEYS[+e.key - 1]); }
      else if (e.key.toLowerCase() === "i" && !e.ctrlKey && summary) updateSettings({ details: !settings.details });
      else if (e.key.toLowerCase() === "m" && !e.ctrlKey && summary) showMap(!mapOpen);
      else if (e.key.toLowerCase() === "h" && !e.ctrlKey && summary) { updateSettings({ hideSmall: !settings.hideSmall }); toast(settings.hideSmall ? "Показаны все объекты" : `Скрыто: ${SMALL_STEPS[settings.smallStep].label}`, "info"); }
      else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c" && selectedId != null && !window.getSelection()?.toString()) copyPath(selectedId);
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });

  const showMap = (v: boolean) => { setMapOpen(v); localStorage.setItem("ss.map", v ? "1" : "0"); };
  // Drag the splitter: down past ~90% hides the treemap completely, dragging back up restores it.
  const startDrag = (e: React.MouseEvent) => {
    e.preventDefault();
    const box = splitRef.current!.getBoundingClientRect();
    let last = split, open = mapOpen;
    document.body.classList.add("row-resizing");
    const move = (ev: MouseEvent) => {
      const r = (ev.clientY - box.top) / box.height;
      if (r > 0.9) { open = false; setMapOpen(false); }
      else { open = true; setMapOpen(true); last = Math.min(0.85, Math.max(0.15, r)); setSplit(last); }
    };
    const up = () => {
      document.body.classList.remove("row-resizing");
      localStorage.setItem("ss.split", String(last));
      localStorage.setItem("ss.map", open ? "1" : "0");
      window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const vp: ViewProps | null = summary && {
    // heavy views (cleanup, types, duplicates…) refresh on scans/deletions, not on every live tick
    // they follow live changes too, throttled like the treemap
    focusId, version: view === "dupes" ? version : mapVersion, selectedId, summary,
    onSelect: setSelectedId, onLocate: locate, onContext: openMenu, onDelete: askDelete,
  };
  const needAdmin = platform?.canElevate && !platform.elevated;
  const totalDelta = summary?.prevTotal != null ? summary.size - summary.prevTotal : null;
  const live = summary?.live ?? "off";
  const liveLabel = live === "usn" ? "Живое обновление · журнал NTFS" : live === "watch" ? "Живое обновление" : live === "paused" ? "Обновление на паузе" : "Без живого обновления";

  return (
    <div className="app">
      <Sidebar drives={drives} summary={summary} view={view} scanning={scanning} onView={setView} onOpen={openDrive} onPickFolder={pickFolder} />
      <main className="main">
        {summary && !scanning && (
          <header className="topbar">
            <button className="icon-btn" title="На уровень выше (Backspace)" disabled={crumbs.length < 2} onClick={goUp}><ArrowUp size={16} /></button>
            <div className="crumbs">
              {crumbs.map((c, i) => (
                <span key={c.id} className="crumb-wrap">
                  {i > 0 && <ChevronRight size={14} className="crumb-sep" />}
                  <button className={`crumb ${i === crumbs.length - 1 ? "last" : ""}`} onClick={() => zoom(c.id)} title={c.name}>
                    {i === 0 ? c.name.replace(/\\$/, "") : c.name}
                  </button>
                </span>
              ))}
            </div>
            <div className="search">
              <Search size={15} />
              <input ref={searchRef} placeholder="Поиск: имя, *.mp4, >1gb" value={query}
                onChange={(e) => { setQuery(e.target.value); if (e.target.value) setView("search"); }}
                onFocus={() => query && setView("search")} />
              {query ? <button className="clear" onClick={() => setQuery("")}><X size={14} /></button> : <kbd>Ctrl+F</kbd>}
            </div>
            {needAdmin && <button className="btn" title="Перезапустить с правами администратора для чтения MFT" onClick={() => elevate(summary.rootPath)}><Shield size={15} /> Быстрый режим</button>}
            <button className="btn" title="Полное пересканирование (F5)" onClick={() => scan(summary.rootPath)}><RefreshCw size={15} /> Пересканировать</button>
            <button className="icon-btn" title="Экспорт в CSV" onClick={exportCsv}><Download size={16} /></button>
            {view === "overview" && <button className={`icon-btn ${mapOpen ? "on" : ""}`} title="Карта (M)" onClick={() => showMap(!mapOpen)}><LayoutDashboard size={16} /></button>}
            {view === "overview" && <button className={`icon-btn ${settings.details ? "on" : ""}`} title="Панель сведений (I)" onClick={() => updateSettings({ details: !settings.details })}><PanelRight size={16} /></button>}
            <button className={`icon-btn ${settingsAt ? "on" : ""}`} data-anchor title="Настройки" onClick={(e) => setSettingsAt(settingsAt ? null : e.currentTarget.getBoundingClientRect())}><Gear size={16} /></button>
          </header>
        )}
        {!summary && !scanning && (
          <header className="topbar" style={{ justifyContent: "flex-end", borderBottom: 0 }}>
            <button className={`icon-btn ${settingsAt ? "on" : ""}`} data-anchor title="Настройки" onClick={(e) => setSettingsAt(settingsAt ? null : e.currentTarget.getBoundingClientRect())}><Gear size={16} /></button>
          </header>
        )}

        {summary && !scanning && summary.stale && (
          <div className="notice">
            <History size={15} />
            <span>Показаны сохранённые данные от {fmtAgo(summary.capturedAt)}. Изменения после этого не учтены{needAdmin ? " — без прав администратора журнал NTFS недоступен" : ""}.</span>
            <button className="btn sm" onClick={() => scan(summary.rootPath)}>Обновить</button>
          </div>
        )}
        {summary && !scanning && !summary.stale && summary.fallbackReason && (
          <div className="notice">
            <AlertTriangle size={15} />
            <span>{summary.fallbackReason}</span>
            {needAdmin && <button className="btn sm" onClick={() => elevate(summary.rootPath)}>Перезапустить как администратор</button>}
          </div>
        )}

        <section className="content">
          {scanning ? (
            <ScanProgress path={scanPath} p={progress} cached={scanFromCache} onCancel={() => api.cancelScan()} />
          ) : !summary || !vp ? (
            <Welcome drives={drives} cached={cached} platform={platform} onScan={scan} onOpenCached={openCached} onPickFolder={pickFolder} onElevate={() => elevate()} />
          ) : view === "overview" ? (
            <div className="overview">
              <div className="ov-main" ref={splitRef}>
                <div style={{ flex: mapOpen ? `${split} 1 0` : "1 1 0" }} className="pane">
                  <TreeTable focusId={focusId} selectedId={selectedId} version={treeVersion} hasPrev={summary.prevTime != null}
                    minSize={settings.hideSmall ? SMALL_STEPS[settings.smallStep]?.bytes ?? 0 : null}
                    onSelect={setSelectedId} onZoom={zoom} onContext={openMenu} onDelete={askDelete} />
                </div>
                <div className={`splitter ${mapOpen ? "" : "closed"}`} onMouseDown={startDrag} onDoubleClick={() => showMap(!mapOpen)}
                  title={mapOpen ? "Потяните вниз до конца, чтобы скрыть карту. Двойной клик — скрыть" : "Потяните вверх или дважды щёлкните, чтобы показать карту"}>
                  <i />{!mapOpen && <span>Карта скрыта · потяните вверх</span>}
                </div>
                {mapOpen && (
                  <>
                    <div style={{ flex: `${1 - split} 1 0` }} className="pane">
                      <Treemap focusId={focusId} selectedId={selectedId} version={mapVersion} theme={theme} onSelect={setSelectedId}
                        onZoom={(id) => api.getNode(id).then((n) => (n.isDir ? zoom(id) : api.openItem(id)))} onContext={openMenu} />
                    </div>
                    <Legend />
                  </>
                )}
              </div>
              {settings.details && (
                <DetailsPanel id={selectedId ?? focusId} isFocus={selectedId == null || selectedId === focusId} version={mapVersion}
                  onSelect={setSelectedId} onZoom={zoom} onDelete={(ids) => askDelete(ids, false)} onCopied={() => toast("Путь скопирован", "ok")} />
              )}
            </div>
          ) : view === "types" ? <TypesView {...vp} />
            : view === "top" ? <TopFilesView {...vp} />
            : view === "cleanup" ? <CleanupView {...vp} />
            : view === "dupes" ? <DupesView {...vp} />
            : view === "changes" ? <ChangesView {...vp} />
            : <SearchView {...vp} query={query} />}
        </section>

        {summary && !scanning && (
          <footer className="status">
            <span className="chip" title={summary.method === "mft" ? "Прочитано напрямую из MFT" : "Обычный обход папок"}>
              {summary.method === "mft" ? <><Zap size={11} /> MFT</> : <><Footprints size={11} /> Обход</>} · {fmtMs(summary.scanMs)}
            </span>
            <button className={`chip ${live === "usn" || live === "watch" ? "ok" : ""}`} title="Нажмите, чтобы приостановить или возобновить" onClick={toggleLive} disabled={live === "off"}>
              <span className={`live-dot ${live === "usn" || live === "watch" ? "" : "off"}`} />{liveLabel}
            </button>
            <span><b>{fmtSize(summary.size)}</b> в файлах</span>
            <span>{fmtSize(summary.alloc)} на диске</span>
            <span>{fmtNum(summary.files)} файлов · {fmtNum(summary.dirs)} папок</span>
            {totalDelta != null && Math.abs(totalDelta) >= 1 << 20 && <span className={totalDelta > 0 ? "up" : "down"}>{fmtDelta(totalDelta)} с прошлого раза</span>}
            <span className="grow" />
            {settings.hideSmall && (
              <button className="chip accent" title="Нажмите, чтобы показать всё (H)" onClick={() => updateSettings({ hideSmall: false })}>
                <EyeOff size={11} /> Скрыты: {SMALL_STEPS[settings.smallStep].label}
              </button>
            )}
            <span className="dim">{summary.fromCache ? `Снимок от ${fmtAgo(summary.capturedAt)}` : `Сканировано ${fmtAgo(summary.capturedAt)}`}</span>
          </footer>
        )}
      </main>
      {menu && <ContextMenu m={menu} onAction={onMenu} onClose={() => setMenu(null)} />}
      {del && <DeleteDialog req={del} onClose={() => setDel(null)} onDone={onDeleted} />}
      {settingsAt && <SettingsPop s={settings} anchor={settingsAt} live={live === "usn" || live === "watch"} canElevate={!!platform?.canElevate} onChange={updateSettings} onCheckUpdates={() => { setSettingsAt(null); updater.check(true); }} onClose={() => setSettingsAt(null)} />}
      {updater.ui}
      <Toasts items={toasts} />
    </div>
  );
}

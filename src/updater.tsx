import { useCallback, useEffect, useRef, useState } from "react";
import { Download, Sparkles } from "lucide-react";
import { isTauri } from "./api";
import { fmtSize } from "./format";

// Updates are published as GitHub releases (see .github/workflows/release.yml).
// "Обновить позже" remembers the version, and the next launch installs it without asking.
const PENDING = "ss.pendingUpdate";

type Update = import("@tauri-apps/plugin-updater").Update;
type Phase = { kind: "idle" } | { kind: "offer"; u: Update } | { kind: "install"; u: Update; got: number; total: number | null; auto: boolean } | { kind: "error"; u: Update; msg: string };

export function useUpdater(toast: (t: string, k?: "ok" | "err" | "info") => void) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const busy = useRef(false);

  const install = useCallback(async (u: Update, auto: boolean) => {
    setPhase({ kind: "install", u, got: 0, total: null, auto });
    try {
      let got = 0, total: number | null = null;
      await u.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? null;
        else if (e.event === "Progress") got += e.data.chunkLength;
        setPhase({ kind: "install", u, got, total, auto });
      });
      localStorage.removeItem(PENDING);
      // Windows: the installer closes the app itself and starts the new version
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      setPhase({ kind: "error", u, msg: String(e) });
    }
  }, []);

  const check = useCallback(async (manual: boolean) => {
    if (!isTauri) { if (manual) toast("Обновления работают только в приложении", "info"); return; }
    if (busy.current) return;
    busy.current = true;
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const u = await check();
      if (!u) {
        localStorage.removeItem(PENDING);
        if (manual) toast(`У вас последняя версия — ${__APP_VERSION__}`, "ok");
        return;
      }
      if (!manual && localStorage.getItem(PENDING)) install(u, true);
      else setPhase({ kind: "offer", u });
    } catch (e) {
      if (manual) toast(`Не удалось проверить обновления: ${e}`, "err");
    } finally {
      busy.current = false;
    }
  }, [install, toast]);

  const later = () => {
    if (phase.kind === "offer" || phase.kind === "error") localStorage.setItem(PENDING, phase.u.version);
    setPhase({ kind: "idle" });
  };

  const ui = <UpdateDialog phase={phase} onUpdate={(u) => install(u, false)} onLater={later} />;
  return { check, ui, open: phase.kind !== "idle" };
}

/** Startup check, a little after the window shows up so it doesn't compete with opening the last scan. */
export function useStartupUpdateCheck(enabled: boolean, ready: boolean, check: (manual: boolean) => void) {
  const done = useRef(false);
  useEffect(() => {
    if (!ready || done.current) return;
    done.current = true;
    if (!enabled && !localStorage.getItem(PENDING)) return;
    const t = setTimeout(() => check(false), 1500);
    return () => clearTimeout(t);
  }, [enabled, ready, check]);
}

function notes(body?: string) {
  if (!body) return null;
  const lines = body.split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !/^#+\s*$/.test(l) && !/^\*\*full changelog/i.test(l));
  if (!lines.length) return null;
  return (
    <div className="upd-notes">
      {lines.slice(0, 14).map((l, i) => {
        const h = l.match(/^#+\s*(.*)/);
        if (h) return <b key={i}>{h[1]}</b>;
        const li = l.match(/^[-*•]\s*(.*)/);
        return li ? <div key={i} className="li">{li[1]}</div> : <div key={i}>{l}</div>;
      })}
    </div>
  );
}

function UpdateDialog({ phase, onUpdate, onLater }: { phase: Phase; onUpdate: (u: Update) => void; onLater: () => void }) {
  if (phase.kind === "idle") return null;
  const u = phase.u;
  if (phase.kind === "install") {
    const pct = phase.total ? Math.min(100, (phase.got / phase.total) * 100) : null;
    return (
      <div className="modal-bg">
        <div className="modal upd">
          <div className="upd-ic"><Download size={22} /></div>
          <h3>{phase.auto ? `Устанавливаем ClearScope ${u.version}` : `Загружаем ClearScope ${u.version}`}</h3>
          <p>{phase.auto ? "Вы отложили это обновление в прошлый раз. Пара секунд — и приложение перезапустится." : "После загрузки приложение само перезапустится."}</p>
          <div className={`upd-bar ${pct == null ? "busy" : ""}`}><i style={{ width: pct == null ? undefined : `${pct}%` }} /></div>
          <div className="upd-sub">{phase.got ? `${fmtSize(phase.got)}${phase.total ? ` из ${fmtSize(phase.total)}` : ""}` : "Подключаемся к GitHub…"}</div>
        </div>
      </div>
    );
  }
  const failed = phase.kind === "error";
  return (
    <div className="modal-bg">
      <div className="modal upd" onMouseDown={(e) => e.stopPropagation()}>
        <div className="upd-ic"><Sparkles size={22} /></div>
        <h3>{failed ? "Обновление не установилось" : "Доступна новая версия"}</h3>
        <p>
          {failed
            ? <>Не получилось скачать или установить ClearScope {u.version}. {phase.msg}</>
            : <>ClearScope <b>{u.version}</b> уже на GitHub. У вас сейчас {__APP_VERSION__}.</>}
        </p>
        {!failed && notes(u.body)}
        <div className="modal-actions">
          <button className="btn" onClick={onLater}>Обновить позже</button>
          <button className="btn primary" autoFocus onClick={() => onUpdate(u)}>{failed ? "Попробовать ещё раз" : "Обновить"}</button>
        </div>
        {!failed && <div className="upd-hint">Если отложить, обновление установится само при следующем запуске.</div>}
      </div>
    </div>
  );
}

import { useEffect, useRef, useState } from "react";
import { ExternalLink, FolderOpen, Copy, Crosshair, ZoomIn, Trash2, Flame, ArrowUpFromLine, ShieldX, AlertTriangle, CheckCircle2 } from "lucide-react";
import { SMALL_STEPS, type DeleteCheck, type DeleteResult, type Settings, type Theme } from "../types";
import { api } from "../api";
import { fmtNum, fmtSize } from "../format";

function useOutside(ref: React.RefObject<HTMLElement | null>, onClose: () => void) {
  useEffect(() => {
    const h = (e: MouseEvent) => { const t = e.target as Element; if (!ref.current?.contains(t) && !t.closest?.("[data-anchor]")) onClose(); };
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", h);
    window.addEventListener("keydown", k);
    return () => { window.removeEventListener("mousedown", h); window.removeEventListener("keydown", k); };
  }, [ref, onClose]);
}

// ------------------------------------------------------------------ context menu
export interface MenuState { id: number; x: number; y: number; isDir: boolean; isRoot: boolean }
export type MenuAction = "open" | "reveal" | "copy" | "locate" | "zoom" | "up" | "trash" | "delete";

export function ContextMenu({ m, onAction, onClose }: { m: MenuState; onAction: (a: MenuAction) => void; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useOutside(ref, onClose);
  const x = Math.min(m.x, window.innerWidth - 240), y = Math.min(m.y, window.innerHeight - 330);
  const item = (a: MenuAction, label: string, Icon: typeof Copy, key?: string, cls = "") => (
    <button className={`menu-item ${cls}`} onClick={() => { onAction(a); onClose(); }}><Icon size={15} />{label}{key && <kbd>{key}</kbd>}</button>
  );
  return (
    <div className="menu" ref={ref} style={{ left: x, top: y }}>
      {item("open", "Открыть", ExternalLink)}
      {item("reveal", "Показать в проводнике", FolderOpen)}
      {item("copy", "Копировать путь", Copy, "Ctrl+C")}
      <div className="menu-sep" />
      {item("locate", "Показать в дереве", Crosshair)}
      {m.isDir && !m.isRoot && item("zoom", "Открыть как корень", ZoomIn, "Enter")}
      {m.isRoot && item("up", "На уровень выше", ArrowUpFromLine, "Backspace")}
      {!m.isRoot && (
        <>
          <div className="menu-sep" />
          {item("trash", "Удалить в корзину…", Trash2, "Del", "danger")}
          {item("delete", "Удалить навсегда…", Flame, "Shift+Del", "danger")}
        </>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ delete dialog (with safety checks)
export interface DeleteReq { ids: number[]; permanent: boolean; contents?: boolean }

export function DeleteDialog({ req, onClose, onDone }: { req: DeleteReq; onClose: () => void; onDone: (r: DeleteResult) => void }) {
  const [items, setItems] = useState<DeleteCheck[] | null>(null);
  const [permanent, setPermanent] = useState(req.permanent);
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.checkDelete(req.ids, !!req.contents).then(setItems).catch((e) => setErr(String(e))); }, [req]);
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose, busy]);

  const blocked = items?.filter((i) => i.level === "blocked") ?? [];
  const warn = items?.filter((i) => i.level === "warn") ?? [];
  const ok = items?.filter((i) => i.level === "ok") ?? [];
  const willDelete = [...ok, ...(ack ? warn : [])];
  const size = willDelete.reduce((s, i) => s + i.size, 0);

  const run = async () => {
    setBusy(true);
    try { onDone(await api.deleteItems(willDelete.map((i) => i.id), permanent, ack, !!req.contents)); onClose(); }
    catch (e) { setErr(String(e)); setBusy(false); }
  };

  const group = (list: DeleteCheck[], cls: string, Icon: typeof ShieldX, title: string) => list.length > 0 && (
    <>
      <div className="del-group-title"><Icon size={14} style={{ color: `var(--${cls === "blocked" ? "danger" : cls === "warn" ? "warn" : "ok"})` }} />{title} · {fmtNum(list.length)}</div>
      {list.slice(0, 40).map((i) => (
        <div key={i.id} className={`del-item ${cls}`}>
          <span className="p" title={i.path}>{i.path}</span><b>{fmtSize(i.size)}</b>
          {i.reason && <span className="r">{i.reason}</span>}
        </div>
      ))}
      {list.length > 40 && <div className="dim more">…и ещё {fmtNum(list.length - 40)}</div>}
    </>
  );

  return (
    <div className="modal-bg" onMouseDown={() => !busy && onClose()}>
      <div className="modal" style={{ width: 560 }} onMouseDown={(e) => e.stopPropagation()}>
        <h3>{permanent ? "Удалить навсегда?" : "Переместить в корзину?"}</h3>
        <p>{permanent ? "Файлы будут удалены без возможности восстановления." : "Файлы можно будет восстановить из корзины."}</p>
        {err && <div className="guard blocked"><ShieldX size={15} />{err}</div>}
        {!items && !err && <div className="loading" style={{ height: 80 }}><div className="spinner" /></div>}
        {items && (
          <div className="del-list">
            {group(blocked, "blocked", ShieldX, "Защищено — не будет удалено")}
            {group(warn, "warn", AlertTriangle, "Требует подтверждения")}
            {group(ok, "ok", CheckCircle2, "Будет удалено")}
          </div>
        )}
        {warn.length > 0 && (
          <label className="ack">
            <input type="checkbox" className="cb" checked={ack} onChange={(e) => setAck(e.target.checked)} />
            <span>Я понимаю, что удаление отмеченных предупреждением объектов может сломать программы или потерять данные.</span>
          </label>
        )}
        <label className="switch-row" style={{ marginTop: 12 }} onClick={(e) => { e.preventDefault(); setPermanent((v) => !v); }}>
          <span>Удалить навсегда, минуя корзину</span>
          <span className={`switch ${permanent ? "on" : ""}`} />
        </label>
        <div className="modal-actions">
          <button className="btn" onClick={onClose} disabled={busy}>Отмена</button>
          <button className={`btn danger ${permanent ? "solid" : ""}`} autoFocus disabled={busy || !willDelete.length} onClick={run}>
            {permanent ? <Flame size={15} /> : <Trash2 size={15} />}
            {busy ? "Удаляю…" : `${permanent ? "Удалить" : "В корзину"}${willDelete.length ? ` · ${fmtSize(size)}` : ""}`}
          </button>
        </div>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ settings popover
export function SettingsPop({ s, anchor, live, canElevate, onChange, onCheckUpdates, onClose }: {
  s: Settings; anchor: DOMRect; live: boolean; canElevate: boolean; onChange: (p: Partial<Settings>) => void; onCheckUpdates: () => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useOutside(ref, onClose);
  const sw = (on: boolean, label: string, hint: string, set: () => void) => (
    <div className="switch-row" onClick={set}>
      <span>{label}<small>{hint}</small></span>
      <span className={`switch ${on ? "on" : ""}`} />
    </div>
  );
  const themes: [Theme, string][] = [["system", "Системная"], ["light", "Светлая"], ["dark", "Тёмная"]];
  return (
    <div className="pop" ref={ref} style={{ top: anchor.bottom + 6, left: Math.max(8, anchor.right - 280) }}>
      <div className="pop-title">Настройки</div>
      <div className="pop-label">Тема</div>
      <div className="seg">{themes.map(([k, l]) => <button key={k} className={s.theme === k ? "on" : ""} onClick={() => onChange({ theme: k })}>{l}</button>)}</div>
      {sw(s.live, "Живое обновление", live ? "Изменения на диске применяются сразу" : "Следить за изменениями без пересканирования", () => onChange({ live: !s.live }))}
      {sw(s.openLast, "Открывать последний результат", "При запуске сразу показывать сохранённое", () => onChange({ openLast: !s.openLast }))}
      {sw(s.details, "Панель сведений", "Справа в «Обзоре» (клавиша I)", () => onChange({ details: !s.details }))}
      <div className="pop-label">Список</div>
      {sw(s.hideSmall, "Скрывать мелкие и пустые", "Не показывать в дереве лишнюю мелочь (клавиша H)", () => onChange({ hideSmall: !s.hideSmall }))}
      <div className={`range-row ${s.hideSmall ? "" : "off"}`}>
        <input type="range" min={0} max={SMALL_STEPS.length - 1} step={1} value={s.smallStep} disabled={!s.hideSmall}
          onChange={(e) => onChange({ smallStep: +e.target.value })} />
        <span>{SMALL_STEPS[s.smallStep]?.label}</span>
      </div>
      {canElevate && (
        <>
          <div className="pop-label">Запуск</div>
          {sw(s.alwaysAdmin, "Всегда с правами администратора", "Быстрое сканирование MFT и живое обновление сразу", () => onChange({ alwaysAdmin: !s.alwaysAdmin }))}
        </>
      )}
      <div className="pop-label">Обновления</div>
      {sw(s.autoUpdate, "Проверять при запуске", "Новые версии берутся из релизов на GitHub", () => onChange({ autoUpdate: !s.autoUpdate }))}
      <div className="upd-row">
        <span>Версия {__APP_VERSION__}</span>
        <button className="btn sm" onClick={onCheckUpdates}>Проверить сейчас</button>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ toasts / empty
export interface Toast { id: number; text: string; kind: "ok" | "err" | "info" }

export function Toasts({ items }: { items: Toast[] }) {
  return <div className="toasts">{items.map((t) => <div key={t.id} className={`toast ${t.kind}`}>{t.text}</div>)}</div>;
}

export function Empty({ icon, title, text, children }: { icon: React.ReactNode; title: string; text?: string; children?: React.ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-ic">{icon}</div>
      <h3>{title}</h3>
      {text && <p>{text}</p>}
      {children}
    </div>
  );
}

import { X } from "lucide-react";
import type { ProgressEvent } from "../types";
import { fmtMs, fmtNum, fmtSize } from "../format";

interface Props { path: string; p: ProgressEvent | null; cached: boolean; onCancel: () => void }

export default function ScanProgress({ path, p, cached, onCancel }: Props) {
  const phase = p?.phase ?? (cached ? "loading" : "reading");
  const fromCache = cached || phase === "loading" || phase === "catchup";
  const mft = p?.method === "mft";
  const steps = fromCache
    ? [{ k: "loading", t: "Загрузка сохранённого" }, { k: "catchup", t: "Применение изменений" }]
    : [{ k: "reading", t: mft ? "Чтение MFT" : "Обход папок" }, { k: "building", t: "Построение дерева" }];
  const cur = Math.max(0, steps.findIndex((s) => s.k === phase));
  const frac = p && p.totalHint > 0 && phase === "reading" ? Math.min(1, p.doneHint / p.totalHint) : null;
  const rate = p && p.elapsedMs > 300 ? Math.round((p.files + p.dirs) / (p.elapsedMs / 1000)) : 0;
  return (
    <div className="scan">
      <div className="scan-card">
        <div className="scan-title">{fromCache ? "Открываю" : "Сканирую"} {path}</div>
        <div className="scan-sub">
          {fromCache ? "Сохранённые результаты + изменения из журнала NTFS" : mft ? "Прямое чтение главной файловой таблицы (MFT)" : "Параллельный обход папок"}
        </div>
        <div className="steps">
          {steps.map((s, i) => (
            <div key={s.k} className={`step ${i < cur ? "done" : i === cur ? "active" : ""}`}>
              <i>{i < cur ? "✓" : i + 1}</i>{s.t}
            </div>
          ))}
        </div>
        <div className={`progress ${frac == null ? "indef" : ""}`}><i style={{ width: `${(frac ?? 0) * 100}%` }} /></div>
        <div className="scan-stats">
          <div><b>{fmtNum(p?.files ?? 0)}</b><span>файлов</span></div>
          <div><b>{fmtNum(p?.dirs ?? 0)}</b><span>папок</span></div>
          <div><b>{fmtSize(p?.bytes ?? 0)}</b><span>найдено</span></div>
          <div><b>{rate ? fmtNum(rate) : "—"}</b><span>объектов/с</span></div>
          <div><b>{fmtMs(p?.elapsedMs ?? 0)}</b><span>прошло</span></div>
        </div>
        <div className="scan-foot">
          <span className="dim">{frac != null ? `${Math.round(frac * 100)}% таблицы прочитано` : ""}</span>
          <button className="btn" onClick={onCancel}><X size={14} /> Отменить <kbd>Esc</kbd></button>
        </div>
      </div>
    </div>
  );
}

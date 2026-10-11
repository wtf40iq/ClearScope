import { HardDrive, ShieldAlert, FolderPlus, Usb, Network, History, Zap } from "lucide-react";
import type { CacheMeta, DriveInfo, PlatformInfo } from "../types";
import { fmtAgo, fmtNum, fmtSize, pct } from "../format";

interface Props {
  drives: DriveInfo[];
  cached: CacheMeta[];
  platform: PlatformInfo | null;
  onScan: (path: string) => void;
  onOpenCached: (path: string) => void;
  onPickFolder: () => void;
  onElevate: () => void;
}

const norm = (p: string) => p.toLowerCase().replace(/[\\/]+$/, "");

export default function Welcome({ drives, cached, platform, onScan, onOpenCached, onPickFolder, onElevate }: Props) {
  const needAdmin = platform?.canElevate && !platform.elevated;
  const cacheOf = (p: string) => cached.find((c) => norm(c.rootPath) === norm(p));
  const folders = cached.filter((c) => !drives.some((d) => norm(d.path) === norm(c.rootPath)));
  return (
    <div className="welcome">
      <div className="welcome-inner">
        <h1>Что занимает место?</h1>
        <p className="lead">Выберите диск или папку. Результаты сохраняются и открываются мгновенно, а изменения подхватываются на лету.</p>
        {needAdmin && (
          <div className="banner">
            <ShieldAlert size={20} />
            <div>
              <b>Быстрый режим недоступен без прав администратора</b>
              <span>С правами ClearScope читает таблицу MFT напрямую (в 5–20 раз быстрее) и следит за изменениями через журнал USN.</span>
            </div>
            <button className="btn" onClick={onElevate}>Перезапустить как администратор</button>
          </div>
        )}
        <div className="drive-list">
          {drives.map((d) => {
            const used = d.total - d.free;
            const p = pct(used, d.total);
            const c = cacheOf(d.path);
            const Icon = d.kind === "removable" ? Usb : d.kind === "network" ? Network : HardDrive;
            return (
              <div key={d.path} className="dcard">
                <div className="dc-top">
                  <Icon size={22} />
                  <div>
                    <div className="dc-name">{d.label || "Локальный диск"} ({d.path.replace(/\\$/, "")})</div>
                    <div className="dc-path">{d.fs || "—"}{d.fs === "NTFS" ? " · поддерживается MFT" : ""}</div>
                  </div>
                  {d.fs === "NTFS" && <span className="chip"><Zap size={11} /> MFT</span>}
                </div>
                <div>
                  <div className="dc-usage"><span><b>{fmtSize(used)}</b> занято</span><span>{fmtSize(d.free)} свободно</span></div>
                  <div className="bar" style={{ marginTop: 7 }}><i className={p > 92 ? "crit" : p > 80 ? "warn" : ""} style={{ width: `${p}%` }} /></div>
                </div>
                <div className="dc-actions">
                  {c ? (
                    <>
                      <button className="btn primary" onClick={() => onOpenCached(d.path)}><History size={15} /> Открыть</button>
                      <button className="btn" onClick={() => onScan(d.path)}>Сканировать заново</button>
                    </>
                  ) : (
                    <button className="btn primary" onClick={() => onScan(d.path)}>Сканировать</button>
                  )}
                </div>
                {c && <div className="dc-cache">Сохранено {fmtAgo(c.capturedAt)} · {fmtNum(c.files)} файлов</div>}
              </div>
            );
          })}
          {folders.map((c) => (
            <div key={c.rootPath} className="dcard">
              <div className="dc-top">
                <FolderPlus size={22} />
                <div>
                  <div className="dc-name" title={c.rootPath}>{c.rootPath.split(/[\\/]/).filter(Boolean).pop()}</div>
                  <div className="dc-path" title={c.rootPath}>{c.rootPath}</div>
                </div>
              </div>
              <div className="dc-usage"><span><b>{fmtSize(c.size)}</b> в {fmtNum(c.files)} файлах</span></div>
              <div className="dc-actions">
                <button className="btn primary" onClick={() => onOpenCached(c.rootPath)}><History size={15} /> Открыть</button>
                <button className="btn" onClick={() => onScan(c.rootPath)}>Сканировать заново</button>
              </div>
              <div className="dc-cache">Сохранено {fmtAgo(c.capturedAt)}</div>
            </div>
          ))}
          <button className="dcard folder" onClick={onPickFolder}>
            <FolderPlus size={26} />
            <div className="dc-name">Выбрать папку</div>
            <div className="dc-path">Проанализировать одну папку</div>
          </button>
        </div>
      </div>
    </div>
  );
}

import { HardDrive, Usb, Network, FolderPlus, LayoutGrid, PieChart, ListOrdered, Sparkles, Copy, History, Search } from "lucide-react";
import type { DriveInfo, Summary, View } from "../types";
import { fmtSize, pct } from "../format";

interface Props {
  drives: DriveInfo[];
  summary: Summary | null;
  view: View;
  scanning: boolean;
  onView: (v: View) => void;
  onOpen: (path: string) => void;
  onPickFolder: () => void;
}

const NAV: { v: View; label: string; icon: typeof LayoutGrid; key?: string }[] = [
  { v: "overview", label: "Обзор", icon: LayoutGrid, key: "1" },
  { v: "types", label: "Типы файлов", icon: PieChart, key: "2" },
  { v: "top", label: "Крупные файлы", icon: ListOrdered, key: "3" },
  { v: "cleanup", label: "Очистка", icon: Sparkles, key: "4" },
  { v: "dupes", label: "Дубликаты", icon: Copy, key: "5" },
  { v: "changes", label: "Что изменилось", icon: History, key: "6" },
  { v: "search", label: "Поиск", icon: Search },
];
export const NAV_KEYS = NAV.filter((n) => n.key).map((n) => n.v);

export default function Sidebar({ drives, summary, view, scanning, onView, onOpen, onPickFolder }: Props) {
  const root = summary?.rootPath.toLowerCase();
  return (
    <aside className="sidebar">
      <div className="brand">
        <img className="logo" src="/logo.svg" alt="" draggable={false} />
        <div className="brand-name">ClearScope</div>
      </div>

      <div className="side-title">Диски</div>
      <div className="drives">
        {drives.map((d) => {
          const used = d.total - d.free;
          const p = pct(used, d.total);
          const Icon = d.kind === "removable" ? Usb : d.kind === "network" ? Network : HardDrive;
          return (
            <button key={d.path} className={`drive ${root === d.path.toLowerCase() ? "active" : ""}`} disabled={scanning} onClick={() => onOpen(d.path)} title={`${d.fs} · ${fmtSize(used)} занято`}>
              <Icon size={17} className="drive-ic" />
              <div className="drive-main">
                <div className="drive-name">{d.label || "Локальный диск"} <b>{d.path.replace(/\\$/, "")}</b></div>
                <div className="bar"><i className={p > 92 ? "crit" : p > 80 ? "warn" : ""} style={{ width: `${p}%` }} /></div>
                <div className="drive-meta">{fmtSize(d.free)} свободно из {fmtSize(d.total)}</div>
              </div>
            </button>
          );
        })}
        <button className="drive folder" disabled={scanning} onClick={onPickFolder}>
          <FolderPlus size={17} className="drive-ic" />
          <div className="drive-main"><div className="drive-name">Папка…</div></div>
        </button>
      </div>

      {summary && !scanning && (
        <>
          <div className="side-title">Анализ</div>
          <nav className="nav">
            {NAV.map(({ v, label, icon: Icon, key }) => (
              <button key={v} className={`nav-item ${view === v ? "active" : ""}`} onClick={() => onView(v)}>
                <Icon size={16} />
                {label}
                {key && <kbd>Alt+{key}</kbd>}
              </button>
            ))}
          </nav>
        </>
      )}
      <div className="side-foot">ClearScope {__APP_VERSION__}</div>
    </aside>
  );
}

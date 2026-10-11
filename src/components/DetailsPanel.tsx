import { useEffect, useState } from "react";
import { FolderOpen, ExternalLink, Trash2, ZoomIn, ShieldX, AlertTriangle, ShieldCheck, Copy } from "lucide-react";
import type { DeleteCheck, NodeInfo } from "../types";
import { api } from "../api";
import { categoryOf } from "../categories";
import { fmtDateTime, fmtDelta, fmtNum, fmtPct, fmtSize, pct } from "../format";
import FileIcon from "./FileIcon";

interface Props {
  id: number;
  isFocus: boolean;
  version: number;
  onSelect: (id: number) => void;
  onZoom: (id: number) => void;
  onDelete: (ids: number[]) => void;
  onCopied: () => void;
}

export default function DetailsPanel({ id, isFocus, version, onSelect, onZoom, onDelete, onCopied }: Props) {
  const [n, setN] = useState<NodeInfo | null>(null);
  const [parent, setParent] = useState<NodeInfo | null>(null);
  const [kids, setKids] = useState<NodeInfo[]>([]);
  const [guard, setGuard] = useState<DeleteCheck | null>(null);

  useEffect(() => {
    let alive = true;
    api.getNode(id).then(async (node) => {
      if (!alive) return;
      setN(node);
      setParent(node.parent != null ? await api.getNode(node.parent).catch(() => null) : null);
      setKids(node.isDir ? await api.getChildren(id, 8).catch(() => []) : []);
    }).catch(() => alive && setN(null));
    api.checkDelete([id]).then((r) => alive && setGuard(r[0] ?? null)).catch(() => alive && setGuard(null));
    return () => { alive = false; };
  }, [id, version]);

  if (!n) return <div className="pane details" />;
  const cat = n.isDir ? "Папка" : `${n.ext ? `.${n.ext.toUpperCase()} · ` : ""}${categoryOf(n.ext).label}`;
  const share = parent ? pct(n.size, parent.size) : 100;
  const copy = async () => { await navigator.clipboard.writeText(n.path ?? n.name).catch(() => {}); onCopied(); };
  const G = guard?.level === "blocked" ? ShieldX : guard?.level === "warn" ? AlertTriangle : ShieldCheck;

  return (
    <div className="pane details">
      <div className="det-body">
        <div className="det-head">
          <FileIcon isDir={n.isDir} ext={n.ext} size={30} />
          <div style={{ minWidth: 0 }}>
            <div className="det-name">{n.name}</div>
            <div className="det-type">{cat}</div>
          </div>
        </div>
        <div>
          <div className="det-size">{fmtSize(n.size)}</div>
          {parent && <div className="dim" style={{ fontSize: 12 }}>{fmtPct(share)} от «{parent.name}»</div>}
        </div>
        <div className="det-path" title="Нажмите, чтобы скопировать" onClick={copy}>{n.path}</div>
        <div className="det-grid">
          <span>На диске</span><b>{fmtSize(n.alloc)}</b>
          {n.isDir && <><span>Файлов</span><b>{fmtNum(n.files)}</b><span>Папок</span><b>{fmtNum(n.dirs)}</b></>}
          <span>Изменён</span><b>{fmtDateTime(n.modified)}</b>
          {n.delta != null && Math.abs(n.delta) > 0 && <><span>С прошлого раза</span><b className={n.delta > 0 ? "up" : "down"}>{fmtDelta(n.delta)}</b></>}
        </div>
        {guard && !isFocus && (
          <div className={`guard ${guard.level}`}>
            <G size={15} />
            <span>{guard.level === "ok" ? "Можно удалять — системной защиты нет." : guard.reason}</span>
          </div>
        )}
        <div className="det-actions">
          <button className="btn sm" onClick={() => api.revealItem(id)}><FolderOpen size={14} /> В проводнике</button>
          <button className="btn sm" onClick={() => api.openItem(id)}><ExternalLink size={14} /> Открыть</button>
          {n.isDir && !isFocus && <button className="btn sm" onClick={() => onZoom(id)}><ZoomIn size={14} /> Войти</button>}
          <button className="btn sm" onClick={copy}><Copy size={14} /> Путь</button>
          {!isFocus && (
            <button className="btn sm danger wide" disabled={guard?.level === "blocked"} onClick={() => onDelete([id])}>
              <Trash2 size={14} /> {guard?.level === "blocked" ? "Удаление запрещено" : "Удалить…"}
            </button>
          )}
        </div>
        {kids.length > 0 && (
          <div>
            <div className="det-sec">Крупнее всего внутри</div>
            <div className="det-kids">
              {kids.map((k) => (
                <div key={k.id} className="det-kid" onClick={() => onSelect(k.id)} onDoubleClick={() => k.isDir && onZoom(k.id)}>
                  <span>{k.name}</span><b>{fmtSize(k.size)}</b>
                  <div className="bar"><i style={{ width: `${pct(k.size, n.size)}%` }} /></div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

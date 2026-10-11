import type { NodeInfo } from "../types";
import { fmtDate, fmtSize } from "../format";
import FileIcon from "./FileIcon";

export function splitPath(p: string): [string, string] {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? [p.slice(0, i + 1), p.slice(i + 1)] : ["", p];
}

interface Props {
  n: NodeInfo;
  max: number;
  rank?: number;
  selected?: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onContext: (x: number, y: number) => void;
}

export default function FileRow({ n, max, rank, selected, onSelect, onOpen, onContext }: Props) {
  const [dir, name] = splitPath(n.path ?? n.name);
  return (
    <div className={`frow ${selected ? "sel" : ""}`} onClick={onSelect} onDoubleClick={onOpen}
      onContextMenu={(e) => { e.preventDefault(); onSelect(); onContext(e.clientX, e.clientY); }}>
      {rank != null && <span className="rank">{rank}</span>}
      <FileIcon isDir={n.isDir} ext={n.ext} size={18} />
      <div className="frow-main">
        <div className="frow-name">{name || n.name}</div>
        <div className="frow-path" title={n.path}>{dir}</div>
      </div>
      <div className="bar"><i style={{ width: `${max > 0 ? (n.size / max) * 100 : 0}%` }} /></div>
      <div className="frow-date">{fmtDate(n.modified)}</div>
      <div className="frow-size">{fmtSize(n.size)}</div>
    </div>
  );
}

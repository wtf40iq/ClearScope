export interface NodeInfo {
  id: number;
  parent: number | null;
  name: string;
  size: number;
  alloc: number;
  files: number;
  dirs: number;
  isDir: boolean;
  modified: number;
  childCount: number;
  ext: string;
  path?: string;
  delta?: number | null;
}

export interface Summary {
  rootId: number;
  rootPath: string;
  method: "mft" | "walk";
  scanMs: number;
  size: number;
  alloc: number;
  files: number;
  dirs: number;
  prevTime: number | null;
  prevTotal: number | null;
  fallbackReason: string | null;
  fromCache: boolean;
  stale: boolean;
  capturedAt: number;
  live: "usn" | "watch" | "paused" | "off";
}

export interface DriveInfo {
  path: string;
  label: string;
  fs: string;
  kind: string;
  total: number;
  free: number;
  cluster: number;
}

export interface PlatformInfo {
  os: string;
  elevated: boolean;
  canElevate: boolean;
}

export interface TmNode {
  id: number;
  name: string;
  size: number;
  isDir: boolean;
  ext: string;
  children?: TmNode[];
}

export interface ExtStat {
  ext: string;
  size: number;
  count: number;
}

export interface CleanupItem {
  id: number;
  path: string;
  size: number;
  isDir: boolean;
}

export interface CleanupGroup {
  key: string;
  title: string;
  description: string;
  risk: "safe" | "review";
  total: number;
  count: number;
  items: CleanupItem[];
}

export interface DupGroup {
  size: number;
  wasted: number;
  items: { id: number; path: string }[];
}

export interface Change {
  path: string;
  id: number | null;
  oldSize: number;
  newSize: number;
  delta: number;
}

export interface ProgressEvent {
  files: number;
  dirs: number;
  bytes: number;
  totalHint: number;
  doneHint: number;
  elapsedMs: number;
  method: "mft" | "walk";
  phase: "idle" | "reading" | "building" | "loading" | "catchup";
}

export interface DoneEvent {
  ok: boolean;
  cancelled: boolean;
  error: string | null;
  summary: Summary | null;
}

export interface SearchResult {
  total: number;
  items: NodeInfo[];
  tookMs: number;
}

export interface DeleteResult {
  deleted: number[];
  failed: { path: string; error: string }[];
  freed: number;
}

export type View = "overview" | "types" | "top" | "cleanup" | "dupes" | "changes" | "search";

export interface DeleteCheck {
  id: number;
  name: string;
  path: string;
  size: number;
  isDir: boolean;
  level: "ok" | "warn" | "blocked";
  reason: string;
}

export interface CacheMeta {
  rootPath: string;
  method: "mft" | "walk";
  scanMs: number;
  capturedAt: number;
  savedAt: number;
  journalId: number;
  nextUsn: number;
  size: number;
  files: number;
  nodes: number;
}

export type Theme = "system" | "light" | "dark";

export interface Settings {
  theme: Theme;
  live: boolean;
  openLast: boolean;
  details: boolean;
  /** hide small items and empty folders in the tree */
  hideSmall: boolean;
  /** index into SMALL_STEPS */
  smallStep: number;
  alwaysAdmin: boolean;
  /** check GitHub releases on startup */
  autoUpdate: boolean;
}

export const SMALL_STEPS: { bytes: number; label: string }[] = [
  { bytes: 0, label: "только пустые" },
  { bytes: 1 << 10, label: "меньше 1 КБ" },
  { bytes: 100 << 10, label: "меньше 100 КБ" },
  { bytes: 1 << 20, label: "меньше 1 МБ" },
  { bytes: 10 << 20, label: "меньше 10 МБ" },
  { bytes: 100 << 20, label: "меньше 100 МБ" },
  { bytes: 1 << 30, label: "меньше 1 ГБ" },
];

import type {
  CacheMeta, Change, CleanupGroup, DeleteCheck, DeleteResult, DoneEvent, DriveInfo, DupGroup, ExtStat, NodeInfo,
  PlatformInfo, ProgressEvent, SearchResult, Summary, TmNode,
} from "./types";
import { mock } from "./mock";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  return mock.invoke(cmd, args ?? {}) as Promise<T>;
}

async function on<T>(event: string, cb: (p: T) => void): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, (e) => cb(e.payload));
  }
  return mock.listen(event, cb as (p: unknown) => void);
}

export const api = {
  listDrives: () => call<DriveInfo[]>("list_drives"),
  platformInfo: () => call<PlatformInfo>("platform_info"),
  restartAsAdmin: (target?: string) => call<void>("restart_as_admin", { target: target ?? null }),
  startupTarget: () => call<string | null>("startup_target"),
  startScan: (path: string, preferMft = true) => call<void>("start_scan", { path, preferMft }),
  openCached: (path: string) => call<void>("open_cached", { path }),
  listCached: () => call<CacheMeta[]>("list_cached"),
  forgetCache: (path: string) => call<void>("forget_cache", { path }),
  setLive: (enabled: boolean) => call<void>("set_live", { enabled }),
  findByPath: (path: string) => call<number | null>("find_by_path", { path }),
  checkDelete: (ids: number[]) => call<DeleteCheck[]>("check_delete", { ids }),
  cancelScan: () => call<void>("cancel_scan"),
  getSummary: () => call<Summary | null>("get_summary"),
  getNode: (id: number) => call<NodeInfo>("get_node", { id }),
  getChildren: (id: number, limit?: number) => call<NodeInfo[]>("get_children", { id, limit }),
  getAncestors: (id: number) => call<NodeInfo[]>("get_ancestors", { id }),
  getTreemap: (id: number, maxNodes?: number) => call<TmNode>("get_treemap", { id, maxNodes }),
  getTopFiles: (id: number, limit?: number) => call<NodeInfo[]>("get_top_files", { id, limit }),
  getExtStats: (id: number, limit?: number) => call<ExtStat[]>("get_ext_stats", { id, limit }),
  search: (id: number, query: string, limit?: number) => call<SearchResult>("search", { id, query, limit }),
  getCleanup: (id: number) => call<CleanupGroup[]>("get_cleanup", { id }),
  getChanges: (limit?: number) => call<Change[] | null>("get_changes", { limit }),
  findDuplicates: (id: number, minSize: number) => call<DupGroup[]>("find_duplicates", { id, minSize }),
  getDupeProgress: () => call<{ done: number; total: number; bytes: number }>("get_dupe_progress"),
  cancelDuplicates: () => call<void>("cancel_duplicates"),
  deleteItems: (ids: number[], permanent: boolean, allowWarn: boolean) => call<DeleteResult>("delete_items", { ids, permanent, allowWarn }),
  openItem: (id: number) => call<void>("open_item", { id }),
  revealItem: (id: number) => call<void>("reveal_item", { id }),
  exportCsv: (id: number, path: string) => call<number>("export_csv", { id, path }),
  onProgress: (cb: (p: ProgressEvent) => void) => on<ProgressEvent>("scan-progress", cb),
  onDone: (cb: (p: DoneEvent) => void) => on<DoneEvent>("scan-done", cb),
  onTreeChanged: (cb: (s: Summary) => void) => on<Summary>("tree-changed", cb),
  onLiveStatus: (cb: (s: string) => void) => on<string>("live-status", cb),
  onLiveResync: (cb: () => void) => on<null>("live-resync", () => cb()),

  async setWindowTheme(theme: "light" | "dark" | null) {
    if (!isTauri) return;
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().setTheme(theme);
    } catch { /* not critical */ }
  },

  async pickFolder(): Promise<string | null> {
    if (!isTauri) return "C:\\Users\\Alex";
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: true, multiple: false, title: "Выберите папку для анализа" });
    return typeof r === "string" ? r : null;
  },
  async pickSaveCsv(): Promise<string | null> {
    if (!isTauri) return null;
    const { save } = await import("@tauri-apps/plugin-dialog");
    return save({ title: "Экспорт в CSV", defaultPath: "clearscope-export.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
  },
};

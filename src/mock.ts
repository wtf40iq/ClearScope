// Browser-only demo backend: lets the UI run with `npm run dev` without Tauri.
type Raw = { name: string; parent: number; size: number; isDir: boolean; modified: number; kids: number[]; deleted?: boolean };

let seed = 42;
const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
const pick = <T,>(a: T[]) => a[Math.floor(rnd() * a.length)];
const now = Math.floor(Date.now() / 1000);

const nodes: Raw[] = [];
function add(parent: number, name: string, isDir: boolean, size = 0, age = 400): number {
  const id = nodes.length;
  nodes.push({ name, parent, size, isDir, modified: now - Math.floor(rnd() * age * 86400), kids: [] });
  if (parent >= 0) nodes[parent].kids.push(id);
  return id;
}
const MB = 1 << 20, GB = 1 << 30;
function files(dir: number, n: number, exts: string[], min: number, max: number, prefix = "file") {
  for (let i = 0; i < n; i++) {
    const s = Math.floor(min + Math.pow(rnd(), 3) * (max - min));
    add(dir, `${prefix}_${i + 1}.${pick(exts)}`, false, s);
  }
}
function path(dir: number, parts: string[]): number {
  let cur = dir;
  for (const p of parts) {
    const f = nodes[cur].kids.find((k) => nodes[k].name === p && nodes[k].isDir);
    cur = f ?? add(cur, p, true);
  }
  return cur;
}

function build() {
  const root = add(-1, "C:\\", true);
  add(root, "pagefile.sys", false, 16 * GB);
  add(root, "hiberfil.sys", false, 6.4 * GB);
  add(root, "swapfile.sys", false, 256 * MB);
  const win = path(root, ["Windows"]);
  for (const d of ["System32", "SysWOW64", "WinSxS", "assembly", "Fonts", "servicing"]) files(path(win, [d]), 120, ["dll", "exe", "sys", "mui", "dat"], 20e3, 90 * MB, d.toLowerCase());
  files(path(win, ["SoftwareDistribution", "Download"]), 14, ["cab", "msu", "esd"], 50 * MB, 900 * MB, "update");
  files(path(win, ["Temp"]), 40, ["tmp", "log", "etl"], 1e3, 60 * MB, "tmp");
  files(path(win, ["Installer"]), 30, ["msi", "msp"], 5 * MB, 300 * MB, "inst");
  const pf = path(root, ["Program Files"]);
  for (const app of ["Adobe", "Microsoft Office", "JetBrains", "NVIDIA Corporation", "Docker", "Blender Foundation", "Mozilla Firefox", "7-Zip"])
    files(path(pf, [app]), 30 + Math.floor(rnd() * 40), ["dll", "exe", "pak", "dat", "json"], 50e3, 400 * MB, app.split(" ")[0].toLowerCase());
  const steam = path(root, ["Games", "Steam", "steamapps", "common"]);
  for (const g of ["Cyberpunk 2077", "Baldur's Gate 3", "Counter-Strike 2", "Red Dead Redemption 2", "Elden Ring"])
    files(path(steam, [g, "archive"]), 25, ["archive", "pak", "bin", "rpf", "bundle"], 200 * MB, 6 * GB, "data");
  files(path(root, ["Games", "Steam", "steamapps", "shadercache"]), 30, ["bin"], MB, 200 * MB, "shader");
  const u = path(root, ["Users", "Alex"]);
  files(path(u, ["Videos", "Отпуск 2025"]), 40, ["mp4", "mov"], 200 * MB, 4 * GB, "VID");
  files(path(u, ["Videos", "OBS"]), 18, ["mkv"], 500 * MB, 8 * GB, "record");
  files(path(u, ["Pictures", "Camera"]), 600, ["jpg", "heic", "png"], 1 * MB, 14 * MB, "IMG");
  files(path(u, ["Pictures", "RAW"]), 200, ["cr2", "dng"], 20 * MB, 60 * MB, "RAW");
  files(path(u, ["Music"]), 400, ["mp3", "flac"], 4 * MB, 60 * MB, "track");
  files(path(u, ["Documents"]), 300, ["pdf", "docx", "xlsx", "pptx", "txt"], 10e3, 80 * MB, "doc");
  const dl = path(u, ["Downloads"]);
  files(dl, 35, ["exe", "msi", "zip", "iso", "rar", "pdf"], 2 * MB, 5 * GB, "setup");
  add(dl, "Windows11_23H2.iso", false, 6.2 * GB, 900);
  add(dl, "ubuntu-24.04-desktop-amd64.iso", false, 5.8 * GB, 700);
  files(path(u, ["AppData", "Local", "Temp"]), 120, ["tmp", "log", "dmp"], 1e3, 120 * MB, "tmp");
  files(path(u, ["AppData", "Local", "Google", "Chrome", "User Data", "Default", "Cache", "Cache_Data"]), 200, ["cache"], 10e3, 30 * MB, "f");
  files(path(u, ["AppData", "Local", "Microsoft", "Edge", "User Data", "Default", "Cache"]), 80, ["cache"], 10e3, 20 * MB, "f");
  files(path(u, ["AppData", "Local", "NVIDIA", "DXCache"]), 50, ["bin"], MB, 80 * MB, "dx");
  files(path(u, ["AppData", "Roaming", "Telegram Desktop", "tdata"]), 90, ["db", "dat"], 1e5, 300 * MB, "td");
  files(path(u, ["AppData", "Local", "npm-cache", "_cacache"]), 300, ["json", "tgz"], 1e3, 8 * MB, "pkg");
  const proj = path(u, ["Projects"]);
  for (const p of ["web-app", "landing", "dashboard"]) {
    files(path(proj, [p, "node_modules", "react"]), 40, ["js", "map", "json"], 1e3, 3 * MB, "m");
    files(path(proj, [p, "node_modules", "@next", "swc"]), 6, ["node"], 30 * MB, 140 * MB, "swc");
    files(path(proj, [p, "src"]), 60, ["ts", "tsx", "css"], 500, 80e3, "src");
  }
  const rs = path(proj, ["rust-engine"]);
  add(rs, "Cargo.toml", false, 1200);
  files(path(rs, ["target", "release", "deps"]), 150, ["rlib", "rmeta", "d"], 1e5, 90 * MB, "dep");
  files(path(root, ["$Recycle.Bin", "S-1-5-21"]), 20, ["mp4", "zip", "docx"], MB, 900 * MB, "$R");
  // duplicates
  const v = path(u, ["Videos", "Отпуск 2025"]);
  const dupSrc = nodes[nodes[v].kids[0]];
  add(path(u, ["Desktop"]), dupSrc.name, false, dupSrc.size);
  add(path(u, ["Documents", "Backup"]), dupSrc.name, false, dupSrc.size);
  return root;
}
const ROOT = build();
// aggregate
const agg = (i: number): number => {
  const n = nodes[i];
  if (!n.isDir) return n.size;
  n.size = n.kids.reduce((s, k) => s + agg(k), 0);
  n.kids.sort((a, b) => nodes[b].size - nodes[a].size);
  return n.size;
};
agg(ROOT);

const live = (i: number) => nodes[i].kids.filter((k) => !nodes[k].deleted);
const extOf = (n: string) => { const i = n.lastIndexOf("."); return i > 0 ? n.slice(i + 1).toLowerCase() : ""; };
const pathOf = (i: number): string => (i === ROOT ? "C:\\" : (nodes[i].parent === ROOT ? "C:\\" : pathOf(nodes[i].parent) + "\\") + nodes[i].name);
function count(i: number): [number, number] {
  const n = nodes[i];
  if (!n.isDir) return [1, 0];
  let f = 0, d = 0;
  for (const k of live(i)) { const [a, b] = count(k); f += a; d += b + (nodes[k].isDir ? 1 : 0); }
  return [f, d];
}
const counts = new Map<number, [number, number]>();
const cnt = (i: number) => { if (!counts.has(i)) counts.set(i, count(i)); return counts.get(i)!; };
const info = (i: number, withPath = false) => {
  const n = nodes[i];
  const [files, dirs] = cnt(i);
  return { id: i, parent: n.parent < 0 ? null : n.parent, name: n.name, size: n.size, alloc: Math.ceil(n.size / 4096) * 4096, files, dirs, isDir: n.isDir, modified: n.modified, childCount: live(i).length, ext: n.isDir ? "" : extOf(n.name), path: withPath ? pathOf(i) : undefined, delta: n.isDir && i % 7 === 0 ? Math.floor((rnd() - 0.3) * n.size * 0.2) : null };
};
function* sub(i: number): Generator<number> { yield i; if (nodes[i].isDir) for (const k of live(i)) yield* sub(k); }

const listeners: Record<string, ((p: unknown) => void)[]> = {};
const emit = (e: string, p: unknown) => (listeners[e] ?? []).forEach((f) => f(p));
let scanned = false;
let fromCache = false;
const summary = () => { const [f, d] = cnt(ROOT); return { rootId: ROOT, rootPath: "C:\\", method: "mft", scanMs: 1840, size: nodes[ROOT].size, alloc: nodes[ROOT].size, files: f, dirs: d, prevTime: now - 5 * 86400, prevTotal: nodes[ROOT].size - 3.4 * GB, fallbackReason: null, fromCache, stale: false, capturedAt: now - 120, live: "usn" }; };
function verdict(i: number): { level: "ok" | "warn" | "blocked"; reason: string } {
  const p = pathOf(i).toLowerCase();
  const c = p.split("\\").filter(Boolean).slice(1);
  if (i === ROOT || c.length === 0) return { level: "blocked", reason: "Корень диска удалить нельзя" };
  if (c[0] === "windows" && !(c[1] === "temp" && c.length >= 3) && !(c[1] === "softwaredistribution" && c[2] === "download" && c.length >= 4)) return { level: "blocked", reason: "Системные файлы Windows. Удаление может сломать систему" };
  if (c.length === 1 && ["pagefile.sys", "hiberfil.sys", "swapfile.sys", "$recycle.bin", "program files", "users"].includes(c[0])) return { level: "blocked", reason: c[0] === "hiberfil.sys" ? "Файл гибернации. Отключить: команда «powercfg /h off» от администратора" : "Служебный объект Windows" };
  if (c[0] === "users" && c.length <= 3) return { level: "blocked", reason: "Стандартная папка профиля. Удаляйте её содержимое, а не саму папку" };
  if (c[0] === "program files") return { level: "warn", reason: "Файлы установленной программы. Правильнее удалить программу через «Параметры → Приложения»" };
  if (c.includes("appdata") && !c.some((x) => ["temp", "cache", "cache_data", "dxcache", "npm-cache"].includes(x))) return { level: "warn", reason: "Настройки и данные программ" };
  return { level: "ok", reason: "" };
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const mock = {
  listen(e: string, cb: (p: unknown) => void) {
    (listeners[e] ??= []).push(cb);
    return Promise.resolve(() => { listeners[e] = listeners[e].filter((f) => f !== cb); });
  },
  async invoke(cmd: string, a: Record<string, any>): Promise<unknown> {
    await sleep(15);
    switch (cmd) {
      case "list_drives": return [
        { path: "C:\\", label: "Windows", fs: "NTFS", kind: "fixed", total: 1000 * GB, free: 214 * GB, cluster: 4096 },
        { path: "D:\\", label: "Games", fs: "NTFS", kind: "fixed", total: 2000 * GB, free: 1210 * GB, cluster: 4096 },
        { path: "E:\\", label: "Flash", fs: "exFAT", kind: "removable", total: 64 * GB, free: 41 * GB, cluster: 131072 },
      ];
      case "platform_info": return { os: "windows", elevated: true, canElevate: true };
      case "start_scan":
      case "open_cached": {
        const cached = cmd === "open_cached";
        const t0 = Date.now(); const [f, d] = cnt(ROOT); const total = 2_400_000; const dur = cached ? 500 : 2200;
        const tick = setInterval(() => {
          const k = Math.min(1, (Date.now() - t0) / dur);
          const phase = cached ? (k < 0.6 ? "loading" : "catchup") : k < 0.85 ? "reading" : "building";
          const r = Math.min(1, k / 0.85);
          emit("scan-progress", { files: Math.floor(f * r * 90), dirs: Math.floor(d * r * 60), bytes: Math.floor(nodes[ROOT].size * r), totalHint: total, doneHint: Math.floor(total * r), elapsedMs: Date.now() - t0, method: "mft", phase });
          if (k >= 1) { clearInterval(tick); scanned = true; fromCache = cached; emit("scan-done", { ok: true, cancelled: false, error: null, summary: summary() }); }
        }, 50);
        return null;
      }
      case "list_cached": return [{ rootPath: "C:\\", method: "mft", scanMs: 1840, capturedAt: now - 3600 * 3, savedAt: now - 600, journalId: 1, nextUsn: 1, size: nodes[ROOT].size, files: 4296, nodes: 5000 }];
      case "startup_target": return null;
      case "set_live": case "forget_cache": return null;
      case "find_by_path": return null;
      case "filter_live": return (a.ids as number[]).filter((i) => nodes[i] && !nodes[i].deleted);
      case "check_delete": return (a.ids as number[]).map((i) => ({ id: i, name: nodes[i].name, path: pathOf(i), size: nodes[i].size, isDir: nodes[i].isDir, ...verdict(i) }));
      case "cancel_scan": return null;
      case "get_summary": return scanned ? summary() : null;
      case "get_node": return info(a.id, true);
      case "get_children": return live(a.id).slice(0, a.limit ?? 5000).map((k) => info(k));
      case "get_ancestors": { const out = []; let i = a.id; while (i >= 0) { out.unshift(info(i)); i = nodes[i].parent; } return out; }
      case "get_treemap": {
        const max = a.maxNodes ?? 9000; let n = 0;
        const b = (i: number): any => { n++; const x = nodes[i]; return { id: i, name: x.name, size: x.size, isDir: x.isDir, ext: x.isDir ? "" : extOf(x.name), children: x.isDir && n < max ? live(i).filter((k) => nodes[k].size > nodes[a.id].size / 200000).map(b) : undefined }; };
        return b(a.id);
      }
      case "get_top_files": return [...sub(a.id)].filter((i) => !nodes[i].isDir).sort((x, y) => nodes[y].size - nodes[x].size).slice(0, a.limit ?? 500).map((i) => info(i, true));
      case "get_ext_stats": { const m = new Map<string, { ext: string; size: number; count: number }>(); for (const i of sub(a.id)) { if (nodes[i].isDir) continue; const e = extOf(nodes[i].name); const s = m.get(e) ?? { ext: e, size: 0, count: 0 }; s.size += nodes[i].size; s.count++; m.set(e, s); } return [...m.values()].sort((x, y) => y.size - x.size); }
      case "search": { const q = String(a.query).toLowerCase().trim(); const t = performance.now(); const ext = q.startsWith("*.") ? q.slice(2) : null; const hits = [...sub(a.id)].filter((i) => i !== a.id && (ext ? extOf(nodes[i].name) === ext : nodes[i].name.toLowerCase().includes(q))).sort((x, y) => nodes[y].size - nodes[x].size); return { total: hits.length, items: hits.slice(0, a.limit ?? 1000).map((i) => info(i, true)), tookMs: Math.round(performance.now() - t) }; }
      case "get_cleanup": {
        const groups: [string, string, string, "safe" | "review", (p: string, i: number) => boolean][] = [
          ["temp", "Временные файлы", "Папки Temp системы и пользователей. Обычно их можно удалить без последствий.", "safe", (p) => /\\temp$/i.test(p)],
          ["browser", "Кэш браузеров", "Кэш Chrome, Edge, Firefox, Яндекс, Opera, Brave. Браузер создаст его заново.", "safe", (p) => /(chrome|edge).*\\cache$/i.test(p)],
          ["shader", "Кэш шейдеров и драйверов", "DirectX/NVIDIA/AMD кэши шейдеров. Пересоздаются автоматически.", "safe", (p) => /(dxcache|shadercache)$/i.test(p)],
          ["dev", "Мусор разработчика", "node_modules, __pycache__, кэши npm/pip/yarn/gradle, папки target у Rust-проектов.", "review", (p) => /(node_modules|npm-cache|rust-engine\\target)$/i.test(p)],
          ["system", "Системный мусор", "Обновления Windows, Windows.old, корзина, дампы памяти и отчёты об ошибках.", "review", (p) => /(softwaredistribution\\download|\$recycle\.bin)$/i.test(p)],
          ["installers", "Установщики и архивы в Загрузках", "Файлы .exe/.msi/.iso/.zip больше 10 МБ в папках Downloads.", "review", (p, i) => /\\downloads\\.*\.(exe|msi|iso|zip|rar)$/i.test(p) && nodes[i].size > 10 * MB],
        ];
        const all = [...sub(a.id)].map((i) => [i, pathOf(i)] as const);
        return groups.map(([key, title, description, risk, f]) => { const items = all.filter(([i, p]) => f(p, i)).map(([i, p]) => ({ id: i, path: p, size: nodes[i].size, isDir: nodes[i].isDir })).sort((x, y) => y.size - x.size); return { key, title, description, risk, total: items.reduce((s, x) => s + x.size, 0), count: items.length, items }; }).filter((g) => g.count).sort((x, y) => y.total - x.total);
      }
      case "get_changes": return [...sub(ROOT)].filter((i) => nodes[i].isDir && nodes[i].size > 500 * MB).slice(0, 40).map((i) => { const d = Math.floor((rnd() - 0.35) * nodes[i].size * 0.3); return { path: pathOf(i), id: i, oldSize: nodes[i].size - d, newSize: nodes[i].size, delta: d }; }).sort((x, y) => Math.abs(y.delta) - Math.abs(x.delta));
      case "find_duplicates": { await sleep(1200); const m = new Map<string, number[]>(); for (const i of sub(a.id)) { const n = nodes[i]; if (n.isDir || n.size < a.minSize) continue; const k = n.name + n.size; m.set(k, [...(m.get(k) ?? []), i]); } return [...m.values()].filter((g) => g.length > 1).map((g) => ({ size: nodes[g[0]].size, wasted: nodes[g[0]].size * (g.length - 1), items: g.map((i) => ({ id: i, path: pathOf(i) })) })); }
      case "get_dupe_progress": return { done: 50, total: 100, bytes: 3 * GB };
      case "cancel_duplicates": return null;
      case "delete_items": { let freed = 0; for (const id of (a.ids as number[]).filter((i) => { const v = verdict(i).level; return v === "ok" || (v === "warn" && a.allowWarn); })) { const n = nodes[id]; n.deleted = true; freed += n.size; let p = n.parent; while (p >= 0) { nodes[p].size -= n.size; p = nodes[p].parent; } } counts.clear(); return { deleted: a.ids, failed: [], freed, left: 0, cleared: 0 }; }
      default: return null;
    }
  },
};

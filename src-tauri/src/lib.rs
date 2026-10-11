mod platform;

use serde::Serialize;
use clearscope_core::{analysis, cache, dupes, live, safety, snapshot, tree::ext_of, Phase, Progress, ScanMethod, Tree, NONE};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

// ---------------------------------------------------------------- state

struct Loaded {
    tree: Tree,
    prev: Option<snapshot::Snapshot>,
    /// (journal id, next usn) for NTFS volumes read through the MFT
    journal: Option<(u64, i64)>,
    from_cache: bool,
    stale: bool,
    dirty: bool,
    cluster: u64,
    last_save: Instant,
}

#[derive(Default)]
struct AppState {
    data: RwLock<Option<Loaded>>,
    prog: Arc<Progress>,
    busy: Arc<AtomicBool>,
    dup_prog: Arc<Progress>,
    live_gen: Arc<AtomicU64>,
    live_paused: Arc<AtomicBool>,
    live_mode: Mutex<&'static str>,
}

type Res<T> = Result<T, String>;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NodeInfo {
    id: u32,
    parent: Option<u32>,
    name: String,
    size: u64,
    alloc: u64,
    files: u32,
    dirs: u32,
    is_dir: bool,
    modified: i64,
    child_count: u32,
    ext: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delta: Option<i64>,
}

fn info(l: &Loaded, x: u32, with_path: bool) -> NodeInfo {
    let t = &l.tree;
    let n = &t.nodes[x as usize];
    let path = if with_path { Some(t.path(x)) } else { None };
    let delta = match (&l.prev, n.is_dir()) {
        (Some(p), true) => {
            let pth = path.clone().unwrap_or_else(|| t.path(x));
            p.entries.get(&pth).map(|&old| n.size as i64 - old as i64)
        }
        _ => None,
    };
    NodeInfo {
        id: x,
        parent: if n.parent == NONE { None } else { Some(n.parent) },
        name: n.name.to_string(),
        size: n.size,
        alloc: n.alloc,
        files: n.files,
        dirs: n.dirs,
        is_dir: n.is_dir(),
        modified: n.modified,
        child_count: n.nkids,
        ext: if n.is_dir() { String::new() } else { ext_of(&n.name) },
        path,
        delta,
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Summary {
    root_id: u32,
    root_path: String,
    method: ScanMethod,
    scan_ms: u64,
    size: u64,
    alloc: u64,
    files: u32,
    dirs: u32,
    prev_time: Option<i64>,
    prev_total: Option<u64>,
    fallback_reason: Option<String>,
    from_cache: bool,
    stale: bool,
    captured_at: i64,
    live: &'static str,
}

fn summary(st: &AppState, l: &Loaded, fallback_reason: Option<String>) -> Summary {
    let t = &l.tree;
    let r = &t.nodes[t.root as usize];
    Summary {
        root_id: t.root,
        root_path: t.root_path.clone(),
        method: t.method,
        scan_ms: t.scan_ms,
        size: r.size,
        alloc: r.alloc,
        files: r.files,
        dirs: r.dirs,
        prev_time: l.prev.as_ref().map(|p| p.time),
        prev_total: l.prev.as_ref().map(|p| p.total),
        fallback_reason,
        from_cache: l.from_cache,
        stale: l.stale,
        captured_at: t.captured_at,
        live: if st.live_paused.load(Ordering::Relaxed) { "paused" } else { *st.live_mode.lock().unwrap() },
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    files: u64,
    dirs: u64,
    bytes: u64,
    total_hint: u64,
    done_hint: u64,
    elapsed_ms: u64,
    method: &'static str,
    phase: &'static str,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    ok: bool,
    cancelled: bool,
    error: Option<String>,
    summary: Option<Summary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformInfo {
    os: &'static str,
    elevated: bool,
    can_elevate: bool,
}

fn with<T>(st: &State<AppState>, f: impl FnOnce(&Loaded) -> T) -> Res<T> {
    let g = st.data.read().map_err(|e| e.to_string())?;
    let l = g.as_ref().ok_or("Сначала выполните сканирование")?;
    Ok(f(l))
}

fn check_id(l: &Loaded, id: u32) -> Res<()> {
    if l.tree.is_live(id) {
        Ok(())
    } else {
        Err("Элемент больше не существует".into())
    }
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("ClearScope"))
}
fn snapshots_dir(app: &AppHandle) -> PathBuf {
    data_dir(app).join("history")
}
fn cache_dir(app: &AppHandle) -> PathBuf {
    data_dir(app).join("cache")
}

fn cluster_for(path: &str) -> u64 {
    platform::drives()
        .into_iter()
        .filter(|d| path.to_lowercase().starts_with(&d.path.to_lowercase()))
        .map(|d| d.cluster)
        .next()
        .unwrap_or(4096)
}

#[cfg(windows)]
fn drive_letter_root(path: &str) -> Option<char> {
    let b = path.as_bytes();
    let ok = (b.len() == 2 || (b.len() == 3 && (b[2] == b'\\' || b[2] == b'/'))) && b[1] == b':' && b[0].is_ascii_alphabetic();
    ok.then(|| (b[0] as char).to_ascii_uppercase())
}

fn normalize_target(p: &str) -> String {
    let t = p.trim().trim_matches('"').to_string();
    if t.len() == 2 && t.ends_with(':') {
        format!("{t}\\")
    } else {
        t
    }
}

// ---------------------------------------------------------------- scanning

struct ScanOut {
    tree: Tree,
    journal: Option<(u64, i64)>,
    reason: Option<String>,
}

fn do_scan(path: &str, prefer_mft: bool, prog: &Progress) -> std::io::Result<ScanOut> {
    let cluster = cluster_for(path);
    #[allow(unused_mut)]
    let mut reason = None;
    #[cfg(windows)]
    if prefer_mft {
        if let Some(letter) = drive_letter_root(path) {
            let readers: Vec<std::fs::File> = (0..4).filter_map(|_| clearscope_core::mft::open_volume(letter).ok()).collect();
            if readers.is_empty() {
                let e = clearscope_core::mft::open_volume(letter).err();
                reason = Some(match e.and_then(|e| e.raw_os_error()) {
                    Some(5) => "Нет прав администратора: использован обычный (медленнее) режим".to_string(),
                    _ => "Том недоступен для прямого чтения: использован обычный режим".to_string(),
                });
            } else {
                // remember the journal position *before* reading, so changes
                // made during the scan are replayed by the live updater
                let journal = clearscope_core::usn::query(&readers[0]).ok().map(|j| (j.id, j.next));
                let root = format!("{letter}:\\");
                match clearscope_core::mft::scan_mft(&readers, &root, prog) {
                    Ok(tree) => return Ok(ScanOut { tree, journal, reason: None }),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => return Err(e),
                    Err(e) => reason = Some(format!("MFT недоступна ({e}): использован обычный режим")),
                }
            }
            prog.reset_counters();
            prog.mft.store(false, Ordering::Relaxed);
        }
    }
    #[cfg(not(windows))]
    let _ = prefer_mft;
    let tree = clearscope_core::walk::scan_walk(std::path::Path::new(path), cluster, prog)?;
    Ok(ScanOut { tree, journal: None, reason })
}

/// Reconcile a batch of changed paths into the loaded tree. Returns the
/// number of changes applied.
fn apply_changes(st: &AppState, mut paths: Vec<String>, ignore: &str) -> usize {
    paths.retain(|p| !p.to_lowercase().starts_with(ignore));
    if paths.is_empty() {
        return 0;
    }
    let (out, cluster) = {
        let mut g = st.data.write().unwrap();
        let Some(l) = g.as_mut() else { return 0 };
        let out = live::apply_paths(&mut l.tree, &mut paths, l.cluster);
        if out.changed > 0 {
            l.dirty = true;
        }
        (out, l.cluster)
    };
    let mut changed = out.changed;
    if !out.new_dirs.is_empty() {
        let scanned: Vec<(String, Tree)> = out
            .new_dirs
            .into_iter()
            .filter_map(|d| clearscope_core::walk::scan_walk(std::path::Path::new(&d), cluster, &Progress::new()).ok().map(|t| (d, t)))
            .collect();
        let mut g = st.data.write().unwrap();
        if let Some(l) = g.as_mut() {
            changed += live::graft_dirs(&mut l.tree, scanned);
            l.dirty = true;
        }
    }
    changed
}

/// Catch up a cached NTFS tree using the USN journal. Returns Ok(true) when
/// the tree is now current, Ok(false) when a rescan is required.
#[cfg(windows)]
fn catch_up(st: &AppState, root: &str, ignore: &str, prog: &Progress) -> bool {
    use clearscope_core::usn;
    let Some(letter) = drive_letter_root(root) else { return false };
    let Ok(vol) = clearscope_core::mft::open_volume(letter) else { return false };
    let Ok(j) = usn::query(&vol) else { return false };
    let Some((id, next)) = st.data.read().unwrap().as_ref().and_then(|l| l.journal) else { return false };
    if id != j.id || next < j.lowest {
        return false;
    }
    prog.set_phase(Phase::CatchUp);
    let Ok((recs, new_next)) = usn::read(&vol, id, next, 3_000_000) else { return false };
    if recs.len() >= 3_000_000 {
        return false;
    }
    prog.total_hint.store(recs.len() as u64, Ordering::Relaxed);
    let paths = usn::records_to_paths(&vol, &recs);
    prog.done_hint.store(recs.len() as u64, Ordering::Relaxed);
    apply_changes(st, paths, ignore);
    if let Some(l) = st.data.write().unwrap().as_mut() {
        l.journal = Some((id, new_next));
    }
    true
}

enum Job {
    Scan { path: String, prefer_mft: bool },
    Cache { path: String },
}

fn spawn_progress_emitter(app: &AppHandle, prog: Arc<Progress>, finished: Arc<AtomicBool>) {
    let app = app.clone();
    let t0 = Instant::now();
    std::thread::spawn(move || {
        while !finished.load(Ordering::Relaxed) {
            let _ = app.emit(
                "scan-progress",
                ProgressEvent {
                    files: prog.files.load(Ordering::Relaxed),
                    dirs: prog.dirs.load(Ordering::Relaxed),
                    bytes: prog.bytes.load(Ordering::Relaxed),
                    total_hint: prog.total_hint.load(Ordering::Relaxed),
                    done_hint: prog.done_hint.load(Ordering::Relaxed),
                    elapsed_ms: t0.elapsed().as_millis() as u64,
                    method: if prog.mft.load(Ordering::Relaxed) { "mft" } else { "walk" },
                    phase: prog.phase().as_str(),
                },
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

fn run_job(app: AppHandle, job: Job) -> Res<()> {
    let st = app.state::<AppState>();
    if st.busy.swap(true, Ordering::SeqCst) {
        return Err("Сканирование уже идёт".into());
    }
    st.live_gen.fetch_add(1, Ordering::SeqCst); // stop current watcher
    let prog = st.prog.clone();
    prog.reset();
    let finished = Arc::new(AtomicBool::new(false));
    spawn_progress_emitter(&app, prog.clone(), finished.clone());
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let ignore = data_dir(&app).to_string_lossy().to_lowercase();
        // save pending live changes of the tree we're replacing
        save_cache_if_dirty(&app, true);
        let mut fresh = true;
        let result: Result<Option<String>, std::io::Error> = (|| {
            match job {
                Job::Cache { path } => {
                    prog.set_phase(Phase::Loading);
                    match cache::load(&cache_dir(&app), &path) {
                        Ok((tree, meta)) => {
                            let cluster = cluster_for(&tree.root_path);
                            let prev = snapshot::load(&snapshots_dir(&app), &tree.root_path);
                            let journal = (meta.journal_id != 0).then_some((meta.journal_id, meta.next_usn));
                            *st.data.write().unwrap() = Some(Loaded { tree, prev, journal, from_cache: true, stale: true, dirty: false, cluster, last_save: Instant::now() });
                            fresh = false;
                            #[cfg(windows)]
                            if journal.is_some() {
                                if catch_up(&st, &path, &ignore, &prog) {
                                    if let Some(l) = st.data.write().unwrap().as_mut() {
                                        l.stale = false;
                                    }
                                    return Ok(None);
                                }
                                if platform::is_elevated() {
                                    // journal wrapped or reset: a fresh MFT scan is fast anyway
                                    prog.reset_counters();
                                    let out = do_scan(&path, true, &prog)?;
                                    store_fresh(&app, out.tree, out.journal);
                                    fresh = true;
                                    return Ok(Some("Журнал изменений устарел: выполнено полное сканирование".into()));
                                }
                            }
                            let _ = &ignore;
                            Ok(Some("Показан сохранённый результат. Нажмите «Обновить», чтобы пересканировать".into()))
                        }
                        Err(_) => {
                            let out = do_scan(&path, true, &prog)?;
                            let r = out.reason.clone();
                            store_fresh(&app, out.tree, out.journal);
                            Ok(r)
                        }
                    }
                }
                Job::Scan { path, prefer_mft } => {
                    let out = do_scan(&path, prefer_mft, &prog)?;
                    let r = out.reason.clone();
                    store_fresh(&app, out.tree, out.journal);
                    Ok(r)
                }
            }
        })();
        finished.store(true, Ordering::Relaxed);
        let ev = match result {
            Ok(reason) => {
                let g = st.data.read().unwrap();
                let s = g.as_ref().map(|l| summary(&st, l, reason));
                DoneEvent { ok: s.is_some(), cancelled: false, error: None, summary: s }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => DoneEvent { ok: false, cancelled: true, error: None, summary: None },
            Err(e) => DoneEvent { ok: false, cancelled: false, error: Some(e.to_string()), summary: None },
        };
        st.busy.store(false, Ordering::SeqCst);
        let ok = ev.ok;
        let _ = app.emit("scan-done", ev);
        if ok {
            if fresh {
                save_cache_if_dirty(&app, true);
            }
            start_live(app.clone());
        }
    });
    Ok(())
}

fn store_fresh(app: &AppHandle, tree: Tree, journal: Option<(u64, i64)>) {
    let st = app.state::<AppState>();
    let sdir = snapshots_dir(app);
    let prev = snapshot::load(&sdir, &tree.root_path);
    // writing the new snapshot isn't needed to show results: do it off the critical path
    let snap = snapshot::take(&tree);
    std::thread::spawn(move || {
        let _ = snapshot::save(&sdir, &snap);
    });
    let cluster = cluster_for(&tree.root_path);
    *st.data.write().unwrap() = Some(Loaded { tree, prev, journal, from_cache: false, stale: false, dirty: true, cluster, last_save: Instant::now() });
}

fn save_cache_if_dirty(app: &AppHandle, force: bool) {
    let st = app.state::<AppState>();
    let dir = cache_dir(app);
    let g = st.data.read().unwrap();
    let Some(l) = g.as_ref() else { return };
    if !l.dirty || (!force && l.last_save.elapsed() < Duration::from_secs(90)) {
        return;
    }
    let (jid, next) = l.journal.unwrap_or((0, 0));
    let ok = cache::save(&dir, &l.tree, jid, next).is_ok();
    drop(g);
    if ok {
        if let Some(l) = st.data.write().unwrap().as_mut() {
            l.dirty = false;
            l.last_save = Instant::now();
        }
    }
}

// ---------------------------------------------------------------- live updates

fn start_live(app: AppHandle) {
    let st = app.state::<AppState>();
    let gen = st.live_gen.load(Ordering::SeqCst);
    let (root, journal) = match st.data.read().unwrap().as_ref() {
        Some(l) => (l.tree.root_path.clone(), l.journal),
        None => return,
    };
    let ignore = data_dir(&app).to_string_lossy().to_lowercase();
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let alive = || st.live_gen.load(Ordering::SeqCst) == gen;
        let mut last_emit = Instant::now() - Duration::from_secs(10);
        let mut pending = 0usize;
        let emit = |n: usize, force: bool, last: &mut Instant, pending: &mut usize| {
            *pending += n;
            if *pending > 0 && (force || last.elapsed() > Duration::from_millis(1500)) {
                let g = st.data.read().unwrap();
                if let Some(l) = g.as_ref() {
                    let _ = app.emit("tree-changed", summary(&st, l, None));
                }
                *pending = 0;
                *last = Instant::now();
            }
        };

        #[cfg(windows)]
        if let Some((jid, _)) = journal {
            use clearscope_core::usn;
            if let Some(vol) = drive_letter_root(&root).and_then(|c| clearscope_core::mft::open_volume(c).ok()) {
                *st.live_mode.lock().unwrap() = "usn";
                let _ = app.emit("live-status", "usn");
                while alive() {
                    std::thread::sleep(Duration::from_millis(1000));
                    if !alive() {
                        break;
                    }
                    if st.live_paused.load(Ordering::Relaxed) {
                        continue;
                    }
                    let Some((_, next)) = st.data.read().unwrap().as_ref().and_then(|l| l.journal) else { break };
                    match usn::read(&vol, jid, next, 500_000) {
                        Ok((recs, new_next)) => {
                            let n = if recs.is_empty() { 0 } else { apply_changes(&st, usn::records_to_paths(&vol, &recs), &ignore) };
                            if let Some(l) = st.data.write().unwrap().as_mut() {
                                l.journal = Some((jid, new_next));
                            }
                            emit(n, false, &mut last_emit, &mut pending);
                        }
                        Err(_) => {
                            let _ = app.emit("live-resync", ());
                            break;
                        }
                    }
                    save_cache_if_dirty(&app, false);
                }
                if alive() {
                    *st.live_mode.lock().unwrap() = "off";
                }
                return;
            }
        }
        let _ = journal;

        // Generic watcher (ReadDirectoryChangesW / inotify / FSEvents).
        use notify::Watcher;
        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let watcher = notify::recommended_watcher(tx).and_then(|mut w| w.watch(std::path::Path::new(&root), notify::RecursiveMode::Recursive).map(|_| w));
        let Ok(_watcher) = watcher else {
            *st.live_mode.lock().unwrap() = "off";
            let _ = app.emit("live-status", "off");
            return;
        };
        *st.live_mode.lock().unwrap() = "watch";
        let _ = app.emit("live-status", "watch");
        let mut batch: Vec<String> = Vec::new();
        let mut batch_start = Instant::now();
        while alive() {
            match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(Ok(ev)) => {
                    if ev.need_rescan() {
                        let _ = app.emit("live-resync", ());
                    }
                    if batch.is_empty() {
                        batch_start = Instant::now();
                    }
                    batch.extend(ev.paths.into_iter().map(|p| p.to_string_lossy().into_owned()));
                }
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
            if !batch.is_empty() && (batch_start.elapsed() > Duration::from_millis(800) || batch.len() > 20_000) && !st.live_paused.load(Ordering::Relaxed) {
                let n = apply_changes(&st, std::mem::take(&mut batch), &ignore);
                emit(n, false, &mut last_emit, &mut pending);
            }
            emit(0, false, &mut last_emit, &mut pending);
            save_cache_if_dirty(&app, false);
        }
        if alive() {
            *st.live_mode.lock().unwrap() = "off";
        }
    });
}

// ---------------------------------------------------------------- commands

#[tauri::command(async)]
fn list_drives() -> Vec<platform::DriveInfo> {
    platform::drives()
}

#[tauri::command]
fn platform_info() -> PlatformInfo {
    PlatformInfo { os: std::env::consts::OS, elevated: platform::is_elevated(), can_elevate: cfg!(windows) }
}

#[tauri::command]
fn restart_as_admin(app: AppHandle, target: Option<String>) -> Res<()> {
    save_cache_if_dirty(&app, true);
    if platform::relaunch_as_admin(target.as_deref()) {
        app.exit(0);
        Ok(())
    } else {
        Err("Не удалось перезапустить с правами администратора".into())
    }
}

#[tauri::command]
fn startup_target() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == "--scan").and_then(|i| args.get(i + 1)).map(|s| normalize_target(s))
}

#[tauri::command(async)]
fn start_scan(app: AppHandle, path: String, prefer_mft: bool) -> Res<()> {
    run_job(app, Job::Scan { path: normalize_target(&path), prefer_mft })
}

#[tauri::command(async)]
fn open_cached(app: AppHandle, path: String) -> Res<()> {
    run_job(app, Job::Cache { path: normalize_target(&path) })
}

#[tauri::command(async)]
fn list_cached(app: AppHandle) -> Vec<cache::CacheMeta> {
    cache::list(&cache_dir(&app))
}

#[tauri::command(async)]
fn forget_cache(app: AppHandle, path: String) {
    cache::remove(&cache_dir(&app), &path);
}

#[tauri::command]
fn cancel_scan(state: State<AppState>) {
    state.prog.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn set_live(app: AppHandle, enabled: bool) {
    let st = app.state::<AppState>();
    st.live_paused.store(!enabled, Ordering::Relaxed);
}

#[tauri::command(async)]
fn get_summary(state: State<AppState>) -> Res<Option<Summary>> {
    let g = state.data.read().map_err(|e| e.to_string())?;
    Ok(g.as_ref().map(|l| summary(&state, l, None)))
}

#[tauri::command(async)]
fn get_node(state: State<AppState>, id: u32) -> Res<NodeInfo> {
    with(&state, |l| check_id(l, id).map(|_| info(l, id, true)))?
}

#[tauri::command(async)]
fn get_children(state: State<AppState>, id: u32, limit: Option<usize>) -> Res<Vec<NodeInfo>> {
    with(&state, |l| {
        check_id(l, id)?;
        Ok(l.tree.kids_sorted(id).into_iter().take(limit.unwrap_or(5000)).map(|c| info(l, c, false)).collect())
    })?
}

#[tauri::command(async)]
fn get_ancestors(state: State<AppState>, id: u32) -> Res<Vec<NodeInfo>> {
    with(&state, |l| {
        check_id(l, id)?;
        Ok(l.tree.ancestors(id).into_iter().map(|x| info(l, x, false)).collect())
    })?
}

#[tauri::command(async)]
fn find_by_path(state: State<AppState>, path: String) -> Res<Option<u32>> {
    with(&state, |l| l.tree.find_path(&path))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TmNode {
    id: u32,
    name: String,
    size: u64,
    is_dir: bool,
    ext: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<TmNode>,
}

#[tauri::command(async)]
fn get_treemap(state: State<AppState>, id: u32, max_nodes: Option<usize>) -> Res<TmNode> {
    with(&state, |l| {
        check_id(l, id)?;
        let t = &l.tree;
        let max = max_nodes.unwrap_or(9000).clamp(100, 50_000);
        let root_size = t.nodes[id as usize].size.max(1);
        let min_size = root_size / 200_000;
        let mut include: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        let mut count = 1usize;
        let mut heap = std::collections::BinaryHeap::new();
        heap.push((root_size, id));
        while let Some((_, x)) = heap.pop() {
            if count >= max {
                break;
            }
            let mut inc = Vec::new();
            // only items big enough to be visible need sorting (folders like
            // WinSxS have tens of thousands of tiny entries)
            let mut big: Vec<u32> = t.kids(x).filter(|&c| t.nodes[c as usize].size > min_size).collect();
            big.sort_unstable_by(|&a, &b| t.nodes[b as usize].size.cmp(&t.nodes[a as usize].size));
            for c in big {
                let n = &t.nodes[c as usize];
                if count >= max {
                    break;
                }
                inc.push(c);
                count += 1;
                if n.is_dir() && n.nkids > 0 {
                    heap.push((n.size, c));
                }
            }
            include.insert(x, inc);
        }
        fn build(t: &Tree, x: u32, inc: &std::collections::HashMap<u32, Vec<u32>>) -> TmNode {
            let n = &t.nodes[x as usize];
            let children = inc.get(&x).map(|v| v.iter().map(|&c| build(t, c, inc)).collect()).unwrap_or_default();
            TmNode { id: x, name: n.name.to_string(), size: n.size, is_dir: n.is_dir(), ext: if n.is_dir() { String::new() } else { ext_of(&n.name) }, children }
        }
        Ok(build(t, id, &include))
    })?
}

#[tauri::command(async)]
fn get_top_files(state: State<AppState>, id: u32, limit: Option<usize>) -> Res<Vec<NodeInfo>> {
    with(&state, |l| {
        check_id(l, id)?;
        Ok(analysis::top_files(&l.tree, id, limit.unwrap_or(500)).into_iter().map(|x| info(l, x, true)).collect())
    })?
}

#[tauri::command(async)]
fn get_ext_stats(state: State<AppState>, id: u32, limit: Option<usize>) -> Res<Vec<analysis::ExtStat>> {
    with(&state, |l| {
        check_id(l, id)?;
        Ok(analysis::ext_stats(&l.tree, id, limit.unwrap_or(300)))
    })?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResult {
    total: usize,
    items: Vec<NodeInfo>,
    took_ms: u64,
}

#[tauri::command(async)]
fn search(state: State<AppState>, id: u32, query: String, limit: Option<usize>) -> Res<SearchResult> {
    with(&state, |l| {
        check_id(l, id)?;
        let t0 = Instant::now();
        let (hits, total) = analysis::search(&l.tree, id, &query, limit.unwrap_or(1000));
        Ok(SearchResult { total, items: hits.into_iter().map(|x| info(l, x, true)).collect(), took_ms: t0.elapsed().as_millis() as u64 })
    })?
}

#[tauri::command(async)]
fn get_cleanup(state: State<AppState>, id: u32) -> Res<Vec<analysis::CleanupGroup>> {
    with(&state, |l| {
        check_id(l, id)?;
        Ok(analysis::cleanup(&l.tree, id, 200))
    })?
}

#[tauri::command(async)]
fn get_changes(state: State<AppState>, limit: Option<usize>) -> Res<Option<Vec<snapshot::Change>>> {
    with(&state, |l| l.prev.as_ref().map(|p| snapshot::diff(&l.tree, p, limit.unwrap_or(300))))
}

#[tauri::command]
async fn find_duplicates(app: AppHandle, id: u32, min_size: u64) -> Res<Vec<dupes::DupGroup>> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        st.dup_prog.reset();
        let g = st.data.read().map_err(|e| e.to_string())?;
        let l = g.as_ref().ok_or("Сначала выполните сканирование")?;
        check_id(l, id)?;
        let r = dupes::find_duplicates(&l.tree, id, min_size, 500, &st.dup_prog);
        if st.dup_prog.cancelled() {
            return Err("Отменено".to_string());
        }
        Ok(r)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DupProgress {
    done: u64,
    total: u64,
    bytes: u64,
}

#[tauri::command]
fn get_dupe_progress(state: State<AppState>) -> DupProgress {
    let p = &state.dup_prog;
    DupProgress { done: p.done_hint.load(Ordering::Relaxed), total: p.total_hint.load(Ordering::Relaxed), bytes: p.bytes.load(Ordering::Relaxed) }
}

#[tauri::command]
fn cancel_duplicates(state: State<AppState>) {
    state.dup_prog.cancel.store(true, Ordering::Relaxed);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteCheck {
    id: u32,
    name: String,
    path: String,
    size: u64,
    is_dir: bool,
    level: safety::Level,
    reason: String,
}

fn verdict(l: &Loaded, x: u32) -> safety::Verdict {
    if x == l.tree.root {
        return safety::Verdict { level: safety::Level::Blocked, reason: "Это корень текущего анализа".into() };
    }
    safety::classify(&l.tree.path(x), l.tree.nodes[x as usize].is_dir())
}

#[tauri::command(async)]
fn check_delete(state: State<AppState>, ids: Vec<u32>) -> Res<Vec<DeleteCheck>> {
    with(&state, |l| {
        ids.iter()
            .filter(|&&x| l.tree.is_live(x))
            .map(|&x| {
                let n = &l.tree.nodes[x as usize];
                let v = verdict(l, x);
                DeleteCheck { id: x, name: n.name.to_string(), path: l.tree.path(x), size: n.size, is_dir: n.is_dir(), level: v.level, reason: v.reason }
            })
            .collect()
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteFailure {
    path: String,
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteResult {
    deleted: Vec<u32>,
    failed: Vec<DeleteFailure>,
    freed: u64,
}

#[tauri::command]
async fn delete_items(app: AppHandle, ids: Vec<u32>, permanent: bool, allow_warn: bool) -> Res<DeleteResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let mut res = DeleteResult { deleted: vec![], failed: vec![], freed: 0 };
        let targets: Vec<(u32, String, bool, u64)> = {
            let g = st.data.read().map_err(|e| e.to_string())?;
            let l = g.as_ref().ok_or("Нет данных")?;
            let mut v = Vec::new();
            for &x in &ids {
                if !l.tree.is_live(x) {
                    continue;
                }
                // skip items inside another selected folder
                if ids.iter().any(|&o| o != x && l.tree.is_live(o) && l.tree.is_within(x, o)) {
                    continue;
                }
                let path = l.tree.path(x);
                let vd = verdict(l, x);
                if vd.level == safety::Level::Blocked || (vd.level == safety::Level::Warn && !allow_warn) {
                    res.failed.push(DeleteFailure { path, error: vd.reason });
                    continue;
                }
                let n = &l.tree.nodes[x as usize];
                v.push((x, path, n.is_dir(), n.size));
            }
            v
        };
        for (x, path, is_dir, size) in targets {
            let r = if permanent {
                if is_dir { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) }.map_err(|e| e.to_string())
            } else {
                trash::delete(&path).map_err(|e| e.to_string())
            };
            match r {
                Ok(()) => {
                    res.deleted.push(x);
                    res.freed += size;
                }
                Err(error) => res.failed.push(DeleteFailure { path, error }),
            }
        }
        if !res.deleted.is_empty() {
            let mut g = st.data.write().map_err(|e| e.to_string())?;
            if let Some(l) = g.as_mut() {
                for &x in &res.deleted {
                    l.tree.remove(x);
                }
                l.dirty = true;
            }
        }
        Ok(res)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn open_item(state: State<AppState>, id: u32) -> Res<()> {
    let p = with(&state, |l| l.tree.path(id))?;
    platform::open(&p);
    Ok(())
}

#[tauri::command(async)]
fn reveal_item(state: State<AppState>, id: u32) -> Res<()> {
    let p = with(&state, |l| l.tree.path(id))?;
    platform::reveal(&p);
    Ok(())
}

#[tauri::command]
async fn export_csv(app: AppHandle, id: u32, path: String) -> Res<u64> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Write;
        let st = app.state::<AppState>();
        let g = st.data.read().map_err(|e| e.to_string())?;
        let l = g.as_ref().ok_or("Нет данных")?;
        let f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut w = std::io::BufWriter::with_capacity(1 << 20, f);
        let _ = w.write_all(b"\xEF\xBB\xBF");
        writeln!(w, "Path;Type;Size;Allocated;Files;Folders;Modified").map_err(|e| e.to_string())?;
        let mut n = 0u64;
        for x in l.tree.subtree(id) {
            let nd = &l.tree.nodes[x as usize];
            let p = l.tree.path(x).replace('"', "\"\"");
            writeln!(w, "\"{}\";{};{};{};{};{};{}", p, if nd.is_dir() { "dir" } else { "file" }, nd.size, nd.alloc, nd.files, nd.dirs, nd.modified).map_err(|e| e.to_string())?;
            n += 1;
        }
        w.flush().map_err(|e| e.to_string())?;
        Ok(n)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .setup(|app| {
            migrate_old_data(app);
            desktop_webview(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_drives,
            platform_info,
            restart_as_admin,
            startup_target,
            start_scan,
            open_cached,
            list_cached,
            forget_cache,
            cancel_scan,
            set_live,
            get_summary,
            get_node,
            get_children,
            get_ancestors,
            find_by_path,
            get_treemap,
            get_top_files,
            get_ext_stats,
            search,
            get_cleanup,
            get_changes,
            find_duplicates,
            get_dupe_progress,
            cancel_duplicates,
            check_delete,
            delete_items,
            open_item,
            reveal_item,
            export_csv
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClearScope")
        .run(|app, ev| {
            if let tauri::RunEvent::ExitRequested { .. } = ev {
                let st = app.state::<AppState>();
                st.live_gen.fetch_add(1, Ordering::SeqCst);
                save_cache_if_dirty(app, true);
            }
        });
}

/// Turn off browser-only behaviour of WebView2: native context menu, reload/find/print
/// accelerators (keys still reach the page, so F5 = rescan keeps working), zoom, status bar.
#[cfg(windows)]
fn desktop_webview(app: &mut tauri::App) {
    let Some(w) = app.get_webview_window("main") else { return };
    let _ = w.with_webview(|wv| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows::core::Interface;
        let Ok(core) = wv.controller().CoreWebView2() else { return };
        let Ok(s) = core.Settings() else { return };
        let _ = s.SetAreDefaultContextMenusEnabled(false);
        let _ = s.SetIsZoomControlEnabled(false);
        let _ = s.SetIsStatusBarEnabled(false);
        #[cfg(not(debug_assertions))]
        let _ = s.SetAreDevToolsEnabled(false);
        if let Ok(s3) = s.cast::<ICoreWebView2Settings3>() {
            let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
        }
    });
}

#[cfg(not(windows))]
fn desktop_webview(_app: &mut tauri::App) {}

/// The app used to be called SpaceScope: keep its saved scans and history.
fn migrate_old_data(app: &tauri::App) {
    let Ok(new) = app.path().app_data_dir() else { return };
    if new.exists() {
        return;
    }
    if let Some(old) = new.parent().map(|p| p.join("app.spacescope.desktop")) {
        if old.is_dir() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

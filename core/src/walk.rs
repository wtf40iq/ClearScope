//! Parallel directory walker (used when MFT access isn't available, for
//! non-NTFS volumes, single folders, and for grafting live changes).
use crate::progress::{Phase, Progress};
use crate::tree::{RawEntry, ScanMethod, Tree, NONE};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Mutex;

pub struct DirItem {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub alloc: u64,
    pub modified: i64,
}

// ------------------------------------------------------------- Windows
#[cfg(windows)]
mod sys {
    use super::DirItem;
    use std::path::Path;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, FIND_FIRST_EX_LARGE_FETCH, WIN32_FIND_DATAW,
    };

    const ATTR_DIR: u32 = 0x10;
    const ATTR_REPARSE: u32 = 0x400;
    const TAG_MOUNT_POINT: u32 = 0xA000_0003;
    const TAG_SYMLINK: u32 = 0xA000_000C;

    pub fn long_path(p: &Path) -> String {
        let s = p.to_string_lossy();
        if s.starts_with(r"\\?\") {
            s.to_string()
        } else if let Some(unc) = s.strip_prefix(r"\\") {
            format!(r"\\?\UNC\{unc}")
        } else {
            format!(r"\\?\{s}")
        }
    }

    fn ft(lo: u32, hi: u32) -> i64 {
        let v = ((hi as u64) << 32) | lo as u64;
        if v == 0 { 0 } else { (v / 10_000_000) as i64 - 11_644_473_600 }
    }

    /// FindFirstFileExW with large fetch + basic info (skips 8.3 names):
    /// noticeably faster than std::fs::read_dir.
    pub fn read_dir(path: &Path, cluster: u64) -> std::io::Result<Vec<DirItem>> {
        let mut pat = long_path(path);
        if !pat.ends_with('\\') {
            pat.push('\\');
        }
        pat.push('*');
        let w: Vec<u16> = pat.encode_utf16().chain(std::iter::once(0)).collect();
        let mut fd: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        let h = unsafe {
            FindFirstFileExW(w.as_ptr(), FindExInfoBasic, &mut fd as *mut _ as *mut core::ffi::c_void, FindExSearchNameMatch, std::ptr::null(), FIND_FIRST_EX_LARGE_FETCH)
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let mut out = Vec::new();
        loop {
            let len = fd.cFileName.iter().position(|&c| c == 0).unwrap_or(fd.cFileName.len());
            let nm = &fd.cFileName[..len];
            let dot = nm == [b'.' as u16] || nm == [b'.' as u16, b'.' as u16];
            let attrs = fd.dwFileAttributes;
            let link = attrs & ATTR_REPARSE != 0 && (fd.dwReserved0 == TAG_MOUNT_POINT || fd.dwReserved0 == TAG_SYMLINK);
            if !dot && !link {
                let is_dir = attrs & ATTR_DIR != 0;
                let size = ((fd.nFileSizeHigh as u64) << 32) | fd.nFileSizeLow as u64;
                let c = cluster.max(1);
                out.push(DirItem {
                    name: String::from_utf16_lossy(nm),
                    is_dir,
                    size: if is_dir { 0 } else { size },
                    alloc: if is_dir { 0 } else { size.div_ceil(c) * c },
                    modified: ft(fd.ftLastWriteTime.dwLowDateTime, fd.ftLastWriteTime.dwHighDateTime),
                });
            }
            if unsafe { FindNextFileW(h, &mut fd) } == 0 {
                break;
            }
        }
        unsafe { FindClose(h) };
        Ok(out)
    }
}

// ------------------------------------------------------------- Unix / other
#[cfg(not(windows))]
mod sys {
    use super::DirItem;
    use std::path::Path;
    use std::time::UNIX_EPOCH;

    #[cfg(unix)]
    fn alloc(md: &std::fs::Metadata) -> u64 {
        use std::os::unix::fs::MetadataExt;
        md.blocks() * 512
    }
    #[cfg(not(unix))]
    fn alloc(md: &std::fs::Metadata) -> u64 {
        md.len()
    }
    #[cfg(unix)]
    pub fn dev(md: &std::fs::Metadata) -> u64 {
        use std::os::unix::fs::MetadataExt;
        md.dev()
    }
    #[cfg(not(unix))]
    pub fn dev(_md: &std::fs::Metadata) -> u64 {
        0
    }

    pub fn read_dir(path: &Path, _cluster: u64) -> std::io::Result<Vec<DirItem>> {
        let root_dev = dev(&std::fs::symlink_metadata(path)?);
        let mut out = Vec::new();
        for e in std::fs::read_dir(path)?.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            if ft.is_dir() && dev(&md) != root_dev {
                continue; // stay on one filesystem (like du -x)
            }
            let modified = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            out.push(DirItem {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir: ft.is_dir(),
                size: if ft.is_dir() { 0 } else { md.len() },
                alloc: if ft.is_dir() { 0 } else { alloc(&md) },
                modified,
            });
        }
        Ok(out)
    }
}

pub use sys::read_dir;

struct Ctx<'a> {
    out: Mutex<Vec<RawEntry>>,
    prog: &'a Progress,
    cluster: u64,
}

fn visit<'s>(s: &rayon::Scope<'s>, ctx: &'s Ctx<'s>, dir: PathBuf, idx: u32) {
    if ctx.prog.cancelled() {
        return;
    }
    let Ok(items) = read_dir(&dir, ctx.cluster) else { return };
    let mut local: Vec<RawEntry> = Vec::with_capacity(items.len());
    let mut subdirs: Vec<(usize, PathBuf)> = Vec::new();
    let (mut nf, mut nb) = (0u64, 0u64);
    for it in items {
        if it.is_dir {
            subdirs.push((local.len(), dir.join(&it.name)));
        } else {
            nf += 1;
            nb += it.size;
        }
        local.push(RawEntry { name: it.name.into(), parent: idx, size: it.size, alloc: it.alloc, is_dir: it.is_dir, modified: it.modified });
    }
    ctx.prog.files.fetch_add(nf, Relaxed);
    ctx.prog.bytes.fetch_add(nb, Relaxed);
    ctx.prog.dirs.fetch_add(subdirs.len() as u64, Relaxed);
    let base = {
        let mut out = ctx.out.lock().unwrap();
        let b = out.len() as u32;
        out.extend(local);
        b
    };
    for (k, p) in subdirs {
        s.spawn(move |s| visit(s, ctx, p, base + k as u32));
    }
}

/// Scan `root` with a parallel directory walk. `cluster` is used to estimate
/// allocated size on platforms without block counts.
pub fn scan_walk(root: &Path, cluster: u64, prog: &Progress) -> std::io::Result<Tree> {
    let t0 = std::time::Instant::now();
    let md = std::fs::metadata(root)?;
    if !md.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a directory"));
    }
    prog.set_phase(Phase::Reading);
    let root_path = root.to_string_lossy().to_string();
    let ctx = Ctx {
        out: Mutex::new(vec![RawEntry { name: root_path.clone().into(), parent: NONE, size: 0, alloc: 0, is_dir: true, modified: 0 }]),
        prog,
        cluster,
    };
    let rp = root.to_path_buf();
    // Directory enumeration is latency-bound (each FindNextFile batch waits on
    // the file system), so we run many more threads than cores: this keeps the
    // SSD/NVMe queue full and is 2-4x faster than a cores-sized pool on cold cache.
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let threads = (cores * 4).clamp(8, 64);
    match rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("walk-{i}")).build() {
        Ok(pool) => pool.scope(|s| visit(s, &ctx, rp, 0)),
        Err(_) => rayon::scope(|s| visit(s, &ctx, rp, 0)),
    }
    if prog.cancelled() {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
    }
    prog.set_phase(Phase::Building);
    let raw = ctx.out.into_inner().unwrap();
    let mut t = Tree::build(raw, 0, root_path, ScanMethod::Walk);
    t.scan_ms = t0.elapsed().as_millis() as u64;
    Ok(t)
}

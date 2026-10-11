//! Applying filesystem changes to a scanned tree without rescanning.
//!
//! Change sources (USN journal on NTFS, ReadDirectoryChangesW / inotify
//! elsewhere) only tell us *which paths* changed. We reconcile each path with
//! the real filesystem state: removed -> unlink, file -> upsert size, new
//! folder -> scan just that folder in parallel and graft it in.
use crate::tree::{is_sep, RawEntry, Tree};
use std::time::UNIX_EPOCH;

pub enum Probe {
    Gone,
    Skip,
    File { size: u64, alloc: u64, mtime: i64 },
    Dir,
}

pub fn probe(path: &str, cluster: u64) -> Probe {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Probe::Gone,
        Err(_) => Probe::Skip,
        Ok(md) => {
            if md.file_type().is_symlink() {
                return Probe::Skip;
            }
            if md.is_dir() {
                return Probe::Dir;
            }
            let size = md.len();
            let c = cluster.max(1);
            let mtime = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            Probe::File { size, alloc: size.div_ceil(c) * c, mtime }
        }
    }
}

pub fn split_parent(path: &str) -> Option<(&str, &str)> {
    let p = path.trim_end_matches(is_sep);
    let i = p.rfind(is_sep)?;
    let parent = if i == 0 || p[..i].ends_with(':') { &p[..=i] } else { &p[..i] };
    Some((parent, &p[i + 1..]))
}

/// Deepest missing folder on the way to `path` (to scan as a unit).
fn missing_ancestor(t: &Tree, path: &str) -> Option<String> {
    let comps = t.rel_components(path)?;
    let mut cur = t.root;
    let mut built = t.root_path.trim_end_matches(is_sep).to_string();
    for c in comps {
        built.push(std::path::MAIN_SEPARATOR);
        built.push_str(c);
        match t.find_child(cur, c) {
            Some(x) if t.nodes[x as usize].is_dir() => cur = x,
            _ => return Some(built),
        }
    }
    None
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub changed: usize,
    /// Folders that appeared and must be scanned, then grafted.
    pub new_dirs: Vec<String>,
}

/// Reconcile changed paths. Cheap (stat per path); new folders are returned
/// so the caller can scan them without holding the tree lock.
pub fn apply_paths(t: &mut Tree, paths: &mut Vec<String>, cluster: u64) -> Outcome {
    paths.sort_unstable_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    paths.dedup();
    let mut out = Outcome::default();
    for path in paths.iter() {
        if t.rel_components(path).is_none_or(|c| c.is_empty()) {
            continue; // outside of tree, or the root itself
        }
        if out.new_dirs.iter().any(|d| path.len() > d.len() && path.starts_with(d.as_str()) && path[d.len()..].starts_with(is_sep)) {
            continue; // covered by a folder we'll scan anyway
        }
        match probe(path, cluster) {
            Probe::Skip => {}
            Probe::Gone => {
                if let Some(id) = t.find_path(path) {
                    t.remove(id);
                    out.changed += 1;
                }
            }
            Probe::File { size, alloc, mtime } => match t.find_path(path) {
                Some(id) if !t.nodes[id as usize].is_dir() => {
                    let n = &t.nodes[id as usize];
                    if n.size != size || n.alloc != alloc || n.modified != mtime {
                        t.update_file(id, size, alloc, mtime);
                        out.changed += 1;
                    }
                }
                found => {
                    if let Some(id) = found {
                        t.remove(id);
                    }
                    let Some((parent, name)) = split_parent(path) else { continue };
                    match t.find_path(parent) {
                        Some(p) if t.nodes[p as usize].is_dir() => {
                            t.insert(p, RawEntry { name: name.into(), parent: p, size, alloc, is_dir: false, modified: mtime });
                            out.changed += 1;
                        }
                        _ => {
                            if let Some(m) = missing_ancestor(t, path) {
                                out.new_dirs.push(m);
                            }
                        }
                    }
                }
            },
            Probe::Dir => match t.find_path(path) {
                Some(id) if t.nodes[id as usize].is_dir() => {}
                Some(id) => {
                    t.remove(id);
                    out.changed += 1;
                    out.new_dirs.push(path.clone());
                }
                None => {
                    if let Some(m) = missing_ancestor(t, path) {
                        out.new_dirs.push(m);
                    }
                }
            },
        }
    }
    out.new_dirs.sort_by_key(|p| p.len());
    let mut kept: Vec<String> = Vec::new();
    for d in out.new_dirs.drain(..) {
        if !kept.iter().any(|k| d == *k || (d.starts_with(k.as_str()) && d[k.len()..].starts_with(is_sep))) {
            kept.push(d);
        }
    }
    out.new_dirs = kept;
    out
}

/// Graft separately scanned folders into the tree (replacing stale copies).
pub fn graft_dirs(t: &mut Tree, scanned: Vec<(String, Tree)>) -> usize {
    let mut n = 0;
    for (path, sub) in scanned {
        let Some((parent, name)) = split_parent(&path) else { continue };
        let Some(p) = t.find_path(parent) else { continue };
        if let Some(old) = t.find_child(p, name) {
            t.remove(old);
        }
        t.graft(p, name, &sub);
        n += 1;
    }
    n
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{walk, Progress};
    #[test]
    fn live_reconcile() {
        let dir = std::env::temp_dir().join(format!("ss-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::write(dir.join("a/f1"), vec![0u8; 1000]).unwrap();
        let mut t = walk::scan_walk(&dir, 4096, &Progress::new()).unwrap();
        assert_eq!(t.nodes[t.root as usize].size, 1000);
        let p = |s: &str| dir.join(s).to_string_lossy().to_string();
        // modify, create file, create nested folder, delete
        std::fs::write(dir.join("a/f1"), vec![0u8; 3000]).unwrap();
        std::fs::write(dir.join("f2"), vec![0u8; 500]).unwrap();
        std::fs::create_dir_all(dir.join("b/c")).unwrap();
        std::fs::write(dir.join("b/c/f3"), vec![0u8; 700]).unwrap();
        let mut paths = vec![p("a/f1"), p("f2"), p("b/c/f3"), p("b")];
        let out = apply_paths(&mut t, &mut paths, 4096);
        assert_eq!(out.new_dirs, vec![p("b")]);
        let sub = walk::scan_walk(std::path::Path::new(&out.new_dirs[0]), 4096, &Progress::new()).unwrap();
        graft_dirs(&mut t, vec![(out.new_dirs[0].clone(), sub)]);
        assert_eq!(t.nodes[t.root as usize].size, 4200);
        assert!(t.find_path(&p("b/c/f3")).is_some());
        std::fs::remove_dir_all(dir.join("a")).unwrap();
        let mut paths = vec![p("a")];
        apply_paths(&mut t, &mut paths, 4096);
        assert_eq!(t.nodes[t.root as usize].size, 1200);
        assert_eq!(t.nodes[t.root as usize].files, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

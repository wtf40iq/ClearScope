//! Duplicate file finder: size -> partial hash -> full hash.
use crate::progress::Progress;
use crate::tree::Tree;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::Ordering::Relaxed;
use xxhash_rust::xxh3::Xxh3;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DupGroup {
    pub size: u64,
    pub wasted: u64,
    pub items: Vec<DupItem>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DupItem {
    pub id: u32,
    pub path: String,
}

const PART: usize = 64 * 1024;

fn partial_hash(path: &str, size: u64) -> Option<u64> {
    let mut f = File::open(path).ok()?;
    let mut h = Xxh3::new();
    if size <= (PART * 2) as u64 {
        let mut all = Vec::with_capacity(size as usize);
        f.read_to_end(&mut all).ok()?;
        h.update(&all);
        return Some(h.digest());
    }
    let mut buf = vec![0u8; PART];
    f.read_exact(&mut buf).ok()?;
    h.update(&buf);
    {
        f.seek(SeekFrom::End(-(PART as i64))).ok()?;
        f.read_exact(&mut buf).ok()?;
        h.update(&buf);
    }
    Some(h.digest())
}

fn full_hash(path: &str, prog: &Progress) -> Option<u64> {
    let mut f = File::open(path).ok()?;
    let mut h = Xxh3::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if prog.cancelled() {
            return None;
        }
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        prog.bytes.fetch_add(n as u64, Relaxed);
    }
    Some(h.digest())
}

fn regroup<K: std::hash::Hash + Eq + Send>(items: Vec<(K, (u32, String, u64))>) -> Vec<Vec<(u32, String, u64)>> {
    let mut m: HashMap<K, Vec<(u32, String, u64)>> = HashMap::new();
    for (k, v) in items {
        m.entry(k).or_default().push(v);
    }
    m.into_values().filter(|g| g.len() > 1).collect()
}

pub fn find_duplicates(t: &Tree, id: u32, min_size: u64, limit: usize, prog: &Progress) -> Vec<DupGroup> {
    hash_groups(candidates(t, id, min_size), limit, prog)
}

/// Files that share their size with another file: (id, path, size).
/// Cheap; the tree is not needed after this.
pub fn candidates(t: &Tree, id: u32, min_size: u64) -> Vec<(u32, String, u64)> {
    let mut by_size: HashMap<u64, Vec<u32>> = HashMap::new();
    for x in t.subtree(id) {
        let n = &t.nodes[x as usize];
        if !n.is_dir() && n.size >= min_size.max(1) {
            by_size.entry(n.size).or_default().push(x);
        }
    }
    let cands: Vec<(u32, String, u64)> = by_size
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .flat_map(|(s, v)| v.into_iter().map(move |x| (x, s)))
        .map(|(x, s)| (x, t.path(x), s))
        .collect();
    cands
}

/// Hash the candidates and group identical files.
pub fn hash_groups(cands: Vec<(u32, String, u64)>, limit: usize, prog: &Progress) -> Vec<DupGroup> {
    prog.total_hint.store(cands.len() as u64, Relaxed);
    // Stage 1: partial hash
    let stage1: Vec<((u64, u64), (u32, String, u64))> = cands
        .into_par_iter()
        .filter_map(|(x, p, s)| {
            if prog.cancelled() {
                return None;
            }
            let h = partial_hash(&p, s);
            prog.done_hint.fetch_add(1, Relaxed);
            h.map(|h| ((s, h), (x, p, s)))
        })
        .collect();
    // Stage 2: full hash for groups still colliding (small files: partial == full)
    let mut confirmed: Vec<Vec<(u32, String, u64)>> = Vec::new();
    for g in regroup(stage1) {
        let size = g[0].2;
        if size <= (PART * 2) as u64 {
            confirmed.push(g); // partial hash already covered the whole file
            continue;
        }
        let hashed: Vec<((u64, u64), (u32, String, u64))> =
            g.into_par_iter().filter_map(|v| full_hash(&v.1, prog).map(|h| ((v.2, h), v))).collect();
        confirmed.extend(regroup(hashed));
    }
    let mut groups: Vec<DupGroup> = confirmed
        .into_iter()
        .map(|g| {
            let size = g[0].2;
            DupGroup { size, wasted: size * (g.len() as u64 - 1), items: g.into_iter().map(|(id, path, _)| DupItem { id, path }).collect() }
        })
        .collect();
    groups.sort_unstable_by(|a, b| b.wasted.cmp(&a.wasted));
    groups.truncate(limit);
    groups
}

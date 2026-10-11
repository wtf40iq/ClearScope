//! Scan history: store folder sizes after each scan and diff against the
//! previous scan ("what grew since last time").
use crate::tree::Tree;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const MAX_DEPTH: usize = 4;
const MIN_SIZE: u64 = 1 << 20;
const MAX_ENTRIES: usize = 40_000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Snapshot {
    pub root: String,
    pub time: i64,
    pub total: u64,
    pub files: u64,
    /// path -> size, for folders down to MAX_DEPTH bigger than MIN_SIZE
    pub entries: HashMap<String, u64>,
}

pub fn take(t: &Tree) -> Snapshot {
    let mut entries = HashMap::new();
    let mut stack = vec![(t.root, 0usize)];
    while let Some((x, d)) = stack.pop() {
        let n = &t.nodes[x as usize];
        if !n.is_dir() || n.size < MIN_SIZE || entries.len() >= MAX_ENTRIES {
            continue;
        }
        entries.insert(t.path(x), n.size);
        if d < MAX_DEPTH {
            for c in t.kids(x) {
                stack.push((c, d + 1));
            }
        }
    }
    let r = &t.nodes[t.root as usize];
    Snapshot {
        root: t.root_path.clone(),
        time: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
        total: r.size,
        files: r.files as u64,
        entries,
    }
}

pub fn file_name_for(root: &str) -> String {
    let h = xxhash_rust::xxh3::xxh3_64(root.to_lowercase().as_bytes());
    format!("snapshot-{h:016x}.json")
}

pub fn load(dir: &Path, root: &str) -> Option<Snapshot> {
    let data = std::fs::read(dir.join(file_name_for(root))).ok()?;
    serde_json::from_slice(&data).ok()
}

pub fn save(dir: &Path, s: &Snapshot) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let data = serde_json::to_vec(s).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(file_name_for(&s.root)), data)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub path: String,
    pub id: Option<u32>,
    pub old_size: u64,
    pub new_size: u64,
    pub delta: i64,
}

/// Biggest changes between `prev` and the current tree. Only "leaf-most"
/// meaningful changes are useful, so we report every tracked folder and let
/// the UI show depth via the path.
pub fn diff(t: &Tree, prev: &Snapshot, limit: usize) -> Vec<Change> {
    let cur = take(t);
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut stack = vec![(t.root, 0usize)];
    while let Some((x, d)) = stack.pop() {
        let n = &t.nodes[x as usize];
        if !n.is_dir() || n.size < MIN_SIZE {
            continue;
        }
        ids.insert(t.path(x), x);
        if d < MAX_DEPTH {
            for c in t.kids(x) {
                stack.push((c, d + 1));
            }
        }
    }
    let mut out: Vec<Change> = Vec::new();
    for (p, &new) in &cur.entries {
        let old = prev.entries.get(p).copied().unwrap_or(0);
        if old != new {
            out.push(Change { path: p.clone(), id: ids.get(p).copied(), old_size: old, new_size: new, delta: new as i64 - old as i64 });
        }
    }
    for (p, &old) in &prev.entries {
        if !cur.entries.contains_key(p) {
            out.push(Change { path: p.clone(), id: ids.get(p).copied(), old_size: old, new_size: 0, delta: -(old as i64) });
        }
    }
    out.sort_unstable_by(|a, b| b.delta.unsigned_abs().cmp(&a.delta.unsigned_abs()));
    out.truncate(limit);
    out
}

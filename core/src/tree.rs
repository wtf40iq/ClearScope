//! Mutable in-memory file tree: arena of nodes linked as first-child /
//! next-sibling lists. Supports cheap inserts, removals and size updates so
//! the tree can follow filesystem changes live.
use serde::{Deserialize, Serialize};

pub const NONE: u32 = u32::MAX;
pub const FLAG_DIR: u8 = 1;
pub const FLAG_DELETED: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanMethod {
    /// Direct NTFS Master File Table read (WizTree-style).
    Mft,
    /// Parallel directory walk (works everywhere, no admin needed).
    Walk,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Box<str>,
    pub parent: u32,
    pub first_child: u32,
    pub next_sibling: u32,
    pub nkids: u32,
    pub files: u32,
    pub dirs: u32,
    pub size: u64,
    pub alloc: u64,
    /// Unix seconds. For folders: newest item inside.
    pub modified: i64,
    pub flags: u8,
}

impl Node {
    #[inline]
    pub fn is_dir(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }
    #[inline]
    pub fn is_deleted(&self) -> bool {
        self.flags & FLAG_DELETED != 0
    }
}

/// Entry produced by scanners before the tree is finalized.
#[derive(Debug, Clone)]
pub struct RawEntry {
    pub name: Box<str>,
    pub parent: u32,
    pub size: u64,
    pub alloc: u64,
    pub is_dir: bool,
    pub modified: i64,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: u32,
    pub root_path: String,
    pub method: ScanMethod,
    pub scan_ms: u64,
    /// Unix time the data was captured (scan start).
    pub captured_at: i64,
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[inline]
pub fn names_eq(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if cfg!(windows) || cfg!(target_os = "macos") {
        a.len() == b.len() && a.eq_ignore_ascii_case(b) || (!a.is_ascii() && a.to_lowercase() == b.to_lowercase())
    } else {
        false
    }
}

pub fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

impl Tree {
    /// Finalize raw entries into a tree: link children and aggregate sizes.
    pub fn build(raw: Vec<RawEntry>, root: u32, root_path: String, method: ScanMethod) -> Tree {
        let n = raw.len();
        let is_dir: Vec<bool> = raw.iter().map(|r| r.is_dir).collect();
        let mut nodes: Vec<Node> = Vec::with_capacity(n);
        for (i, r) in raw.into_iter().enumerate() {
            let mut parent = r.parent;
            if i as u32 == root {
                parent = NONE;
            } else if parent == NONE || parent as usize >= n || parent as usize == i || !is_dir[parent as usize] {
                parent = root; // orphan -> attach to root
            }
            nodes.push(Node {
                name: r.name,
                parent,
                first_child: NONE,
                next_sibling: NONE,
                nkids: 0,
                files: if r.is_dir { 0 } else { 1 },
                dirs: 0,
                size: if r.is_dir { 0 } else { r.size },
                alloc: r.alloc,
                modified: r.modified,
                flags: if r.is_dir { FLAG_DIR } else { 0 },
            });
        }
        for i in (0..n).rev() {
            let p = nodes[i].parent;
            if p != NONE {
                nodes[i].next_sibling = nodes[p as usize].first_child;
                let pn = &mut nodes[p as usize];
                pn.first_child = i as u32;
                pn.nkids += 1;
            }
        }
        // Pre-order from root (cycle-safe), then aggregate in reverse.
        let mut order: Vec<u32> = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        if (root as usize) < n {
            let mut stack = vec![root];
            seen[root as usize] = true;
            while let Some(x) = stack.pop() {
                order.push(x);
                let mut c = nodes[x as usize].first_child;
                while c != NONE {
                    if !seen[c as usize] {
                        seen[c as usize] = true;
                        stack.push(c);
                    }
                    c = nodes[c as usize].next_sibling;
                }
            }
        }
        for (i, s) in seen.iter().enumerate() {
            if !s {
                nodes[i].flags |= FLAG_DELETED; // unreachable (cycle) -> hide
            }
        }
        for &x in order.iter().rev() {
            let p = nodes[x as usize].parent;
            if p == NONE {
                continue;
            }
            let (size, alloc, files, dirs, isd, m) = {
                let c = &nodes[x as usize];
                (c.size, c.alloc, c.files, c.dirs, c.is_dir(), c.modified)
            };
            let pn = &mut nodes[p as usize];
            pn.size += size;
            pn.alloc += alloc;
            pn.files += files;
            pn.dirs += dirs + isd as u32;
            pn.modified = pn.modified.max(m);
        }
        Tree { nodes, root, root_path, method, scan_ms: 0, captured_at: now_unix() }
    }

    #[inline]
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn is_live(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| !n.is_deleted())
    }

    /// Children in arbitrary order.
    pub fn kids(&self, id: u32) -> Kids<'_> {
        Kids { t: self, cur: self.nodes[id as usize].first_child }
    }

    /// Children sorted by size (desc).
    pub fn kids_sorted(&self, id: u32) -> Vec<u32> {
        let mut v: Vec<u32> = self.kids(id).collect();
        v.sort_unstable_by(|&a, &b| self.nodes[b as usize].size.cmp(&self.nodes[a as usize].size).then(a.cmp(&b)));
        v
    }

    pub fn ancestors(&self, mut id: u32) -> Vec<u32> {
        let mut v = Vec::new();
        while id != NONE {
            v.push(id);
            if id == self.root {
                break;
            }
            id = self.nodes[id as usize].parent;
        }
        v.reverse();
        v
    }

    pub fn depth(&self, id: u32) -> usize {
        self.ancestors(id).len().saturating_sub(1)
    }

    pub fn is_within(&self, mut id: u32, ancestor: u32) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            if id == self.root || id == NONE {
                return false;
            }
            id = self.nodes[id as usize].parent;
        }
    }

    /// Full filesystem path of a node.
    pub fn path(&self, id: u32) -> String {
        let chain = self.ancestors(id);
        let sep = std::path::MAIN_SEPARATOR;
        let mut s = self.root_path.clone();
        for &x in chain.iter().skip(1) {
            if !s.ends_with(is_sep) {
                s.push(sep);
            }
            s.push_str(&self.nodes[x as usize].name);
        }
        s
    }

    /// Path relative to the root, split into components; None if outside.
    pub fn rel_components<'a>(&self, path: &'a str) -> Option<Vec<&'a str>> {
        let root = self.root_path.trim_end_matches(is_sep);
        if path.len() < root.len() || !names_eq(&path[..root.len()], root) {
            return None;
        }
        let rest = &path[root.len()..];
        if !rest.is_empty() && !rest.starts_with(is_sep) {
            return None;
        }
        Some(rest.split(is_sep).filter(|s| !s.is_empty()).collect())
    }

    pub fn find_child(&self, parent: u32, name: &str) -> Option<u32> {
        self.kids(parent).find(|&c| names_eq(&self.nodes[c as usize].name, name))
    }

    pub fn find_path(&self, path: &str) -> Option<u32> {
        let comps = self.rel_components(path)?;
        let mut cur = self.root;
        for c in comps {
            cur = self.find_child(cur, c)?;
        }
        Some(cur)
    }

    /// Iterate all live nodes in a subtree (pre-order, including `id`).
    pub fn subtree(&self, id: u32) -> SubtreeIter<'_> {
        SubtreeIter { t: self, stack: vec![id] }
    }

    fn bubble(&mut self, mut p: u32, dsize: i64, dalloc: i64, dfiles: i64, ddirs: i64, mtime: i64) {
        while p != NONE {
            let n = &mut self.nodes[p as usize];
            n.size = (n.size as i64 + dsize).max(0) as u64;
            n.alloc = (n.alloc as i64 + dalloc).max(0) as u64;
            n.files = (n.files as i64 + dfiles).max(0) as u32;
            n.dirs = (n.dirs as i64 + ddirs).max(0) as u32;
            if mtime > n.modified {
                n.modified = mtime;
            }
            if p == self.root {
                break;
            }
            p = n.parent;
        }
    }

    /// Insert a new (empty, if dir) node under `parent`. Returns its id.
    pub fn insert(&mut self, parent: u32, e: RawEntry) -> u32 {
        let id = self.nodes.len() as u32;
        let first = self.nodes[parent as usize].first_child;
        let (size, alloc) = if e.is_dir { (0, e.alloc) } else { (e.size, e.alloc) };
        self.nodes.push(Node {
            name: e.name,
            parent,
            first_child: NONE,
            next_sibling: first,
            nkids: 0,
            files: if e.is_dir { 0 } else { 1 },
            dirs: 0,
            size,
            alloc,
            modified: e.modified,
            flags: if e.is_dir { FLAG_DIR } else { 0 },
        });
        let pn = &mut self.nodes[parent as usize];
        pn.first_child = id;
        pn.nkids += 1;
        self.bubble(parent, size as i64, alloc as i64, !e.is_dir as i64, e.is_dir as i64, e.modified);
        id
    }

    /// Copy a whole separately-scanned tree under `parent` (its root becomes
    /// a folder called `name`). Returns the new folder id.
    pub fn graft(&mut self, parent: u32, name: &str, sub: &Tree) -> u32 {
        let top = self.insert(parent, RawEntry { name: name.into(), parent, size: 0, alloc: 0, is_dir: true, modified: 0 });
        let mut stack = vec![(sub.root, top)];
        while let Some((s, d)) = stack.pop() {
            for c in sub.kids(s).collect::<Vec<_>>() {
                let n = &sub.nodes[c as usize];
                let id = self.insert(d, RawEntry { name: n.name.clone(), parent: d, size: n.size, alloc: if n.is_dir() { 0 } else { n.alloc }, is_dir: n.is_dir(), modified: n.modified });
                if n.is_dir() {
                    stack.push((c, id));
                }
            }
        }
        top
    }

    /// Update a file's size / mtime.
    pub fn update_file(&mut self, id: u32, size: u64, alloc: u64, mtime: i64) {
        let n = &mut self.nodes[id as usize];
        if n.is_dir() {
            return;
        }
        let (ds, da) = (size as i64 - n.size as i64, alloc as i64 - n.alloc as i64);
        n.size = size;
        n.alloc = alloc;
        n.modified = mtime;
        let p = n.parent;
        if ds != 0 || da != 0 {
            self.bubble(p, ds, da, 0, 0, mtime);
        }
    }

    /// Unlink a node (and its subtree), subtracting it from ancestors.
    pub fn remove(&mut self, id: u32) {
        if id == self.root || !self.is_live(id) {
            return;
        }
        let ids: Vec<u32> = self.subtree(id).collect();
        let (size, alloc, files, dirs, isd, parent) = {
            let n = &self.nodes[id as usize];
            (n.size, n.alloc, n.files, n.dirs, n.is_dir(), n.parent)
        };
        // unlink from sibling list
        let next = self.nodes[id as usize].next_sibling;
        if self.nodes[parent as usize].first_child == id {
            self.nodes[parent as usize].first_child = next;
        } else {
            let mut c = self.nodes[parent as usize].first_child;
            while c != NONE {
                if self.nodes[c as usize].next_sibling == id {
                    self.nodes[c as usize].next_sibling = next;
                    break;
                }
                c = self.nodes[c as usize].next_sibling;
            }
        }
        self.nodes[parent as usize].nkids = self.nodes[parent as usize].nkids.saturating_sub(1);
        for x in ids {
            self.nodes[x as usize].flags |= FLAG_DELETED;
        }
        self.bubble(parent, -(size as i64), -(alloc as i64), -(files as i64), -(dirs as i64 + isd as i64), 0);
    }

    pub fn live_count(&self) -> usize {
        self.subtree(self.root).count()
    }
}

pub struct Kids<'a> {
    t: &'a Tree,
    cur: u32,
}

impl Iterator for Kids<'_> {
    type Item = u32;
    #[inline]
    fn next(&mut self) -> Option<u32> {
        if self.cur == NONE {
            return None;
        }
        let x = self.cur;
        self.cur = self.t.nodes[x as usize].next_sibling;
        Some(x)
    }
}

pub struct SubtreeIter<'a> {
    t: &'a Tree,
    stack: Vec<u32>,
}

impl Iterator for SubtreeIter<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        let x = self.stack.pop()?;
        let mut c = self.t.nodes[x as usize].first_child;
        while c != NONE {
            self.stack.push(c);
            c = self.t.nodes[c as usize].next_sibling;
        }
        Some(x)
    }
}

/// Lowercased extension of a file name ("" if none).
pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 12 => name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(name: &str, parent: u32, size: u64, is_dir: bool) -> RawEntry {
        RawEntry { name: name.into(), parent, size, alloc: size, is_dir, modified: 0 }
    }
    fn sample() -> Tree {
        let raw = vec![
            e("C:\\", NONE, 0, true),
            e("a", 0, 0, true),
            e("x.bin", 1, 100, false),
            e("y.bin", 1, 300, false),
            e("z.txt", 0, 50, false),
            e("orphan", 999, 7, false),
        ];
        Tree::build(raw, 0, "C:\\".into(), ScanMethod::Walk)
    }
    #[test]
    fn build_aggregate_remove() {
        let mut t = sample();
        assert_eq!(t.nodes[0].size, 457);
        assert_eq!(t.nodes[0].files, 4);
        assert_eq!(t.nodes[0].dirs, 1);
        assert_eq!(t.kids_sorted(1), vec![3, 2]);
        assert!(t.path(3).ends_with("y.bin"));
        t.remove(1);
        assert_eq!(t.nodes[0].size, 57);
        assert_eq!(t.nodes[0].files, 2);
        assert_eq!(t.subtree(0).count(), 3);
        assert_eq!(t.nodes[0].nkids, 2);
    }
    #[test]
    fn insert_update_find() {
        let mut t = sample();
        let sep = std::path::MAIN_SEPARATOR;
        let a = t.find_path(&format!("C:\\{sep}a")).or_else(|| t.find_path("C:\\a")).unwrap();
        assert_eq!(a, 1);
        let n = t.insert(a, e("new.dat", a, 1000, false));
        assert_eq!(t.nodes[1].size, 1400);
        assert_eq!(t.nodes[0].size, 1457);
        t.update_file(n, 10, 10, 5);
        assert_eq!(t.nodes[0].size, 467);
        assert_eq!(t.nodes[0].files, 5);
        assert_eq!(t.find_path("C:\\a\\new.dat"), Some(n));
        assert_eq!(t.find_path("D:\\a"), None);
    }
}

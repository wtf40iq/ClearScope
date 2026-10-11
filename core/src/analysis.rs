//! Read-only analyses over a scanned tree.
use crate::tree::{ext_of, Tree};
use serde::Serialize;
use std::cmp::Reverse;
use rayon::prelude::*;
use std::collections::{BinaryHeap, HashMap};

#[derive(Serialize, Clone, Debug)]
pub struct ExtStat {
    pub ext: String,
    pub size: u64,
    pub count: u64,
}

#[inline]
fn ext_slice(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 12 => &name[i + 1..],
        _ => "",
    }
}

pub fn ext_stats(t: &Tree, id: u32, limit: usize) -> Vec<ExtStat> {
    let fold = |mut m: HashMap<Box<str>, (u64, u64)>, x: u32, buf: &mut String| {
        let n = &t.nodes[x as usize];
        if n.is_dir() {
            return m;
        }
        buf.clear();
        buf.push_str(ext_slice(&n.name));
        buf.make_ascii_lowercase();
        match m.get_mut(buf.as_str()) {
            Some(e) => {
                e.0 += n.size;
                e.1 += 1;
            }
            None => {
                m.insert(buf.as_str().into(), (n.size, 1));
            }
        }
        m
    };
    let m: HashMap<Box<str>, (u64, u64)> = if id == t.root {
        (0..t.nodes.len() as u32)
            .into_par_iter()
            .with_min_len(8192)
            .filter(|&x| !t.nodes[x as usize].is_deleted())
            .fold(|| (HashMap::new(), String::new()), |(m, mut buf), x| (fold(m, x, &mut buf), buf))
            .map(|(m, _)| m)
            .reduce(HashMap::new, |mut a, b| {
                for (k, v) in b {
                    let e = a.entry(k).or_default();
                    e.0 += v.0;
                    e.1 += v.1;
                }
                a
            })
    } else {
        let mut buf = String::new();
        t.subtree(id).fold(HashMap::new(), |m, x| fold(m, x, &mut buf))
    };
    let mut v: Vec<ExtStat> = m.into_iter().map(|(ext, (size, count))| ExtStat { ext: ext.into(), size, count }).collect();
    v.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    v.truncate(limit);
    v
}

pub fn top_files(t: &Tree, id: u32, limit: usize) -> Vec<u32> {
    let mut h: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(limit + 1);
    for x in t.subtree(id) {
        let n = &t.nodes[x as usize];
        if n.is_dir() {
            continue;
        }
        if h.len() < limit {
            h.push(Reverse((n.size, x)));
        } else if let Some(Reverse((s, _))) = h.peek() {
            if n.size > *s {
                h.pop();
                h.push(Reverse((n.size, x)));
            }
        }
    }
    let mut v: Vec<(u64, u32)> = h.into_iter().map(|Reverse(p)| p).collect();
    v.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|p| p.1).collect()
}

/// Search names in a subtree. Supports plain substring ("report"),
/// extension filters ("*.mp4" or "ext:mp4"), and size filters (">1gb").
pub fn search(t: &Tree, id: u32, query: &str, limit: usize) -> (Vec<u32>, usize) {
    let mut ext: Option<String> = None;
    let mut min_size: u64 = 0;
    let mut words: Vec<String> = Vec::new();
    for tok in query.split_whitespace() {
        let tl = tok.to_lowercase();
        if let Some(e) = tl.strip_prefix("*.").or_else(|| tl.strip_prefix("ext:")) {
            ext = Some(e.to_string());
        } else if let Some(s) = tl.strip_prefix('>') {
            min_size = parse_size(s).unwrap_or(0);
        } else {
            words.push(tl);
        }
    }
    let matches = |x: u32, buf: &mut String| -> bool {
        if x == id {
            return false;
        }
        let n = &t.nodes[x as usize];
        if n.size < min_size {
            return false;
        }
        if let Some(e) = &ext {
            if n.is_dir() || !ext_slice(&n.name).eq_ignore_ascii_case(e) {
                return false;
            }
        }
        if !words.is_empty() {
            buf.clear();
            if n.name.is_ascii() {
                buf.push_str(&n.name);
                buf.make_ascii_lowercase();
            } else {
                buf.extend(n.name.chars().flat_map(char::to_lowercase));
            }
            if !words.iter().all(|w| buf.contains(w.as_str())) {
                return false;
            }
        }
        true
    };
    let mut hits: Vec<u32> = if id == t.root {
        (0..t.nodes.len() as u32)
            .into_par_iter()
            .with_min_len(8192)
            .filter(|&x| !t.nodes[x as usize].is_deleted())
            .map_init(String::new, |buf, x| matches(x, buf).then_some(x))
            .flatten()
            .collect()
    } else {
        let mut buf = String::new();
        t.subtree(id).filter(|&x| matches(x, &mut buf)).collect()
    };
    let total = hits.len();
    hits.sort_unstable_by(|&a, &b| t.nodes[b as usize].size.cmp(&t.nodes[a as usize].size));
    hits.truncate(limit);
    (hits, total)
}

pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    let idx = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let num: f64 = s[..idx].parse().ok()?;
    let mul: f64 = match s[idx..].trim() {
        "" | "b" => 1.0,
        "k" | "kb" | "кб" => 1024.0,
        "m" | "mb" | "мб" => 1024.0 * 1024.0,
        "g" | "gb" | "гб" => 1024.0f64.powi(3),
        "t" | "tb" | "тб" => 1024.0f64.powi(4),
        _ => return None,
    };
    Some((num * mul) as u64)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItem {
    pub id: u32,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanupGroup {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub risk: &'static str, // "safe" | "review"
    pub total: u64,
    pub count: usize,
    pub items: Vec<CleanupItem>,
}

const GROUPS: &[(&str, &str, &str, &str)] = &[
    ("temp", "Временные файлы", "Папки Temp системы и пользователей. Обычно их можно удалить без последствий.", "safe"),
    ("browser", "Кэш браузеров", "Кэш Chrome, Edge, Firefox, Яндекс, Opera, Brave. Браузер создаст его заново.", "safe"),
    ("shader", "Кэш шейдеров и драйверов", "DirectX/NVIDIA/AMD кэши шейдеров. Пересоздаются автоматически.", "safe"),
    ("dev", "Мусор разработчика", "node_modules, __pycache__, кэши npm/pip/yarn/gradle, папки target у Rust-проектов.", "review"),
    ("system", "Системный мусор", "Загруженные обновления Windows, Windows.old, дампы памяти и отчёты об ошибках.", "review"),
    ("installers", "Установщики и архивы в Загрузках", "Файлы .exe/.msi/.iso/.zip больше 10 МБ в папках Downloads.", "review"),
    ("logs", "Большие логи и дампы", "Файлы .log/.dmp/.etl больше 20 МБ.", "review"),
    ("old", "Большие и давно не изменявшиеся", "Файлы больше 500 МБ, которые не менялись больше года.", "review"),
];

fn classify(t: &Tree, x: u32, name: &str, path: &str, is_dir: bool, size: u64, now: i64, modified: i64) -> Option<&'static str> {
    let has = |s: &str| path.contains(s);
    if is_dir {
        match name {
            "temp" | "tmp" if has("appdata\\local\\temp") || has("windows\\temp") || path == "/tmp" || path.ends_with("/tmp") => {
                return Some("temp")
            }
            "cache" | "cache2" | "code cache" | "gpucache" | "cache_data" | "service worker"
                if ["google\\chrome", "microsoft\\edge", "mozilla", "yandex", "opera", "brave", "chromium", ".cache/google-chrome", ".cache/mozilla"]
                    .iter()
                    .any(|b| has(b)) =>
            {
                return Some("browser")
            }
            "d3dscache" | "dxcache" | "glcache" | "nv_cache" | "shadercache" | "dxccache" => return Some("shader"),
            "node_modules" | "__pycache__" | ".pytest_cache" | ".mypy_cache" | ".next" | ".parcel-cache" | ".turbo" => return Some("dev"),
            "npm-cache" if has("appdata") => return Some("dev"),
            "cache" if has("\\pip\\") || has("yarn") || has(".gradle") || has("go-build") => return Some("dev"),
            "target" => {
                // Rust build dir: sibling Cargo.toml
                let p = t.nodes[x as usize].parent;
                if p != crate::NONE && t.kids(p).any(|c| &*t.nodes[c as usize].name == "Cargo.toml") {
                    return Some("dev");
                }
            }
            "windows.old" | "crashdumps" | "minidump" | "livekernelreports" => return Some("system"),
            "download" if has("softwaredistribution") => return Some("system"),
            "wer" if has("microsoft\\windows\\wer") => return Some("system"),
            _ => {}
        }
        return None;
    }
    let ext = ext_of(name);
    if name == "memory.dmp" {
        return Some("system");
    }
    if size >= 10 << 20 && (has("\\downloads\\") || has("/downloads/")) && matches!(ext.as_str(), "exe" | "msi" | "iso" | "zip" | "rar" | "7z" | "img" | "dmg")
    {
        return Some("installers");
    }
    if size >= 20 << 20 && matches!(ext.as_str(), "log" | "dmp" | "etl" | "trace") {
        return Some("logs");
    }
    if size >= 500 << 20 && modified > 0 && now - modified > 365 * 86400 && !matches!(ext.as_str(), "sys") {
        return Some("old");
    }
    None
}

/// Groups whose folders are emptied instead of deleted: programs expect Temp
/// and cache folders to exist, only what is inside is junk.
pub const CLEAR_CONTENTS: &[&str] = &["temp", "browser", "shader"];

/// A path standing for "something inside this folder" for the safety rules.
pub fn content_probe(dir: &str) -> String {
    let sep = if dir.contains('\\') { '\\' } else { '/' };
    let mut p = dir.trim_end_matches(sep).to_string();
    p.push(sep);
    p.push_str("clearscope-item");
    p
}

/// Is this folder a Temp/browser/shader cache that should be emptied rather than removed?
pub fn is_clearable(t: &Tree, x: u32) -> bool {
    let n = &t.nodes[x as usize];
    if !n.is_dir() {
        return false;
    }
    let path = t.path(x).to_lowercase();
    let name = n.name.to_lowercase();
    matches!(classify(t, x, &name, &path, true, n.size, 0, n.modified), Some(k) if CLEAR_CONTENTS.contains(&k))
}

pub fn cleanup(t: &Tree, id: u32, per_group: usize) -> Vec<CleanupGroup> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let mut found: HashMap<&'static str, Vec<u32>> = HashMap::new();
    // DFS over folders with an incrementally built lowercase path (no per-node
    // joins/allocations). Small files can never match a rule, so they're
    // rejected before any string work: this is what made the view slow.
    let base = t.path(id).to_lowercase();
    let sep = if base.contains('\\') { '\\' } else { '/' };
    let mut path = String::with_capacity(512);
    let mut ends: Vec<usize> = Vec::new();
    let push_name = |path: &mut String, name: &str| {
        if !path.ends_with(sep) {
            path.push(sep);
        }
        let start = path.len();
        if name.is_ascii() {
            path.push_str(name);
            path[start..].make_ascii_lowercase();
        } else {
            path.extend(name.chars().flat_map(char::to_lowercase));
        }
        start
    };
    let hit = |k: &'static str, x: u32, found: &mut HashMap<&'static str, Vec<u32>>| {
        let n = &t.nodes[x as usize];
        // cache folders are cleared, not removed, so what matters is whether their contents may go
        let path = if n.is_dir() && CLEAR_CONTENTS.contains(&k) { content_probe(&t.path(x)) } else { t.path(x) };
        if n.size > 0 && crate::safety::classify(&path, n.is_dir()).level != crate::safety::Level::Blocked {
            found.entry(k).or_default().push(x);
        }
    };
    let mut stack: Vec<(u32, usize)> = vec![(id, 0)];
    while let Some((x, depth)) = stack.pop() {
        ends.truncate(depth);
        path.truncate(if depth == 0 { 0 } else { ends[depth - 1] });
        let n = &t.nodes[x as usize];
        if depth == 0 {
            path.push_str(&base);
        } else {
            let start = push_name(&mut path, &n.name);
            let lname = path[start..].to_string();
            if let Some(k) = classify(t, x, &lname, &path, true, n.size, now, n.modified) {
                hit(k, x, &mut found);
                continue;
            }
        }
        ends.push(path.len());
        for c in t.kids(x) {
            let cn = &t.nodes[c as usize];
            if cn.is_dir() {
                stack.push((c, depth + 1));
            } else if cn.size >= 10 << 20 || (cn.name.len() == 10 && cn.name.eq_ignore_ascii_case("memory.dmp")) {
                let keep = path.len();
                let start = push_name(&mut path, &cn.name);
                let lname = path[start..].to_string();
                if let Some(k) = classify(t, c, &lname, &path, false, cn.size, now, cn.modified) {
                    hit(k, c, &mut found);
                }
                path.truncate(keep);
            }
        }
    }
    let mut out = Vec::new();
    for (key, title, description, risk) in GROUPS {
        let Some(mut ids) = found.remove(key) else { continue };
        ids.sort_unstable_by(|&a, &b| t.nodes[b as usize].size.cmp(&t.nodes[a as usize].size));
        let total = ids.iter().map(|&i| t.nodes[i as usize].size).sum();
        let count = ids.len();
        let items = ids
            .into_iter()
            .take(per_group)
            .map(|i| CleanupItem { id: i, path: t.path(i), size: t.nodes[i as usize].size, is_dir: t.nodes[i as usize].is_dir() })
            .collect();
        out.push(CleanupGroup { key, title, description, risk, total, count, items });
    }
    out.sort_by(|a, b| b.total.cmp(&a.total));
    out
}

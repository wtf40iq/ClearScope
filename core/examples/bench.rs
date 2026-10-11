//! Usage: bench walk <dir>   |   bench mft <image-or-volume> [root]
use clearscope_core::*;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let prog = Progress::new();
    let t0 = Instant::now();
    let tree = match a[1].as_str() {
        "walk" => walk::scan_walk(std::path::Path::new(&a[2]), 4096, &prog).unwrap(),
        "mft" => {
            let fs: Vec<std::fs::File> = (0..4).map(|_| std::fs::File::open(&a[2]).unwrap()).collect();
            mft::scan_mft(&fs, a.get(3).map(|s| s.as_str()).unwrap_or("C:\\"), &prog).unwrap()
        }
        _ => panic!("mode"),
    };
    let r = &tree.nodes[tree.root as usize];
    println!("method={:?} time={:?} nodes={} size={} alloc={} files={} dirs={}", tree.method, t0.elapsed(), tree.nodes.len(), r.size, r.alloc, r.files, r.dirs);
    for c in tree.kids(tree.root).take(8) {
        let n = &tree.nodes[c as usize];
        println!("  {:>12} {:>6} {}", n.size, n.files, tree.path(c));
    }
    println!("ext: {:?}", analysis::ext_stats(&tree, tree.root, 6).iter().map(|e| (e.ext.clone(), e.size, e.count)).collect::<Vec<_>>());
    println!("top: {:?}", analysis::top_files(&tree, tree.root, 3).iter().map(|&x| tree.path(x)).collect::<Vec<_>>());
    let (hits, total) = analysis::search(&tree, tree.root, "*.mp4 >1mb", 5);
    println!("search mp4>1mb: {} {:?}", total, hits.iter().map(|&x| tree.path(x)).collect::<Vec<_>>());
    for g in analysis::cleanup(&tree, tree.root, 3) {
        println!("cleanup {} total={} count={} first={:?}", g.key, g.total, g.count, g.items.first().map(|i| i.path.clone()));
    }
    if let Some(prefix) = a.get(4) {
        // dupes need real file paths: map tree root to a mounted path
        let _ = prefix;
    }
    if a[1] == "walk" {
        let d = dupes::find_duplicates(&tree, tree.root, 1 << 20, 5, &Progress::new());
        println!("dupes: {:?}", d.iter().map(|g| (g.size, g.items.len())).collect::<Vec<_>>());
    }
    let s = snapshot::take(&tree);
    println!("snapshot entries={}", s.entries.len());
    let dir = std::env::temp_dir().join("ss-cache-test");
    let t1 = Instant::now();
    let meta = cache::save(&dir, &tree, 0, 0).unwrap();
    let save_ms = t1.elapsed();
    let t2 = Instant::now();
    let (t2tree, _) = cache::load(&dir, &tree.root_path).unwrap();
    println!("cache: nodes={} save={:?} load={:?} size_ok={} files_ok={} bytes={}", meta.nodes, save_ms, t2.elapsed(),
        t2tree.nodes[t2tree.root as usize].size == r.size, t2tree.nodes[t2tree.root as usize].files == r.files,
        std::fs::metadata(cache::file_for(&dir, &tree.root_path)).unwrap().len());
}

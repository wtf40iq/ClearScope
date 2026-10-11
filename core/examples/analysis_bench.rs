//! Synthetic large tree: timing + sanity of cleanup/search/ext_stats.
use clearscope_core::tree::{RawEntry, ScanMethod, Tree, NONE};
use clearscope_core::*;
use std::time::Instant;

fn main() {
    let mut raw = vec![RawEntry { name: "C:\\".into(), parent: NONE, size: 0, alloc: 0, is_dir: true, modified: 0 }];
    let add = |raw: &mut Vec<RawEntry>, name: &str, parent: u32, size: u64, dir: bool| {
        raw.push(RawEntry { name: name.into(), parent, size, alloc: size, is_dir: dir, modified: 1_600_000_000 });
        (raw.len() - 1) as u32
    };
    let users = add(&mut raw, "Users", 0, 0, true);
    let alex = add(&mut raw, "Alex", users, 0, true);
    let appdata = add(&mut raw, "AppData", alex, 0, true);
    let local = add(&mut raw, "Local", appdata, 0, true);
    let temp = add(&mut raw, "Temp", local, 0, true);
    add(&mut raw, "x.tmp", temp, 5 << 20, false);
    let dl = add(&mut raw, "Downloads", alex, 0, true);
    add(&mut raw, "big.iso", dl, 3 << 30, false);
    add(&mut raw, "MEMORY.DMP", 0, 1 << 30, false);
    let proj = add(&mut raw, "Projects", alex, 0, true);
    for p in 0..2000u32 {
        let d = add(&mut raw, &format!("proj{p}"), proj, 0, true);
        let nm = add(&mut raw, "node_modules", d, 0, true);
        for k in 0..30u32 {
            let sub = add(&mut raw, &format!("pkg{k}"), nm, 0, true);
            for f in 0..12u32 {
                add(&mut raw, &format!("file{f}.js"), sub, 1000 + f as u64, false);
            }
        }
        let src = add(&mut raw, "src", d, 0, true);
        for f in 0..60u32 {
            add(&mut raw, &format!("отчёт{f}.ts"), src, 2000, false);
        }
    }
    let n = raw.len();
    let t = Tree::build(raw, 0, "C:\\".into(), ScanMethod::Mft);
    println!("nodes={n}");
    let t0 = Instant::now();
    let g = analysis::cleanup(&t, t.root, 200);
    println!("cleanup {:?}", t0.elapsed());
    for x in &g {
        println!("  {} count={} total={} first={:?}", x.key, x.count, x.total, x.items.first().map(|i| &i.path));
    }
    let t0 = Instant::now();
    let e = analysis::ext_stats(&t, t.root, 5);
    println!("ext {:?} {:?}", t0.elapsed(), e.iter().map(|e| (&e.ext, e.count)).collect::<Vec<_>>());
    let t0 = Instant::now();
    let (h, tot) = analysis::search(&t, t.root, "ОТЧЁТ1", 10);
    println!("search {:?} total={tot} first={:?}", t0.elapsed(), h.first().map(|&x| t.path(x)));
    let t0 = Instant::now();
    let (_, tot) = analysis::search(&t, t.root, "*.ISO >1gb", 10);
    println!("search ext {:?} total={tot}", t0.elapsed());
}

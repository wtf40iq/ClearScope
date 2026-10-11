//! On-disk cache of a scanned tree for instant reopening.
//!
//! Layout: b"SSC2" | u32 header_len | header JSON | LZ4 frame of node records.
//! Nodes are written in pre-order with parent indices, so loading is a single
//! pass followed by `Tree::build`.
use crate::tree::{RawEntry, ScanMethod, Tree, NONE};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"SSC2";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CacheMeta {
    pub root_path: String,
    pub method: ScanMethod,
    pub scan_ms: u64,
    pub captured_at: i64,
    pub saved_at: i64,
    /// USN journal position the data is consistent with (NTFS only).
    pub journal_id: u64,
    pub next_usn: i64,
    pub size: u64,
    pub files: u64,
    pub nodes: u64,
}

pub fn file_for(dir: &Path, root: &str) -> PathBuf {
    let h = xxhash_rust::xxh3::xxh3_64(root.to_lowercase().as_bytes());
    dir.join(format!("cache-{h:016x}.ssc"))
}

pub fn save(dir: &Path, t: &Tree, journal_id: u64, next_usn: i64) -> io::Result<CacheMeta> {
    std::fs::create_dir_all(dir)?;
    let order: Vec<u32> = t.subtree(t.root).collect();
    let r = &t.nodes[t.root as usize];
    let meta = CacheMeta {
        root_path: t.root_path.clone(),
        method: t.method,
        scan_ms: t.scan_ms,
        captured_at: t.captured_at,
        saved_at: crate::tree::now_unix(),
        journal_id,
        next_usn,
        size: r.size,
        files: r.files as u64,
        nodes: order.len() as u64,
    };
    let final_path = file_for(dir, &t.root_path);
    let tmp = final_path.with_extension("tmp");
    {
        let mut f = BufWriter::with_capacity(1 << 20, File::create(&tmp)?);
        let hdr = serde_json::to_vec(&meta).map_err(io::Error::other)?;
        f.write_all(MAGIC)?;
        f.write_all(&(hdr.len() as u32).to_le_bytes())?;
        f.write_all(&hdr)?;
        let mut z = lz4_flex::frame::FrameEncoder::new(f);
        let mut remap = vec![NONE; t.nodes.len()];
        let mut buf: Vec<u8> = Vec::with_capacity(1 << 16);
        for (i, &x) in order.iter().enumerate() {
            remap[x as usize] = i as u32;
            let n = &t.nodes[x as usize];
            let p = if x == t.root { NONE } else { remap[n.parent as usize] };
            buf.extend_from_slice(&p.to_le_bytes());
            buf.push(n.is_dir() as u8);
            buf.extend_from_slice(&(if n.is_dir() { 0 } else { n.size }).to_le_bytes());
            buf.extend_from_slice(&(if n.is_dir() { 0 } else { n.alloc }).to_le_bytes());
            buf.extend_from_slice(&n.modified.to_le_bytes());
            let nb = n.name.as_bytes();
            let l = nb.len().min(u16::MAX as usize);
            buf.extend_from_slice(&(l as u16).to_le_bytes());
            buf.extend_from_slice(&nb[..l]);
            if buf.len() > 1 << 16 {
                z.write_all(&buf)?;
                buf.clear();
            }
        }
        z.write_all(&buf)?;
        let mut f = z.finish().map_err(io::Error::other)?;
        f.flush()?;
    }
    std::fs::rename(&tmp, &final_path)?;
    Ok(meta)
}

fn read_meta(f: &mut impl Read) -> io::Result<CacheMeta> {
    let mut m = [0u8; 4];
    f.read_exact(&mut m)?;
    if &m != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad cache"));
    }
    let mut l = [0u8; 4];
    f.read_exact(&mut l)?;
    let len = u32::from_le_bytes(l) as usize;
    if len > 1 << 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad cache header"));
    }
    let mut h = vec![0u8; len];
    f.read_exact(&mut h)?;
    serde_json::from_slice(&h).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn peek(path: &Path) -> io::Result<CacheMeta> {
    read_meta(&mut BufReader::new(File::open(path)?))
}

pub fn list(dir: &Path) -> Vec<CacheMeta> {
    let mut v: Vec<CacheMeta> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "ssc"))
        .filter_map(|e| peek(&e.path()).ok())
        .collect();
    v.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    v
}

pub fn load(dir: &Path, root: &str) -> io::Result<(Tree, CacheMeta)> {
    let mut f = BufReader::with_capacity(1 << 20, File::open(file_for(dir, root))?);
    let meta = read_meta(&mut f)?;
    let mut z = BufReader::with_capacity(1 << 20, lz4_flex::frame::FrameDecoder::new(f));
    let mut raw: Vec<RawEntry> = Vec::with_capacity(meta.nodes as usize);
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "corrupt cache");
    let mut fixed = [0u8; 4 + 1 + 8 + 8 + 8 + 2];
    for _ in 0..meta.nodes {
        z.read_exact(&mut fixed)?;
        let parent = u32::from_le_bytes(fixed[0..4].try_into().unwrap());
        let is_dir = fixed[4] != 0;
        let size = u64::from_le_bytes(fixed[5..13].try_into().unwrap());
        let alloc = u64::from_le_bytes(fixed[13..21].try_into().unwrap());
        let modified = i64::from_le_bytes(fixed[21..29].try_into().unwrap());
        let l = u16::from_le_bytes(fixed[29..31].try_into().unwrap()) as usize;
        let mut nb = vec![0u8; l];
        z.read_exact(&mut nb)?;
        if parent != NONE && parent as usize >= raw.len() {
            return Err(bad());
        }
        raw.push(RawEntry { name: String::from_utf8(nb).map_err(|_| bad())?.into(), parent, size, alloc, is_dir, modified });
    }
    if raw.is_empty() {
        return Err(bad());
    }
    let mut t = Tree::build(raw, 0, meta.root_path.clone(), meta.method);
    t.scan_ms = meta.scan_ms;
    t.captured_at = meta.captured_at;
    Ok((t, meta))
}

pub fn remove(dir: &Path, root: &str) {
    let _ = std::fs::remove_file(file_for(dir, root));
}

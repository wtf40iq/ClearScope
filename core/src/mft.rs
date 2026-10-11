//! Direct NTFS Master File Table reader.
//!
//! Reads the raw `$MFT` of a volume sequentially in large chunks and parses
//! FILE records in parallel. This is the same technique WizTree uses and it
//! is orders of magnitude faster than walking directories, because every
//! file's name, parent and size live in the MFT.
use crate::progress::{Phase, Progress};
use crate::tree::{RawEntry, ScanMethod, Tree, NONE};
use rayon::prelude::*;
use std::io;
use std::sync::atomic::Ordering::Relaxed;

/// Positional reader over a raw device / image. Offsets and lengths passed by
/// this module are always multiples of the sector size.
pub trait SectorRead: Sync {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<()>;
}

impl SectorRead for std::fs::File {
    #[cfg(unix)]
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<()> {
        use std::os::unix::fs::FileExt;
        self.read_exact_at(buf, off)
    }
    #[cfg(windows)]
    fn read_at(&self, mut off: u64, mut buf: &mut [u8]) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            let n = self.seek_read(buf, off)?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short read"));
            }
            off += n as u64;
            buf = &mut buf[n..];
        }
        Ok(())
    }
}

/// Open a volume like `C:` for raw reading (requires administrator rights).
#[cfg(windows)]
pub fn open_volume(letter: char) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2) // FILE_SHARE_READ | FILE_SHARE_WRITE
        .open(format!(r"\\.\{}:", letter))
}

#[inline]
fn u16le(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
#[inline]
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}
#[inline]
fn u64le(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()))
}

fn filetime_to_unix(ft: u64) -> i64 {
    if ft == 0 {
        return 0;
    }
    (ft / 10_000_000) as i64 - 11_644_473_600
}

/// Apply the NTFS update-sequence-array fixup in place. Returns false if the
/// record is not a valid FILE record.
pub fn apply_fixup(rec: &mut [u8]) -> bool {
    if rec.len() < 48 || &rec[0..4] != b"FILE" {
        return false;
    }
    let usa_off = u16le(rec, 4).unwrap() as usize;
    let usa_cnt = u16le(rec, 6).unwrap() as usize;
    if usa_cnt < 2 || usa_off + usa_cnt * 2 > rec.len() {
        return false;
    }
    let stride = rec.len() / (usa_cnt - 1);
    let usn = [rec[usa_off], rec[usa_off + 1]];
    for i in 1..usa_cnt {
        let pos = i * stride - 2;
        if pos + 2 > rec.len() {
            break;
        }
        if rec[pos] != usn[0] || rec[pos + 1] != usn[1] {
            return false; // torn write
        }
        rec[pos] = rec[usa_off + 2 * i];
        rec[pos + 1] = rec[usa_off + 2 * i + 1];
    }
    true
}

/// Decode an NTFS data-run list into (lcn, cluster_count). `None` lcn = sparse.
pub fn decode_runs(d: &[u8]) -> Vec<(Option<u64>, u64)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut lcn: i64 = 0;
    while i < d.len() {
        let h = d[i];
        if h == 0 {
            break;
        }
        let ls = (h & 0x0F) as usize;
        let os = (h >> 4) as usize;
        i += 1;
        if ls == 0 || ls > 8 || os > 8 || i + ls + os > d.len() {
            break;
        }
        let mut len = 0u64;
        for k in 0..ls {
            len |= (d[i + k] as u64) << (8 * k);
        }
        i += ls;
        if os == 0 {
            out.push((None, len));
        } else {
            let mut off = 0i64;
            for k in 0..os {
                off |= (d[i + k] as i64) << (8 * k);
            }
            let shift = 64 - 8 * os as u32;
            off = (off << shift) >> shift; // sign-extend
            lcn += off;
            out.push((Some(lcn as u64), len));
        }
        i += os;
    }
    out
}

/// What we extract from a single FILE record.
#[derive(Default, Debug)]
struct Parsed {
    rec: u64,
    base: u64,
    is_dir: bool,
    name: Option<(Box<str>, u8, u64)>, // name, namespace rank, parent record
    data: Option<(u64, u64)>,          // logical size, allocated size
    mtime: i64,
}

fn ns_rank(ns: u8) -> u8 {
    match ns {
        1 | 3 => 3, // Win32 / Win32+DOS
        0 => 2,     // POSIX
        _ => 1,     // DOS 8.3 only
    }
}

/// UTF-16LE -> Box<str> with a single allocation (ASCII fast path: most names).
#[inline]
fn utf16_name(raw: &[u8]) -> Box<str> {
    let n = raw.len() / 2;
    if raw.chunks_exact(2).all(|c| c[1] == 0 && c[0] < 0x80) {
        let mut v = Vec::with_capacity(n);
        v.extend(raw.chunks_exact(2).map(|c| c[0]));
        // SAFETY: all bytes are < 0x80, i.e. valid single-byte UTF-8
        return unsafe { String::from_utf8_unchecked(v) }.into_boxed_str();
    }
    let mut s = String::with_capacity(n + n / 2);
    for ch in char::decode_utf16(raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]))) {
        s.push(ch.unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    s.into_boxed_str()
}

fn parse_record(rec: &mut [u8], rec_no: u64) -> Option<Parsed> {
    // cheap rejection of unused records before touching the fixup array
    if rec.len() < 48 || &rec[0..4] != b"FILE" || rec[0x16] & 0x01 == 0 {
        return None;
    }
    if !apply_fixup(rec) {
        return None;
    }
    let flags = u16le(rec, 0x16)?;
    let mut p = Parsed { rec: rec_no, base: u64le(rec, 0x20)? & 0x0000_FFFF_FFFF_FFFF, is_dir: flags & 0x02 != 0, ..Default::default() };
    let used = (u32le(rec, 0x18)? as usize).min(rec.len());
    let mut off = u16le(rec, 0x14)? as usize;
    while off + 16 <= used {
        let ty = u32le(rec, off)?;
        if ty == 0xFFFF_FFFF {
            break;
        }
        let len = u32le(rec, off + 4)? as usize;
        if len < 16 || off + len > used {
            break;
        }
        let a = &rec[off..off + len];
        let nonres = a[8] != 0;
        let name_len = a[9];
        match ty {
            0x10 if !nonres => {
                // $STANDARD_INFORMATION
                let co = u16le(a, 0x14)? as usize;
                if let Some(m) = u64le(a, co + 8) {
                    p.mtime = filetime_to_unix(m);
                }
            }
            0x30 if !nonres => {
                // $FILE_NAME
                let co = u16le(a, 0x14)? as usize;
                let parent = u64le(a, co)? & 0x0000_FFFF_FFFF_FFFF;
                let nlen = *a.get(co + 0x40)? as usize;
                let ns = *a.get(co + 0x41)?;
                let rank = ns_rank(ns);
                if p.name.as_ref().map_or(true, |(_, r, _)| rank > *r) {
                    let raw = a.get(co + 0x42..co + 0x42 + nlen * 2)?;
                    p.name = Some((utf16_name(raw), rank, parent));
                }
                if p.mtime == 0 {
                    p.mtime = filetime_to_unix(u64le(a, co + 0x10).unwrap_or(0));
                }
            }
            0x80 if name_len == 0 => {
                // unnamed $DATA stream
                if nonres {
                    let start_vcn = u64le(a, 0x10)?;
                    if start_vcn == 0 {
                        let alloc = u64le(a, 0x28)?;
                        let size = u64le(a, 0x30)?;
                        let aflags = u16le(a, 0x0C)?;
                        // compressed / sparse: real on-disk usage is at 0x40
                        let alloc = if aflags & 0x8001 != 0 { u64le(a, 0x40).unwrap_or(alloc) } else { alloc };
                        p.data = Some((size, alloc));
                    }
                } else {
                    let clen = u32le(a, 0x10)? as u64;
                    p.data = Some((clen, 0));
                }
            }
            _ => {}
        }
        off += len;
    }
    Some(p)
}

#[derive(Default, Clone)]
struct Entry {
    in_use: bool,
    is_dir: bool,
    rank: u8,
    name: Option<Box<str>>,
    parent: u64,
    size: u64,
    alloc: u64,
    mtime: i64,
}

pub struct VolumeGeometry {
    pub cluster: u64,
    pub record_size: usize,
    pub mft_runs: Vec<(Option<u64>, u64)>,
    pub mft_size: u64,
}

pub fn read_geometry<R: SectorRead>(dev: &R) -> io::Result<VolumeGeometry> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
    let mut boot = vec![0u8; 4096];
    dev.read_at(0, &mut boot)?;
    if &boot[3..11] != b"NTFS    " {
        return Err(bad("not an NTFS volume"));
    }
    let bps = u16le(&boot, 0x0B).unwrap() as u64;
    let spc_raw = boot[0x0D];
    let spc: u64 = if spc_raw > 0x80 { 1u64 << (256 - spc_raw as u32) } else { spc_raw as u64 };
    let cluster = bps * spc;
    let mft_lcn = u64le(&boot, 0x30).unwrap();
    let cpr = boot[0x40] as i8;
    let record_size: usize = if cpr > 0 { cpr as usize * cluster as usize } else { 1usize << (-(cpr as i32)) };
    if cluster == 0 || record_size < 512 || record_size > 65536 {
        return Err(bad("bad NTFS geometry"));
    }
    let mut rec0 = vec![0u8; record_size.max(bps as usize)];
    dev.read_at(mft_lcn * cluster, &mut rec0)?;
    rec0.truncate(record_size);
    if !apply_fixup(&mut rec0) {
        return Err(bad("bad $MFT record"));
    }
    let mut off = u16le(&rec0, 0x14).unwrap() as usize;
    let used = (u32le(&rec0, 0x18).unwrap() as usize).min(record_size);
    while off + 16 <= used {
        let ty = u32le(&rec0, off).unwrap();
        let len = u32le(&rec0, off + 4).unwrap() as usize;
        if ty == 0xFFFF_FFFF || len < 16 || off + len > used {
            break;
        }
        let a = &rec0[off..off + len];
        if ty == 0x80 && a[8] != 0 && a[9] == 0 {
            let ro = u16le(a, 0x20).unwrap() as usize;
            let size = u64le(a, 0x30).unwrap();
            return Ok(VolumeGeometry { cluster, record_size, mft_runs: decode_runs(&a[ro.min(len)..]), mft_size: size });
        }
        off += len;
    }
    Err(bad("$MFT $DATA attribute not found"))
}

const CHUNK: u64 = 8 * 1024 * 1024;

/// Scan an NTFS volume through its MFT. `readers` are independent handles to
/// the same volume (one per I/O thread, so reads run in parallel at a higher
/// queue depth, which matters a lot on NVMe). `root_path` is e.g. `C:\`.
pub fn scan_mft<R: SectorRead>(readers: &[R], root_path: &str, prog: &Progress) -> io::Result<Tree> {
    let t0 = std::time::Instant::now();
    let captured_at = crate::tree::now_unix();
    let dev0 = readers.first().ok_or_else(|| io::Error::other("no reader"))?;
    let g = read_geometry(dev0)?;
    let rs = g.record_size as u64;
    let total = (g.mft_size / rs) as usize;
    prog.mft.store(true, Relaxed);
    prog.set_phase(Phase::Reading);
    prog.total_hint.store(total as u64, Relaxed);
    let mut table: Vec<Entry> = vec![Entry::default(); total];

    // Split the MFT extents into chunk jobs.
    let unit = g.cluster.max(rs);
    let chunk = (CHUNK / unit).max(1) * unit;
    let mut jobs: Vec<(u64, u64, u64)> = Vec::new(); // (byte offset, len, first record)
    let mut rec_no = 0u64;
    for &(lcn, clusters) in &g.mft_runs {
        let bytes = clusters * g.cluster;
        if let Some(l) = lcn {
            let mut done = 0;
            while done < bytes {
                let n = chunk.min(bytes - done);
                if rec_no + done / rs < total as u64 {
                    jobs.push((l * g.cluster + done, n, rec_no + done / rs));
                }
                done += n;
            }
        }
        rec_no += bytes / rs;
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::sync_channel::<io::Result<(u64, Vec<u8>)>>(readers.len() * 2 + 2);
    std::thread::scope(|s| -> io::Result<()> {
        for dev in readers {
            let tx = tx.clone();
            let (jobs, next) = (&jobs, &next);
            s.spawn(move || loop {
                if prog.cancelled() {
                    return;
                }
                let j = next.fetch_add(1, Relaxed);
                let Some(&(off, len, first)) = jobs.get(j) else { return };
                let mut buf = vec![0u8; len as usize];
                let r = dev.read_at(off, &mut buf).map(|_| (first, buf));
                let failed = r.is_err();
                if tx.send(r).is_err() || failed {
                    return;
                }
            });
        }
        drop(tx);
        for msg in rx {
            let (first, mut buf) = msg?;
            if prog.cancelled() {
                break;
            }
            let parsed: Vec<Parsed> = buf
                .par_chunks_exact_mut(g.record_size)
                .enumerate()
                .filter_map(|(i, r)| parse_record(r, first + i as u64))
                .collect();
            let n_recs = (buf.len() / g.record_size) as u64;
            let (mut nf, mut nd, mut nb) = (0u64, 0u64, 0u64);
            for p in parsed {
                let target = if p.base != 0 { p.base } else { p.rec } as usize;
                if target >= total {
                    continue;
                }
                let e = &mut table[target];
                if p.base == 0 {
                    e.in_use = true;
                    e.is_dir = p.is_dir;
                    if p.is_dir { nd += 1 } else { nf += 1 }
                    if p.mtime != 0 {
                        e.mtime = p.mtime;
                    }
                }
                if let Some((name, rank, parent)) = p.name {
                    if e.name.is_none() || rank > e.rank {
                        e.name = Some(name);
                        e.rank = rank;
                        e.parent = parent;
                    }
                }
                if let Some((size, alloc)) = p.data {
                    e.size = size;
                    e.alloc = alloc;
                    nb += size;
                }
            }
            prog.files.fetch_add(nf, Relaxed);
            prog.dirs.fetch_add(nd, Relaxed);
            prog.bytes.fetch_add(nb, Relaxed);
            prog.done_hint.fetch_add(n_recs, Relaxed);
        }
        Ok(())
    })?;
    if prog.cancelled() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    prog.set_phase(Phase::Building);

    // Build raw entries: record number -> node index.
    const ROOT_REC: usize = 5;
    let mut map = vec![NONE; total];
    let mut raw: Vec<RawEntry> = Vec::with_capacity(total / 2);
    for (i, e) in table.iter().enumerate() {
        if e.in_use && (e.name.is_some() || i == ROOT_REC) {
            map[i] = raw.len() as u32;
            raw.push(RawEntry { name: "".into(), parent: NONE, size: e.size, alloc: e.alloc, is_dir: e.is_dir, modified: e.mtime });
        }
    }
    if ROOT_REC >= total || map[ROOT_REC] == NONE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "root directory record missing"));
    }
    for (i, e) in table.into_iter().enumerate() {
        let idx = map[i];
        if idx == NONE {
            continue;
        }
        let r = &mut raw[idx as usize];
        r.name = e.name.unwrap_or_else(|| "".into());
        let pr = e.parent as usize;
        r.parent = if pr < total { map[pr] } else { NONE };
    }
    let root = map[ROOT_REC];
    raw[root as usize].name = root_path.into();
    let mut t = Tree::build(raw, root, root_path.to_string(), ScanMethod::Mft);
    t.scan_ms = t0.elapsed().as_millis() as u64;
    t.captured_at = captured_at;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runs_decode() {
        // 0x21: len 1 byte, off 2 bytes: len=0x18, lcn=0x5634 ; then 0x11 len 0x10 off -0x10
        let d = [0x21, 0x18, 0x34, 0x56, 0x11, 0x10, 0xF0, 0x00];
        let r = decode_runs(&d);
        assert_eq!(r, vec![(Some(0x5634), 0x18), (Some(0x5634 - 0x10), 0x10)]);
    }
}

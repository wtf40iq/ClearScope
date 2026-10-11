//! NTFS USN change journal: tells us every file created / modified / renamed
//! / deleted on a volume, even while the app was closed.
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, OpenFileById, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0};
use windows_sys::Win32::System::IO::DeviceIoControl;

const FSCTL_QUERY_USN_JOURNAL: u32 = 0x000900f4;
const FSCTL_READ_USN_JOURNAL: u32 = 0x000900bb;
pub const ERROR_JOURNAL_NOT_ACTIVE: i32 = 1179;
pub const ERROR_JOURNAL_ENTRY_DELETED: i32 = 1181;

pub const REASON_RENAME_OLD_NAME: u32 = 0x0000_1000;
pub const REASON_FILE_DELETE: u32 = 0x0000_0200;

#[repr(C)]
#[derive(Default)]
struct UsnJournalDataV0 {
    usn_journal_id: u64,
    first_usn: i64,
    next_usn: i64,
    lowest_valid_usn: i64,
    max_usn: i64,
    maximum_size: u64,
    allocation_delta: u64,
}

#[repr(C)]
struct ReadUsnJournalDataV1 {
    start_usn: i64,
    reason_mask: u32,
    return_only_on_close: u32,
    timeout: u64,
    bytes_to_wait_for: u64,
    usn_journal_id: u64,
    min_major_version: u16,
    max_major_version: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Journal {
    pub id: u64,
    pub next: i64,
    pub lowest: i64,
}

fn h(f: &File) -> HANDLE {
    f.as_raw_handle() as HANDLE
}

pub fn query(vol: &File) -> io::Result<Journal> {
    let mut d = UsnJournalDataV0::default();
    let mut ret = 0u32;
    let ok = unsafe {
        DeviceIoControl(h(vol), FSCTL_QUERY_USN_JOURNAL, std::ptr::null(), 0, &mut d as *mut _ as *mut _, std::mem::size_of::<UsnJournalDataV0>() as u32, &mut ret, std::ptr::null_mut())
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Journal { id: d.usn_journal_id, next: d.next_usn, lowest: d.lowest_valid_usn })
}

#[derive(Debug, Clone)]
pub struct Record {
    pub frn: u64,
    pub parent: u64,
    pub reason: u32,
    pub name: String,
}

/// Read all records from `start` up to the current end of the journal.
/// Returns the records and the next USN to continue from.
pub fn read(vol: &File, journal_id: u64, start: i64, max_records: usize) -> io::Result<(Vec<Record>, i64)> {
    let mut out = Vec::new();
    let mut next = start;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let req = ReadUsnJournalDataV1 {
            start_usn: next,
            reason_mask: 0xFFFF_FFFF,
            return_only_on_close: 0,
            timeout: 0,
            bytes_to_wait_for: 0,
            usn_journal_id: journal_id,
            min_major_version: 2,
            max_major_version: 2,
        };
        let mut ret = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                h(vol),
                FSCTL_READ_USN_JOURNAL,
                &req as *const _ as *const _,
                std::mem::size_of::<ReadUsnJournalDataV1>() as u32,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut ret,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let ret = ret as usize;
        if ret < 8 {
            break;
        }
        let new_next = i64::from_le_bytes(buf[0..8].try_into().unwrap());
        let mut off = 8usize;
        while off + 60 <= ret {
            let len = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
            if len < 60 || off + len > ret {
                break;
            }
            let r = &buf[off..off + len];
            let major = u16::from_le_bytes([r[4], r[5]]);
            if major == 2 {
                let frn = u64::from_le_bytes(r[8..16].try_into().unwrap()) & 0x0000_FFFF_FFFF_FFFF;
                let parent = u64::from_le_bytes(r[16..24].try_into().unwrap());
                let reason = u32::from_le_bytes(r[40..44].try_into().unwrap());
                let nlen = u16::from_le_bytes([r[56], r[57]]) as usize;
                let noff = u16::from_le_bytes([r[58], r[59]]) as usize;
                if noff + nlen <= len {
                    let u: Vec<u16> = r[noff..noff + nlen].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    out.push(Record { frn, parent, reason, name: String::from_utf16_lossy(&u) });
                }
            }
            off += len;
        }
        if new_next == next || ret == 8 {
            next = new_next;
            break;
        }
        next = new_next;
        if out.len() >= max_records {
            break;
        }
    }
    Ok((out, next))
}

/// Resolve a file reference number (full 64-bit, with sequence) to a path.
pub fn path_of(vol: &File, frn: u64) -> Option<String> {
    let desc = FILE_ID_DESCRIPTOR { dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32, Type: 0, Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: frn as i64 } };
    let fh = unsafe { OpenFileById(h(vol), &desc, 0x80 /* FILE_READ_ATTRIBUTES */, 7, std::ptr::null(), 0x0200_0000 /* BACKUP_SEMANTICS */) };
    if fh == INVALID_HANDLE_VALUE || fh.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut n = unsafe { GetFinalPathNameByHandleW(fh, buf.as_mut_ptr(), buf.len() as u32, 0) } as usize;
    if n > buf.len() {
        buf = vec![0u16; n + 1];
        n = unsafe { GetFinalPathNameByHandleW(fh, buf.as_mut_ptr(), buf.len() as u32, 0) } as usize;
    }
    unsafe { CloseHandle(fh) };
    if n == 0 || n > buf.len() {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..n]);
    Some(s.strip_prefix(r"\\?\").map(|x| x.to_string()).unwrap_or(s))
}

/// Turn journal records into the set of paths that need reconciling.
pub fn records_to_paths(vol: &File, recs: &[Record]) -> Vec<String> {
    let mut dirs: HashMap<u64, Option<String>> = HashMap::new();
    let mut out = Vec::with_capacity(recs.len());
    for r in recs {
        let parent = dirs.entry(r.parent).or_insert_with(|| path_of(vol, r.parent));
        if let Some(p) = parent {
            let mut s = p.clone();
            if !s.ends_with('\\') {
                s.push('\\');
            }
            s.push_str(&r.name);
            out.push(s);
        }
    }
    out
}

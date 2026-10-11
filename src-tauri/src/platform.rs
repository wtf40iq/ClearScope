//! OS-specific helpers: drive list, elevation, opening files.
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub path: String,
    pub label: String,
    pub fs: String,
    pub kind: String,
    pub total: u64,
    pub free: u64,
    pub cluster: u64,
}

#[cfg(windows)]
mod imp {
    use super::DriveInfo;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    fn from_wide(b: &[u16]) -> String {
        let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf16_lossy(&b[..n])
    }

    pub fn drives() -> Vec<DriveInfo> {
        let mut out = Vec::new();
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{letter}:\\");
            let w = wide(&root);
            let ty = unsafe { GetDriveTypeW(w.as_ptr()) };
            let kind = match ty {
                2 => "removable",
                3 => "fixed",
                4 => "network",
                5 => "cdrom",
                6 => "ram",
                _ => continue,
            };
            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            if unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
                continue; // no media
            }
            let mut label = [0u16; 261];
            let mut fsname = [0u16; 261];
            let (mut serial, mut maxc, mut flags) = (0u32, 0u32, 0u32);
            unsafe {
                GetVolumeInformationW(w.as_ptr(), label.as_mut_ptr(), 261, &mut serial, &mut maxc, &mut flags, fsname.as_mut_ptr(), 261);
            }
            let (mut spc, mut bps, mut fc, mut tc) = (0u32, 0u32, 0u32, 0u32);
            let cluster = if unsafe { GetDiskFreeSpaceW(w.as_ptr(), &mut spc, &mut bps, &mut fc, &mut tc) } != 0 {
                spc as u64 * bps as u64
            } else {
                4096
            };
            out.push(DriveInfo { path: root, label: from_wide(&label), fs: from_wide(&fsname), kind: kind.into(), total, free, cluster });
        }
        out
    }

    pub fn is_elevated() -> bool {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elev as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            );
            CloseHandle(token);
            ok != 0 && elev.TokenIsElevated != 0
        }
    }

    pub fn relaunch_as_admin(target: Option<&str>) -> bool {
        let Ok(exe) = std::env::current_exe() else { return false };
        let exe = wide(&exe.to_string_lossy());
        let verb = wide("runas");
        // avoid a trailing backslash right before the closing quote (C:\" -> C:")
        let params = target.map(|t| format!("--scan \"{}\"", t.trim_end_matches('\\')));
        let pw = params.as_deref().map(wide);
        let pp = pw.as_ref().map(|w| w.as_ptr()).unwrap_or(std::ptr::null());
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), exe.as_ptr(), pp, std::ptr::null(), 1) };
        r as isize > 32
    }

    pub fn reveal(path: &str) {
        let _ = std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{path}\"")).spawn();
    }
    pub fn open(path: &str) {
        let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
    }
    use std::os::windows::process::CommandExt;
}

#[cfg(not(windows))]
mod imp {
    use super::DriveInfo;
    fn statvfs(p: &str) -> Option<(u64, u64, u64)> {
        let c = std::ffi::CString::new(p).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return None;
        }
        let f = s.f_frsize as u64;
        Some((s.f_blocks as u64 * f, s.f_bavail as u64 * f, s.f_bsize as u64))
    }
    pub fn drives() -> Vec<DriveInfo> {
        let mut v = Vec::new();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        for (p, label) in [("/".to_string(), "System".to_string()), (home, "Home".to_string())] {
            if let Some((total, free, bs)) = statvfs(&p) {
                v.push(DriveInfo { path: p, label, fs: String::new(), kind: "fixed".into(), total, free, cluster: bs });
            }
        }
        v
    }
    pub fn is_elevated() -> bool {
        unsafe { libc::geteuid() == 0 }
    }
    pub fn relaunch_as_admin(_target: Option<&str>) -> bool {
        false
    }
    pub fn reveal(path: &str) {
        let parent = std::path::Path::new(path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let _ = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" }).arg(parent).spawn();
    }
    pub fn open(path: &str) {
        let _ = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" }).arg(path).spawn();
    }
}

pub use imp::*;

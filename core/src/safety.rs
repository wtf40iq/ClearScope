//! Deletion safety rules. The backend enforces these; the UI only displays them.
use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Warn,
    Blocked,
}

#[derive(Serialize, Clone, Debug)]
pub struct Verdict {
    pub level: Level,
    pub reason: String,
}

fn v(level: Level, reason: &str) -> Verdict {
    Verdict { level, reason: reason.to_string() }
}

/// Normalize to lowercase, backslashes, no trailing separator; returns the
/// part after the drive ("c:") as a list of components.
fn components(path: &str) -> (String, Vec<String>) {
    let p = path.replace('/', "\\").to_lowercase();
    let p = p.strip_prefix(r"\\?\").unwrap_or(&p).to_string();
    let mut it = p.split('\\').filter(|s| !s.is_empty()).map(|s| s.to_string());
    let first = it.next().unwrap_or_default();
    (first, it.collect())
}

const ROOT_BLOCK_DIRS: &[(&str, &str)] = &[
    ("windows", "Системная папка Windows"),
    ("program files", "Папка установленных программ. Удаляйте программы через «Параметры → Приложения»"),
    ("program files (x86)", "Папка установленных программ. Удаляйте программы через «Параметры → Приложения»"),
    ("programdata", "Общие данные программ и системы"),
    ("users", "Папка всех профилей пользователей"),
    ("system volume information", "Точки восстановления и служебные данные Windows"),
    ("recovery", "Среда восстановления Windows"),
    ("boot", "Загрузчик Windows"),
    ("efi", "Загрузчик системы"),
    ("$recycle.bin", "Корзина. Очищайте её через Windows (ПКМ по корзине → Очистить)"),
    ("config.msi", "Служебная папка установщика Windows"),
    ("documents and settings", "Системная ссылка Windows"),
    ("perflogs", "Служебная папка Windows"),
];

const ROOT_BLOCK_FILES: &[(&str, &str)] = &[
    ("pagefile.sys", "Файл подкачки. Размер настраивается в «Система → О системе → Дополнительные параметры → Быстродействие»"),
    ("hiberfil.sys", "Файл гибернации. Отключить: команда «powercfg /h off» от администратора"),
    ("swapfile.sys", "Системный файл подкачки приложений"),
    ("bootmgr", "Загрузчик Windows"),
    ("bootnxt", "Загрузчик Windows"),
];

const PROFILE_DIRS: &[&str] = &["desktop", "documents", "downloads", "pictures", "videos", "music", "onedrive", "appdata", "favorites", "contacts", "saved games", "links", "searches", "3d objects"];

/// Classify a path for deletion.
pub fn classify(path: &str, is_dir: bool) -> Verdict {
    let (drive, c) = components(path);
    let unix = drive.is_empty() || !drive.ends_with(':');
    if c.is_empty() {
        return v(Level::Blocked, "Корень диска удалить нельзя");
    }
    if unix {
        // minimal rules for non-Windows
        let all: Vec<&str> = std::iter::once(drive.as_str()).chain(c.iter().map(|s| s.as_str())).collect();
        if matches!(all.first().copied(), Some("bin" | "boot" | "dev" | "etc" | "lib" | "lib64" | "proc" | "sbin" | "sys" | "usr" | "var")) {
            return v(Level::Blocked, "Системный каталог");
        }
        return v(Level::Ok, "");
    }
    let c0 = c[0].as_str();
    let n = c.len();
    let name = c[n - 1].as_str();

    if n == 1 {
        if c0.starts_with('$') && c0 != "$recycle.bin" && c0 != "$windows.~bt" && c0 != "$windows.~ws" {
            return v(Level::Blocked, "Служебный файл файловой системы NTFS");
        }
        if let Some((_, r)) = ROOT_BLOCK_DIRS.iter().find(|(d, _)| *d == c0) {
            return v(Level::Blocked, r);
        }
        if let Some((_, r)) = ROOT_BLOCK_FILES.iter().find(|(d, _)| *d == c0) {
            return v(Level::Blocked, r);
        }
        if c0.starts_with("dumpstack.log") {
            return v(Level::Blocked, "Служебный журнал Windows");
        }
        if c0 == "windows.old" || c0 == "$windows.~bt" || c0 == "$windows.~ws" {
            return v(Level::Warn, "Остатки прошлой версии Windows. Надёжнее удалить через «Очистку диска» (cleanmgr)");
        }
    }

    match c0 {
        "windows" => {
            // allow only the contents of known-junk folders
            let allowed = (n >= 3 && c[1] == "temp")
                || (n >= 4 && c[1] == "softwaredistribution" && c[2] == "download")
                || (n >= 3 && c[1] == "prefetch")
                || (n >= 3 && c[1] == "logs" && !is_dir);
            if allowed {
                return v(Level::Ok, "");
            }
            return v(Level::Blocked, "Системные файлы Windows. Удаление может сломать систему");
        }
        "$recycle.bin" | "system volume information" | "recovery" | "boot" | "efi" | "config.msi" => {
            return v(Level::Blocked, "Служебные данные Windows");
        }
        "program files" | "program files (x86)" => {
            if n >= 2 && c[1] == "windowsapps" {
                return v(Level::Blocked, "Приложения Microsoft Store управляются системой");
            }
            if n >= 2 && matches!(c[1].as_str(), "windows defender" | "windows nt" | "common files" | "microsoft" | "windowspowershell" | "internet explorer" | "windows mail" | "windows media player" | "windows photo viewer" | "windows security" | "windows sidebar" | "modifiablewindowsapps") {
                return v(Level::Blocked, "Системный компонент Windows");
            }
            return v(Level::Warn, "Файлы установленной программы. Правильнее удалить программу через «Параметры → Приложения»");
        }
        "programdata" => {
            if n >= 2 && c[1] == "microsoft" {
                return v(Level::Blocked, "Системные данные Windows и Microsoft");
            }
            if n == 2 {
                return v(Level::Warn, "Общие данные программы: настройки, лицензии, базы");
            }
            return v(Level::Warn, "Общие данные программы");
        }
        "users" => {
            if n == 2 {
                return v(Level::Blocked, "Папка профиля пользователя");
            }
            if matches!(c[1].as_str(), "default" | "default user" | "all users") {
                return v(Level::Blocked, "Системный профиль-шаблон");
            }
            let sub = c[2].as_str();
            if n == 3 && PROFILE_DIRS.contains(&sub) {
                return v(Level::Blocked, "Стандартная папка профиля. Удаляйте её содержимое, а не саму папку");
            }
            if n == 3 && sub.starts_with("ntuser") {
                return v(Level::Blocked, "Реестр пользователя");
            }
            if sub == "appdata" {
                if n == 4 {
                    return v(Level::Blocked, "Системная папка данных приложений");
                }
                let rest = &c[3..];
                let junk = rest.iter().any(|s| matches!(s.as_str(), "temp" | "cache" | "cache2" | "code cache" | "gpucache" | "cache_data" | "dxcache" | "d3dscache" | "glcache" | "crashdumps" | "npm-cache" | "shadercache"));
                if junk && !(n == 5 && rest[1] == "temp") {
                    return v(Level::Ok, "");
                }
                if n == 5 && rest[1] == "temp" {
                    return v(Level::Warn, "Папка Temp. Лучше удалять её содержимое, а не саму папку");
                }
                if n >= 5 && rest[0] == "local" && rest[1] == "microsoft" && rest.get(2).is_some_and(|s| s == "windows" || s == "windowsapps") {
                    return v(Level::Blocked, "Системные данные Windows");
                }
                if n >= 5 && rest[0] == "local" && rest[1] == "packages" {
                    return v(Level::Warn, "Данные приложений Microsoft Store");
                }
                return v(Level::Warn, "Настройки и данные программ. Программа может потерять настройки или перестать работать");
            }
        }
        _ => {}
    }
    if !is_dir {
        let ext = crate::tree::ext_of(name);
        let in_downloads = c.iter().any(|s| s == "downloads");
        if matches!(ext.as_str(), "sys" | "dll" | "exe" | "drv" | "ocx") && !in_downloads {
            return v(Level::Warn, "Исполняемый или системный файл. Программа, которой он принадлежит, может перестать работать");
        }
    }
    v(Level::Ok, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules() {
        let b = |p: &str, d: bool| classify(p, d).level;
        // «Очистка» empties these folders: their contents must be allowed
        let inside = |p: &str| classify(&crate::analysis::content_probe(p), true).level;
        assert_eq!(inside(r"C:\Users\Alex\AppData\Local\Temp"), Level::Ok);
        assert_eq!(inside(r"C:\Windows\Temp"), Level::Ok);
        assert_eq!(inside(r"C:\Users\Alex\AppData\Local\NVIDIA\DXCache"), Level::Ok);
        assert_eq!(inside(r"C:\Users\Alex\AppData\Local\Google\Chrome\User Data\Default\Cache"), Level::Ok);
        assert_eq!(b(r"C:\", true), Level::Blocked);
        assert_eq!(b(r"C:\Windows", true), Level::Blocked);
        assert_eq!(b(r"C:\Windows\System32\kernel32.dll", false), Level::Blocked);
        assert_eq!(b(r"C:\Windows\Temp", true), Level::Blocked);
        assert_eq!(b(r"C:\Windows\Temp\x.tmp", false), Level::Ok);
        assert_eq!(b(r"C:\Windows\SoftwareDistribution\Download\abc", true), Level::Ok);
        assert_eq!(b(r"C:\pagefile.sys", false), Level::Blocked);
        assert_eq!(b(r"C:\$MFT", false), Level::Blocked);
        assert_eq!(b(r"C:\Users\Alex", true), Level::Blocked);
        assert_eq!(b(r"C:\Users\Alex\Documents", true), Level::Blocked);
        assert_eq!(b(r"C:\Users\Alex\Documents\a.docx", false), Level::Ok);
        assert_eq!(b(r"C:\Users\Alex\AppData", true), Level::Blocked);
        assert_eq!(b(r"C:\Users\Alex\AppData\Roaming\Telegram Desktop", true), Level::Warn);
        assert_eq!(b(r"C:\Users\Alex\AppData\Local\Temp", true), Level::Warn);
        assert_eq!(b(r"C:\Users\Alex\AppData\Local\Temp\abc", true), Level::Ok);
        assert_eq!(b(r"C:\Users\Alex\AppData\Local\Google\Chrome\User Data\Default\Cache", true), Level::Ok);
        assert_eq!(b(r"C:\Program Files\Adobe", true), Level::Warn);
        assert_eq!(b(r"C:\Program Files\WindowsApps\x", true), Level::Blocked);
        assert_eq!(b(r"C:\Games\x.exe", false), Level::Warn);
        assert_eq!(b(r"C:\Users\Alex\Downloads\setup.exe", false), Level::Ok);
        assert_eq!(b(r"D:\Movies\film.mkv", false), Level::Ok);
        assert_eq!(b(r"C:\Windows.old", true), Level::Warn);
    }
}

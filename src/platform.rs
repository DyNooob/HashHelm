//! Platform paths are independent of the working directory and of writable resources.
#[cfg(unix)]
use std::ffi::OsString;
use std::{
    env,
    path::{Path, PathBuf},
};

pub const OS: &str = env::consts::OS;
pub const ARCH: &str = env::consts::ARCH;

pub fn project_url(key: &str) -> Option<&'static str> {
    match key {
        "website" | "digiforensics" => Some("https://digiforensics.cn"),
        "forendev" => Some("https://www.foren.dev"),
        "downloads" => Some("https://foren.dev/#projects"),
        "repository" => Some("https://github.com/DyNooob/HashHelm"),
        "hashcat_drivers" => Some("https://hashcat.net/faq/wrongdriver"),
        "microsoft_privacy" => Some("https://privacy.microsoft.com/privacystatement"),
        _ => None,
    }
}

/// Used only with the updater's recorded, successfully verified download folder.
pub fn open_directory(path: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    if !path.is_absolute() || !path.is_dir() {
        return Err("Download folder is unavailable".into());
    }
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let system = env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:/Windows"));
        let mut c = Command::new(system.join("explorer.exe"));
        c.creation_flags(0x08000000);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = Command::new("/usr/bin/open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    let mut child = command
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn open_project_link(key: &str) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let url = project_url(key).ok_or("Unknown project link")?;
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let system = env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:/Windows"));
        let mut command = Command::new(system.join("System32/rundll32.exe"));
        command
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(0x08000000);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/open");
        command.arg(url);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn setting(name: &str, legacy: &str) -> Option<std::ffi::OsString> {
    env::var_os(name).or_else(|| env::var_os(legacy))
}

fn existing_data(preferred: PathBuf, legacy: PathBuf) -> PathBuf {
    if !preferred.exists() && legacy.is_dir() {
        legacy
    } else {
        preferred
    }
}

pub fn resources_for(exe: &Path, source: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new("."));
    // macOS: HashHelm.app/Contents/MacOS/HashHelm; Resources is immutable at runtime.
    let bundle = dir.parent().unwrap_or(dir).join("Resources");
    if dir.file_name().is_some_and(|s| s == "MacOS") && bundle.join("assets").is_dir() {
        bundle
    } else if dir.join("assets").is_dir() || dir.join("hashcat").is_dir() {
        dir.to_owned()
    } else if source.join("Cargo.toml").is_file() && source.join("assets").is_dir() {
        // Development builds only; installed packages always include assets.
        source.to_owned()
    } else {
        dir.to_owned()
    }
}

pub fn resource_root() -> PathBuf {
    resources_for(
        &env::current_exe().unwrap_or_default(),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
}

pub fn data_for(
    os: &str,
    root: &Path,
    home: Option<&Path>,
    xdg: Option<&Path>,
    override_dir: Option<&Path>,
    portable: bool,
) -> Result<PathBuf, String> {
    if let Some(path) = override_dir.filter(|p| p.is_absolute()) {
        return Ok(path.to_owned());
    }
    if override_dir.is_some() {
        return Err("HASHHELM_DATA_DIR must be an absolute path".into());
    }
    if os == "windows" || (portable && os == "linux") {
        return Ok(root.join("workspace"));
    }
    if os == "linux"
        && let Some(path) = xdg.filter(|p| p.is_absolute())
    {
        return Ok(path.join("digiforensics/hashhelm"));
    }
    let home = home
        .filter(|p| p.is_absolute())
        .ok_or("Cannot locate the user data directory; set HASHHELM_DATA_DIR")?;
    Ok(if os == "macos" {
        home.join("Library/Application Support/DigiForensics/HashHelm")
    } else {
        home.join(".local/share/digiforensics/hashhelm")
    })
}

pub fn data_dir(root: &Path) -> Result<PathBuf, String> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let xdg = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let override_dir = setting("HASHHELM_DATA_DIR", "FORENHC_DATA_DIR").map(PathBuf::from);
    let preferred = data_for(
        OS,
        root,
        home.as_deref(),
        xdg.as_deref(),
        override_dir.as_deref(),
        root.join("portable.flag").is_file(),
    )?;
    if override_dir.is_some() || OS == "windows" || root.join("portable.flag").is_file() {
        return Ok(preferred);
    }
    let legacy = preferred.with_file_name(if OS == "macos" { "ForenHC" } else { "forenhc" });
    Ok(existing_data(preferred, legacy))
}

pub fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn engine_for(os: &str, root: &Path, search_path: Option<&std::ffi::OsStr>) -> PathBuf {
    let names: &[&str] = match os {
        "windows" => &["hashcat.exe"],
        "linux" => &["hashcat.bin", "hashcat"],
        _ => &["hashcat", "hashcat.bin"],
    };
    for folder in [
        root.join("hashcat"),
        root.join("vendor/hashcat-8.0.0"),
        root.join("vendor/hashcat-src-8.0.0"),
    ] {
        for name in names {
            let path = folder.join(name);
            if executable(&path) {
                return path;
            }
        }
    }
    // Finder does not inherit a terminal's Homebrew PATH.
    let mut folders: Vec<PathBuf> = search_path
        .map(env::split_paths)
        .into_iter()
        .flatten()
        .filter(|p| p.is_absolute())
        .collect();
    if os == "macos" {
        folders.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]);
    }
    for folder in folders {
        for name in names {
            let path = folder.join(name);
            if executable(&path) {
                return path;
            }
        }
    }
    root.join("hashcat").join(names[0])
}

pub fn engine_path(root: &Path) -> PathBuf {
    if let Some(path) = setting("HASHHELM_HASHCAT", "FORENHC_HASHCAT")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && executable(p))
    {
        return path;
    }
    engine_for(OS, root, env::var_os("PATH").as_deref())
}

pub fn engine_workdir(exe: &Path) -> PathBuf {
    // Follow /opt/homebrew/bin/hashcat symlinks to the real runtime tree.
    exe.canonicalize()
        .unwrap_or_else(|_| exe.to_owned())
        .parent()
        .unwrap_or(Path::new("."))
        .to_owned()
}

pub fn display_name() -> String {
    format!(
        "HashHelm {} / {} {}",
        env!("CARGO_PKG_VERSION"),
        match OS {
            "macos" => "macOS",
            "linux" => "Linux",
            _ => "Windows",
        },
        match ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            other => other,
        }
    )
}

// A watchdog keeps a pipe open to the UI. Even a crash closes the pipe, so the
// independent watchdog terminates the engine process group on Unix.
#[cfg(unix)]
pub fn guardian_entry() -> Option<i32> {
    use std::io::Read;
    let args: Vec<OsString> = env::args_os().collect();
    if args
        .get(1)
        .is_none_or(|s| s != "--hashhelm-engine-guardian" && s != "--forenhc-engine-guardian")
    {
        return None;
    }
    let Some(pid) = args
        .get(2)
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|p| *p > 1)
    else {
        return Some(2);
    };
    let mut byte = [0u8; 64];
    while let Ok(n) = std::io::stdin().read(&mut byte) {
        if n == 0 {
            break;
        }
    }
    // The process group is created by our engine spawn, owned by the same user.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    Some(0)
}
#[cfg(not(unix))]
pub fn guardian_entry() -> Option<i32> {
    None
}

#[cfg(unix)]
pub struct ProcessGroup {
    pid: i32,
    guardian: std::process::Child,
    pipe: Option<std::process::ChildStdin>,
}
#[cfg(unix)]
impl ProcessGroup {
    pub fn attach(child: &std::process::Child) -> std::io::Result<Self> {
        use std::process::{Command, Stdio};
        let mut guardian = Command::new(env::current_exe()?)
            .args(["--hashhelm-engine-guardian", &child.id().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let pipe = guardian.stdin.take();
        Ok(Self {
            pid: child.id() as i32,
            guardian,
            pipe,
        })
    }
    pub fn kill(&self) {
        unsafe {
            libc::kill(-self.pid, libc::SIGKILL);
        }
    }
}
#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.kill();
        self.pipe.take();
        let _ = self.guardian.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_data_is_separate_from_bundle_and_cwd() {
        let base = env::temp_dir();
        let root = &base.join("Applications/HashHelm.app/Contents/Resources");
        let home = &base.join("Users/jenny");
        let xdg = &base.join("data");
        assert_eq!(
            data_for("macos", root, Some(home), None, None, true).unwrap(),
            home.join("Library/Application Support/DigiForensics/HashHelm")
        );
        assert_eq!(
            data_for("linux", root, Some(home), Some(xdg), None, false).unwrap(),
            xdg.join("digiforensics/hashhelm")
        );
        assert_eq!(
            data_for(
                "linux",
                root,
                Some(home),
                Some(Path::new("relative")),
                None,
                false
            )
            .unwrap(),
            home.join(".local/share/digiforensics/hashhelm")
        );
        assert_eq!(
            data_for("linux", root, None, None, None, true).unwrap(),
            root.join("workspace")
        );
        assert!(data_for("macos", root, None, None, None, false).is_err());
        assert!(
            data_for(
                "linux",
                root,
                Some(home),
                None,
                Some(Path::new("relative")),
                false
            )
            .is_err()
        );
    }
    #[test]
    fn missing_engine_uses_platform_name() {
        let dir =
            std::env::temp_dir().join(format!("hashhelm-engine-missing-{}", std::process::id()));
        assert!(engine_for("windows", &dir, None).ends_with("hashcat/hashcat.exe"));
        assert!(engine_for("linux", &dir, None).ends_with("hashcat/hashcat.bin"));
        // macOS may discover a real Homebrew installation; only assert if absent.
        let mac = engine_for("macos", &dir, Some(std::ffi::OsStr::new("")));
        assert!(mac.ends_with("hashcat/hashcat") || executable(&mac));
    }
    #[test]
    fn bundle_and_portable_resources_ignore_current_directory() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/artifacts/platform-layouts");
        let resources = base.join("HashHelm.app/Contents/Resources");
        std::fs::create_dir_all(resources.join("assets")).unwrap();
        let exe = base.join("HashHelm.app/Contents/MacOS/HashHelm");
        assert_eq!(resources_for(&exe, Path::new("missing-source")), resources);
        let portable = base.join("portable");
        std::fs::create_dir_all(portable.join("assets")).unwrap();
        assert_eq!(
            resources_for(&portable.join("HashHelm"), Path::new("missing-source")),
            portable
        );
    }
    #[test]
    fn renamed_app_reuses_existing_data_without_overwriting() {
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/artifacts/rename-compatibility");
        let legacy = base.join("ForenHC");
        std::fs::create_dir_all(&legacy).unwrap();
        let preferred = base.join("HashHelm");
        assert_eq!(existing_data(preferred.clone(), legacy.clone()), legacy);
        let present = base.join("current/HashHelm");
        std::fs::create_dir_all(&present).unwrap();
        assert_eq!(existing_data(present.clone(), legacy), present);
    }
    #[test]
    fn external_links_are_limited_to_author_supplied_addresses() {
        assert_eq!(project_url("website"), Some("https://digiforensics.cn"));
        assert_eq!(project_url("forendev"), Some("https://www.foren.dev"));
        assert_eq!(
            project_url("repository"),
            Some("https://github.com/DyNooob/HashHelm")
        );
        assert_eq!(project_url("file:///etc/passwd"), None);
        assert_eq!(project_url("https://example.com"), None);
    }
}

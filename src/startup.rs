//! Recoverable startup with an application log and a native error dialog.
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
pub fn os_details() -> Value {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};
        let mut version: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
        version.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if unsafe { GetVersionExW(&mut version) } != 0 {
            return json!({"family":"Windows","major":version.dwMajorVersion,"minor":version.dwMinorVersion,"build":version.dwBuildNumber});
        }
    }
    json!({"family":crate::platform::OS})
}
pub fn writable(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    let probe = path.join(format!(
        ".hashhelm-write-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&probe)?;
        file.write_all(b"HashHelm write check")?;
        file.sync_all()
    })();
    let _ = std::fs::remove_file(probe);
    result.map_err(|e| e.to_string())
}
fn fallback_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
        .join("DigiForensics/HashHelm")
}
pub fn prepare_data(root: &Path) -> Result<PathBuf> {
    let preferred = crate::platform::data_dir(root)?;
    match writable(&preferred) {
        Ok(()) => Ok(preferred),
        Err(error)
            if cfg!(windows)
                && crate::platform::setting("HASHHELM_DATA_DIR", "FORENHC_DATA_DIR").is_none() =>
        {
            let fallback = fallback_data();
            writable(&fallback)?;
            let _ = crate::app_log::init(&fallback);
            crate::app_log::event(
                "warning",
                "data_directory_fallback",
                json!({"preferred":preferred,"selected":fallback,"reason":error}),
            );
            Ok(fallback)
        }
        Err(error) => Err(format!(
            "工作区不可写 / Data directory is not writable: {}\n{error}",
            preferred.display()
        )),
    }
}
pub fn initialize_log(data: &Path) -> Result<()> {
    if crate::app_log::current().is_none() {
        crate::app_log::init(data).map_err(|e| e.to_string())?;
    }
    crate::app_log::event(
        "info",
        "starting",
        json!({"version":env!("CARGO_PKG_VERSION"),"platform":crate::platform::OS,"arch":crate::platform::ARCH,"data":data,"resources":crate::platform::resource_root()}),
    );
    std::panic::set_hook(Box::new(|info| {
        crate::app_log::event(
            "error",
            "panic",
            json!({"location":info.location().map(|p|format!("{}:{}",p.file(),p.line())),"backtrace":std::backtrace::Backtrace::force_capture().to_string().chars().take(8000).collect::<String>()}),
        );
        show_error(&format!(
            "程序发生异常 / An unexpected error occurred.\n{info}"
        ));
    }));
    Ok(())
}
pub fn show_error(message: &str) {
    let path = crate::app_log::current()
        .map(|l| l.path().display().to_string())
        .unwrap_or_else(|| "unavailable".into());
    let text = format!(
        "HashHelm 无法继续 / HashHelm could not continue\n\n{message}\n\n日志 / Log: {path}\n\n请完整解压便携包后重试。可运行 Diagnose.cmd 生成诊断。\nExtract the complete package; run Diagnose.cmd for a report."
    );
    eprintln!("{text}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let text = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let title = "HashHelm · 启动错误 / Startup error"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}
pub fn fatal(message: &str) {
    if crate::app_log::current().is_none() {
        let _ = crate::app_log::init(&fallback_data());
    }
    crate::app_log::event("error", "fatal", json!({"message":message}));
    show_error(message);
}
pub fn runtime(root: &Path) -> Result<Value> {
    #[cfg(windows)]
    {
        let bundled = root.join("runtime/webview2");
        let explicit = std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let (source, path) = if let Some(path) = explicit {
            ("override", Some(path))
        } else if bundled.is_dir() {
            ("bundled", Some(bundled))
        } else {
            ("system", None)
        };
        if let Some(path) = path {
            if !path.is_absolute()
                || !path.join("msedgewebview2.exe").is_file()
                || !path.join("msedge.dll").is_file()
            {
                return Err(format!(
                    "WebView2 运行时不完整 / Incomplete WebView2 runtime: {}",
                    path.display()
                ));
            }
            if source == "bundled" {
                // Required by Microsoft for unpackaged Win10 apps using Fixed Runtime
                // 120+. Grant only read/execute on public runtime files, never on data.
                use std::os::windows::process::CommandExt;
                let system = std::env::var_os("SystemRoot")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("C:/Windows"));
                let output = std::process::Command::new(system.join("System32/icacls.exe"))
                    .arg(&path)
                    .args([
                        "/grant",
                        "*S-1-15-2-2:(OI)(CI)(RX)",
                        "/grant",
                        "*S-1-15-2-1:(OI)(CI)(RX)",
                        "/Q",
                    ])
                    .creation_flags(0x08000000)
                    .output()
                    .map_err(|e| e.to_string())?;
                crate::app_log::event(
                    if output.status.success() {
                        "info"
                    } else {
                        "warning"
                    },
                    "runtime_read_execute_access",
                    json!({"path":path,"success":output.status.success()}),
                );
            }
            // This runs before GUI/COM and before any application thread starts.
            unsafe {
                std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &path);
            }
            let manifest = std::fs::read(root.join("runtime/runtime-manifest.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            return Ok(
                json!({"source":source,"path":path,"version":manifest.as_ref().and_then(|m|m["version"].as_str()),"vcRuntime":"statically-linked"}),
            );
        }
        Ok(json!({"source":"system","path":null,"vcRuntime":"statically-linked"}))
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Ok(json!({"source":"system-webkit","path":null}))
    }
}
pub fn support_bundle(root: &Path, data: &Path, runtime: &Value) -> Value {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default();
    let log = crate::app_log::view();
    let entries = log["text"]
        .as_str()
        .unwrap_or("")
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let bundle = json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),"platform":crate::platform::OS,"osVersion":os_details(),"arch":crate::platform::ARCH,"runtime":runtime,"engineExists":crate::platform::executable(&crate::platform::engine_path(root)),"applicationLog":{"path":log["path"],"entries":entries},"privacy":"Excludes settings, tasks, engine logs, dictionaries, candidates and recovered passwords."});
    crate::app_log::redact(
        bundle,
        &[(data, "<DATA>"), (root, "<APP>"), (&home, "<USER>")],
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writable_probe_preserves_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("existing.txt");
        std::fs::write(&source, b"preserve").unwrap();
        assert!(writable(temp.path()).is_ok());
        assert_eq!(std::fs::read(&source).unwrap(), b"preserve");
        assert!(writable(&source).is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

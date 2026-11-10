//! Small structured application log. Never log IPC bodies, argv or passwords.
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
static LOGGER: OnceLock<Logger> = OnceLock::new();
const LIMIT: u64 = 2 * 1024 * 1024;
pub struct Logger {
    path: PathBuf,
    file: Mutex<(File, u64)>,
}
fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
impl Logger {
    pub fn new(directory: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(directory)?;
        let path = directory.join(format!("app-{}-{}.jsonl", stamp(), std::process::id()));
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)?;
        let mut old = std::fs::read_dir(directory)?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_ok_and(|t| t.is_file())
                    && e.file_name()
                        .to_str()
                        .is_some_and(|s| s.starts_with("app-") && s.ends_with(".jsonl"))
            })
            .map(|e| e.path())
            .collect::<Vec<_>>();
        old.sort();
        let remove = old.len().saturating_sub(20);
        for old in old.into_iter().take(remove) {
            if old != path {
                let _ = std::fs::remove_file(old);
            }
        }
        Ok(Self {
            path,
            file: Mutex::new((file, 0)),
        })
    }
    pub fn event(&self, level: &str, event: &str, detail: Value) {
        let value = json!({"timeUtcMs":stamp(),"level":level,"event":event,"pid":std::process::id(),"detail":detail});
        let Ok(mut line) = serde_json::to_vec(&value) else {
            return;
        };
        if line.len() > 16384 {
            return;
        }
        line.push(b'\n');
        let mut file = self.file.lock().unwrap_or_else(|p| p.into_inner());
        if file.1 + line.len() as u64 > LIMIT {
            return;
        }
        if file.0.write_all(&line).is_ok() {
            file.1 += line.len() as u64;
            let _ = file.0.flush();
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn tail(&self) -> std::io::Result<String> {
        let _guard = self.file.lock().unwrap_or_else(|p| p.into_inner());
        let mut file = File::open(&self.path)?;
        let size = file.metadata()?.len();
        file.seek(SeekFrom::Start(size.saturating_sub(256 * 1024)))?;
        let mut bytes = Vec::new();
        file.take(256 * 1024).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
pub fn init(preferred: &Path) -> std::io::Result<&'static Logger> {
    let logger = Logger::new(&preferred.join("logs"))?;
    let _ = LOGGER.set(logger);
    Ok(LOGGER.get().unwrap())
}
pub fn current() -> Option<&'static Logger> {
    LOGGER.get()
}
pub fn event(level: &str, name: &str, detail: Value) {
    if let Some(log) = current() {
        log.event(level, name, detail);
    }
}
pub fn operation(op: &str) {
    if matches!(
        op,
        "queue_submit"
            | "queue_control"
            | "diagnose"
            | "analyze_plan"
            | "profile"
            | "verify_file"
            | "results"
            | "import"
            | "run"
            | "stop"
            | "close"
            | "support_bundle"
            | "update_check"
            | "update_download"
            | "update_cancel"
    ) {
        event("info", "operation", json!({"operation":op}));
    }
}
pub fn response(value: &Value) {
    let kind = value["type"].as_str().unwrap_or("");
    if matches!(
        kind,
        "error"
            | "import_error"
            | "results_error"
            | "profile_error"
            | "diagnose_error"
            | "preflight_error"
            | "file_verify_error"
            | "queue_error"
            | "rejected"
            | "update_error"
    ) {
        // Deliberately exclude the backend message: third-party errors may echo input.
        event("error", "operation_failed", json!({"type":kind}));
    } else if matches!(
        kind,
        "finished"
            | "imported"
            | "results"
            | "profile_done"
            | "diagnosed"
            | "file_verified"
            | "update_checked"
            | "update_downloaded"
    ) {
        event(
            "info",
            "operation_finished",
            json!({"type":kind,"exitCode":value.get("code")}),
        );
    }
    if kind == "queue_event" {
        response(&value["event"]);
    }
}
pub fn view() -> Value {
    match current() {
        Some(log) => json!({"path":log.path(),"text":log.tail().unwrap_or_else(|e|e.to_string())}),
        None => json!({"path":null,"text":"Application logging is unavailable"}),
    }
}
pub fn redact(value: Value, roots: &[(&Path, &str)]) -> Value {
    match value {
        Value::String(mut s) => {
            for (path, label) in roots {
                let path = path.to_string_lossy();
                if !path.is_empty() {
                    s = s
                        .replace(path.as_ref(), label)
                        .replace(&path.replace('\\', "/"), label);
                }
            }
            Value::String(s)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(|v| redact(v, roots)).collect()),
        Value::Object(o) => {
            Value::Object(o.into_iter().map(|(k, v)| (k, redact(v, roots))).collect())
        }
        v => v,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_logs_and_redaction() {
        let temp = tempfile::tempdir().unwrap();
        let log = Logger::new(temp.path()).unwrap();
        for _ in 0..500 {
            log.event("info", "test", json!({"detail":"x".repeat(15000)}));
        }
        assert!(log.path().metadata().unwrap().len() <= LIMIT);
        assert!(log.tail().unwrap().len() <= 256 * 1024);
        let value = redact(
            json!({"path":temp.path().join("a.txt")}),
            &[(temp.path(), "<DATA>")],
        );
        assert!(value["path"].as_str().unwrap().starts_with("<DATA>"));
    }
}

//! Keep only the newest queued snapshot and replace settings atomically.
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

pub fn load(data: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(data.join("settings.json")).ok()?).ok()
}

pub fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let data = path.parent().ok_or("Missing data directory")?;
    let temporary = data.join(format!(
        ".{}-{}-{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        serde_json::to_writer(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        // Same-directory rename preserves the last complete snapshot on failure.
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("无法保存任务配置：{e}"))
}

enum Request {
    Save,
    Flush(mpsc::Sender<Result<(), String>>),
    Shutdown,
}

pub struct Writer {
    pending: Arc<Mutex<Option<Value>>>,
    sender: mpsc::SyncSender<Request>,
    worker: Option<JoinHandle<()>>,
}

impl Writer {
    pub fn new(data: PathBuf, on_error: impl Fn(String) + Send + 'static) -> Self {
        let pending = Arc::new(Mutex::new(None));
        let latest = pending.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let mut result = Ok(());
            while let Ok(request) = receiver.recv() {
                let snapshot = latest.lock().unwrap().take();
                if let Some(value) = snapshot {
                    result = write_json(&data.join("settings.json"), &value);
                    if let Err(message) = &result {
                        on_error(message.clone());
                    }
                }
                match request {
                    Request::Save => {}
                    Request::Flush(reply) => {
                        let _ = reply.send(result.clone());
                    }
                    Request::Shutdown => break,
                }
            }
        });
        Self {
            pending,
            sender,
            worker: Some(worker),
        }
    }

    pub fn save(&self, value: Value) -> Result<(), String> {
        *self.pending.lock().unwrap() = Some(value);
        match self.sender.try_send(Request::Save) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => Ok(()),
            Err(mpsc::TrySendError::Disconnected(_)) => Err("配置保存线程已停止".into()),
        }
    }

    pub fn flush(&self) -> Result<(), String> {
        let (sender, receiver) = mpsc::channel();
        self.sender
            .send(Request::Flush(sender))
            .map_err(|e| e.to_string())?;
        receiver.recv().map_err(|e| e.to_string())?
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.sender.send(Request::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn latest_snapshot_survives_flush_and_shutdown() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/artifacts/settings-latest");
        fs::create_dir_all(&data).unwrap();
        let writer = Writer::new(data.clone(), |message| panic!("{message}"));
        for n in 0..1000 {
            writer
                .save(json!({"groups":[{"progress":n}],"preferences":{"language":"en"}}))
                .unwrap();
        }
        writer.flush().unwrap();
        assert_eq!(load(&data).unwrap()["groups"][0]["progress"], 999);
        writer.save(json!({"groups":["last task"]})).unwrap();
        drop(writer);
        assert_eq!(load(&data).unwrap()["groups"][0], "last task");
        assert!(
            fs::read_dir(&data).unwrap().all(|p| !p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }

    #[test]
    fn failed_replace_preserves_existing_destination_and_reports_error() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/artifacts/settings-failure");
        fs::create_dir_all(data.join("settings.json")).unwrap();
        fs::write(
            data.join("settings.json/keep.txt"),
            "preserve existing data",
        )
        .unwrap();
        let (sender, receiver) = mpsc::channel();
        let writer = Writer::new(data.clone(), move |message| {
            let _ = sender.send(message);
        });
        writer.save(json!({"groups":[]})).unwrap();
        assert!(writer.flush().is_err());
        assert!(receiver.recv().unwrap().contains("无法保存任务配置"));
        assert_eq!(
            fs::read_to_string(data.join("settings.json/keep.txt")).unwrap(),
            "preserve existing data"
        );
        assert!(
            fs::read_dir(&data).unwrap().all(|p| !p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }
}

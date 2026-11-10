//! Foren DEV updates: HTTPS metadata, explicit downloads, no installation or execution.
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const SERVICE: &str = "https://foren.dev/updates/v1";
const MAX_METADATA: u64 = 1024 * 1024;
const MAX_PACKAGE: u64 = 2 * 1024 * 1024 * 1024;
type Result<T> = std::result::Result<T, String>;

pub fn build_number() -> u32 {
    env!("HASHHELM_BUILD_NUMBER").parse().unwrap()
}
pub fn architecture() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        other => other,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    pub channel: String,
    pub lang: String,
}
impl Options {
    pub fn new(channel: &str, lang: &str) -> Result<Self> {
        if !matches!(channel, "stable" | "beta") || !matches!(lang, "zh" | "en") {
            return Err("invalid_options".into());
        }
        Ok(Self {
            channel: channel.into(),
            lang: lang.into(),
        })
    }
    fn url(&self) -> String {
        format!(
            "{SERVICE}/check/?app_id=hashhelm&version={}&build={}&platform={}&arch={}&channel={}&lang={}",
            env!("CARGO_PKG_VERSION").replace('+', "%2B"),
            build_number(),
            std::env::consts::OS,
            architecture(),
            self.channel,
            self.lang
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub platform: String,
    pub arch: String,
    pub package_type: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub release_id: u64,
    pub version: String,
    pub build: u32,
    pub release_notes: String,
    pub published_at: String,
    pub minimum_version: Option<String>,
    pub artifact: Artifact,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Current {
    pub version: String,
    pub build: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct App {
    pub app_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub ok: bool,
    pub schema_version: u32,
    pub app: App,
    pub channel: String,
    pub platform: String,
    pub arch: String,
    pub current: Current,
    pub available: bool,
    pub update_available: bool,
    pub update_required: bool,
    pub status: String,
    pub latest: Option<Release>,
}

fn https_url(raw: &str) -> bool {
    if raw.len() > 8192 || raw.contains(['#', '\\']) || raw.chars().any(char::is_control) {
        return false;
    }
    raw.parse::<ureq::http::Uri>().is_ok_and(|u| {
        u.scheme_str() == Some("https")
            && u.authority().is_some_and(|a| !a.as_str().contains('@'))
            && u.host().is_some_and(|h| !h.is_empty())
    })
}
pub fn validate(report: &Report, options: &Options) -> Result<()> {
    let invalid = || "metadata".to_owned();
    if !report.ok
        || report.schema_version != 1
        || report.app.app_id != "hashhelm"
        || report.channel != options.channel
        || report.platform != std::env::consts::OS
        || report.arch != architecture()
        || report.current.version != env!("CARGO_PKG_VERSION")
        || report.current.build != build_number()
        || report.available != report.latest.is_some()
        || report.update_required && !report.update_available
    {
        return Err(invalid());
    }
    let Some(release) = &report.latest else {
        return if report.status == "no_release"
            && !report.update_available
            && !report.update_required
        {
            Ok(())
        } else {
            Err(invalid())
        };
    };
    let remote = Version::parse(&release.version).map_err(|_| invalid())?;
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|_| invalid())?;
    let a = &release.artifact;
    if release.release_id == 0
        || release.version.len() > 128
        || !(1..=i32::MAX as u32).contains(&release.build)
        || options.channel == "stable" && !remote.pre.is_empty()
        || release.release_notes.len() > 32768
        || release.published_at.len() > 64
        || a.platform != report.platform
        || (a.arch != report.arch && a.arch != "any")
        || !matches!(a.package_type.as_str(), "portable" | "installer")
        || !https_url(&a.url)
        || a.sha256.len() != 64
        || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || a.size_bytes.is_some_and(|n| n == 0 || n > MAX_PACKAGE)
    {
        return Err(invalid());
    }
    let order = remote
        .cmp_precedence(&current)
        .then(release.build.cmp(&build_number()));
    let status = if order.is_gt() {
        if remote.cmp_precedence(&current).is_eq() {
            "new_build_available"
        } else {
            "update_available"
        }
    } else if order.is_lt() {
        "current_is_newer"
    } else {
        "up_to_date"
    };
    let required = if let Some(minimum) = &release.minimum_version {
        current
            .cmp_precedence(&Version::parse(minimum).map_err(|_| invalid())?)
            .is_lt()
            && order.is_gt()
    } else {
        false
    };
    if report.status != status
        || report.update_available != order.is_gt()
        || report.update_required != required
    {
        return Err(invalid());
    }
    Ok(())
}

fn agent(download: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .http_status_as_error(false)
        .max_redirects(5)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_per_read(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(if download { 1800 } else { 10 })))
        .user_agent(concat!(
            "HashHelm/",
            env!("CARGO_PKG_VERSION"),
            " Foren.Dev-Updater/1"
        ))
        .build()
        .into()
}
#[derive(Serialize, Deserialize)]
struct Cache {
    etag: Option<String>,
    report: Report,
}
fn cache_path(data: &Path, options: &Options) -> PathBuf {
    data.join("cache")
        .join(format!("updates-{}-{}.json", options.channel, options.lang))
}
fn cached(data: &Path, options: &Options) -> Option<Cache> {
    let path = cache_path(data, options);
    if fs::metadata(&path).ok()?.len() > MAX_METADATA {
        return None;
    }
    let cache: Cache = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    validate(&cache.report, options).ok()?;
    Some(cache)
}
pub fn check(data: &Path, options: &Options) -> Result<Report> {
    Options::new(&options.channel, &options.lang)?;
    let cache = cached(data, options);
    let client = agent(false);
    let mut request = client
        .get(options.url())
        .header("Accept", "application/json");
    if let Some(etag) = cache
        .as_ref()
        .and_then(|c| c.etag.as_ref())
        .filter(|e| e.len() <= 512)
    {
        request = request.header("If-None-Match", etag);
    }
    let mut response = request.call().map_err(|_| "network")?;
    let report = match response.status().as_u16() {
        304 => cache.ok_or("metadata")?.report,
        200 => {
            let etag = response
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .filter(|s| s.len() <= 512)
                .map(str::to_owned);
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .take(MAX_METADATA + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "network")?;
            if bytes.len() as u64 > MAX_METADATA {
                return Err("metadata".into());
            }
            let report: Report = serde_json::from_slice(&bytes).map_err(|_| "metadata")?;
            validate(&report, options)?;
            let _ = fs::create_dir_all(data.join("cache"));
            let _ = crate::settings::write_json(
                &cache_path(data, options),
                &json!(Cache {
                    etag,
                    report: report.clone()
                }),
            );
            report
        }
        code => return Err(format!("http_{code}")),
    };
    validate(&report, options)?;
    Ok(report)
}

// The output is committed only after the entire archive passes both checks.
// A same-directory temporary file gives cancellation and errors automatic cleanup.
fn save_stream(
    mut input: impl Read,
    artifact: &Artifact,
    destination: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    if destination.exists() {
        return Err("exists".into());
    }
    let parent = destination.parent().filter(|p| p.is_dir()).ok_or("io")?;
    let mut temp = tempfile::Builder::new()
        .prefix(".hashhelm-update-")
        .suffix(".partial")
        .tempfile_in(parent)
        .map_err(|_| "io")?;
    let mut hash = Sha256::new();
    let mut received = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    let mut last = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        let n = input.read(&mut buffer).map_err(|_| "network")?;
        if n == 0 {
            break;
        }
        received = received.checked_add(n as u64).ok_or("size")?;
        if received > MAX_PACKAGE || artifact.size_bytes.is_some_and(|total| received > total) {
            return Err("size".into());
        }
        temp.write_all(&buffer[..n]).map_err(|_| "io")?;
        hash.update(&buffer[..n]);
        if last.elapsed() >= Duration::from_millis(150) {
            progress(received);
            last = Instant::now();
        }
    }
    if cancel.load(Ordering::SeqCst) {
        return Err("cancelled".into());
    }
    if artifact.size_bytes.is_some_and(|n| n != received) || received == 0 {
        return Err("size".into());
    }
    if !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&artifact.sha256) {
        return Err("checksum".into());
    }
    temp.as_file().sync_all().map_err(|_| "io")?;
    if cancel.load(Ordering::SeqCst) {
        return Err("cancelled".into());
    }
    temp.persist_noclobber(destination).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            "exists"
        } else {
            "io"
        }
    })?;
    progress(received);
    Ok(())
}

#[derive(Default)]
struct State {
    selection: Option<(Options, Report)>,
    saved: Option<PathBuf>,
}
#[derive(Clone)]
pub struct Controller {
    data: PathBuf,
    state: Arc<Mutex<State>>,
    busy: Arc<AtomicBool>,
    downloading: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}
impl Controller {
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            state: Arc::new(Mutex::new(State::default())),
            busy: Arc::new(AtomicBool::new(false)),
            downloading: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn downloading(&self) -> bool {
        self.downloading.load(Ordering::SeqCst)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn saved_folder(&self) -> Option<PathBuf> {
        self.state
            .lock()
            .unwrap()
            .saved
            .as_ref()
            .filter(|p| p.is_file())
            .and_then(|p| p.parent().map(Path::to_path_buf))
    }
    pub fn filename(&self) -> Option<String> {
        let state = self.state.lock().unwrap();
        let (_, report) = state.selection.as_ref()?;
        if !report.update_available {
            return None;
        }
        let r = report.latest.as_ref()?;
        let extension = [
            ".tar.gz",
            ".tar.xz",
            ".7z",
            ".zip",
            ".dmg",
            ".pkg",
            ".exe",
            ".msi",
            ".AppImage",
            ".deb",
            ".rpm",
        ]
        .into_iter()
        .find(|s| r.artifact.url.split('?').next().unwrap_or("").ends_with(s))
        .unwrap_or(".bin");
        Some(format!(
            "HashHelm-{}-build{}-{}-{}{extension}",
            r.version, r.build, report.platform, report.arch
        ))
    }
    pub fn check(&self, options: Options, id: u64, emit: impl Fn(Value) + Send + 'static) {
        if self.busy.swap(true, Ordering::SeqCst) {
            emit(json!({"type":"update_error","id":id,"code":"busy"}));
            return;
        }
        self.state.lock().unwrap().selection = None;
        let this = self.clone();
        std::thread::spawn(move || {
            let result = check(&this.data, &options);
            if let Ok(report) = &result {
                this.state.lock().unwrap().selection = Some((options, report.clone()));
            }
            this.busy.store(false, Ordering::SeqCst);
            emit(match result {
                Ok(report) => json!({"type":"update_checked","id":id,"report":report}),
                Err(code) => json!({"type":"update_error","id":id,"code":code}),
            });
        });
    }
    pub fn download(&self, destination: PathBuf, id: u64, emit: impl Fn(Value) + Send + 'static) {
        if self.busy.swap(true, Ordering::SeqCst) {
            emit(json!({"type":"update_error","id":id,"code":"busy"}));
            return;
        }
        let selected = self.state.lock().unwrap().selection.clone();
        self.cancel.store(false, Ordering::SeqCst);
        self.downloading.store(true, Ordering::SeqCst);
        let this = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let (options, old) = selected.ok_or("not_available")?;
                if !old.update_available {
                    return Err("not_available".into());
                }
                // Withdrawn or replaced metadata cannot authorize a new download.
                let fresh = check(&this.data, &options)?;
                let r = fresh
                    .latest
                    .as_ref()
                    .filter(|_| fresh.update_available)
                    .ok_or("withdrawn")?;
                let previous = old.latest.as_ref().ok_or("not_available")?;
                if r.release_id != previous.release_id
                    || r.version != previous.version
                    || r.build != previous.build
                    || r.artifact != previous.artifact
                {
                    return Err("withdrawn".into());
                }
                if this.cancel.load(Ordering::SeqCst) {
                    return Err("cancelled".into());
                }
                emit(
                    json!({"type":"update_progress","id":id,"received":0,"total":r.artifact.size_bytes}),
                );
                let mut response = agent(true)
                    .get(&r.artifact.url)
                    .header("Accept", "application/octet-stream")
                    .call()
                    .map_err(|_| "network")?;
                if response.status().as_u16() != 200 {
                    return Err(format!("http_{}", response.status().as_u16()));
                }
                if let Some(length) = response
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    && (length > MAX_PACKAGE || r.artifact.size_bytes.is_some_and(|n| n != length))
                {
                    return Err("size".into());
                }
                save_stream(
                    response.body_mut().as_reader(),
                    &r.artifact,
                    &destination,
                    &this.cancel,
                    |received| {
                        emit(
                            json!({"type":"update_progress","id":id,"received":received,"total":r.artifact.size_bytes}),
                        )
                    },
                )
            })();
            if result.is_ok() {
                this.state.lock().unwrap().saved = Some(destination.clone());
            }
            this.downloading.store(false, Ordering::SeqCst);
            this.busy.store(false, Ordering::SeqCst);
            emit(match result {
                Ok(()) => json!({"type":"update_downloaded","id":id,"path":destination}),
                Err(code) => json!({"type":"update_error","id":id,"code":code}),
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options() -> Options {
        Options::new("stable", "zh").unwrap()
    }
    fn report() -> Report {
        serde_json::from_value(json!({"ok":true,"schema_version":1,"app":{"app_id":"hashhelm"},"channel":"stable",
            "platform":std::env::consts::OS,"arch":architecture(),"current":{"version":env!("CARGO_PKG_VERSION"),"build":build_number()},
            "available":false,"update_available":false,"update_required":false,"status":"no_release","latest":null})).unwrap()
    }
    fn artifact(bytes: &[u8]) -> Artifact {
        Artifact {
            platform: std::env::consts::OS.into(),
            arch: architecture().into(),
            package_type: "portable".into(),
            url: "https://foren.dev/downloads/HashHelm-99.0.0.zip".into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: Some(bytes.len() as u64),
        }
    }
    fn available(version: &str, build: u32, status: &str) -> Report {
        let mut r = report();
        r.available = true;
        r.update_available = matches!(status, "update_available" | "new_build_available");
        r.status = status.into();
        r.latest = Some(Release {
            release_id: 1,
            version: version.into(),
            build,
            release_notes: "<b>plain text</b>".into(),
            published_at: "2026-11-10T00:00:00Z".into(),
            minimum_version: None,
            artifact: artifact(b"archive"),
        });
        r
    }
    #[test]
    fn state_and_version_comparison() {
        validate(&report(), &options()).unwrap();
        validate(&available("99.0.0", 1, "update_available"), &options()).unwrap();
        validate(
            &available(
                env!("CARGO_PKG_VERSION"),
                build_number() + 1,
                "new_build_available",
            ),
            &options(),
        )
        .unwrap();
        validate(&available("0.0.1", 1, "current_is_newer"), &options()).unwrap();
        validate(
            &available(env!("CARGO_PKG_VERSION"), build_number(), "up_to_date"),
            &options(),
        )
        .unwrap();
        let mut r = available("99.0.0", 1, "update_available");
        r.update_available = false;
        assert!(validate(&r, &options()).is_err());
        let mut r = report();
        r.status = "up_to_date".into();
        assert!(validate(&r, &options()).is_err());
        let r = available(
            &format!("{}+abc", env!("CARGO_PKG_VERSION")),
            build_number(),
            "up_to_date",
        );
        validate(&r, &options()).unwrap();
    }
    #[test]
    fn reject_wrong_targets_checksums_and_downgrades() {
        let good = available("99.0.0", 1, "update_available");
        for (key, value) in [
            ("app", json!({"app_id":"another"})),
            ("platform", json!("source")),
            ("arch", json!("unknown")),
            ("schema_version", json!(2)),
            ("update_required", json!(true)),
        ] {
            let mut v = json!(good);
            v[key] = value;
            let r: Report = serde_json::from_value(v).unwrap();
            assert!(validate(&r, &options()).is_err(), "{key}");
        }
        for (key, value) in [
            ("url", json!("http://foren.dev/a.zip")),
            ("url", json!("https://user:pass@foren.dev/a.zip")),
            ("url", json!("https://foren.dev/a.zip#fragment")),
            ("sha256", json!("bad")),
            ("size_bytes", json!(0)),
            ("size_bytes", json!(MAX_PACKAGE + 1)),
            ("package_type", json!("source")),
            ("platform", json!("source")),
            ("arch", json!("mystery")),
        ] {
            let mut v = json!(good);
            v["latest"]["artifact"][key] = value;
            assert!(
                validate(&serde_json::from_value(v).unwrap(), &options()).is_err(),
                "{key}"
            );
        }
        assert!(validate(&available("0.0.1", 1, "update_available"), &options()).is_err());
        assert!(
            validate(
                &available("99.0.0-beta.1", 1, "update_available"),
                &options()
            )
            .is_err()
        );
        let mut beta = available("99.0.0-beta.1", 1, "update_available");
        beta.channel = "beta".into();
        validate(&beta, &Options::new("beta", "en").unwrap()).unwrap();
    }
    #[test]
    fn verified_download_is_atomic_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("update.zip");
        let bytes = b"public synthetic archive bytes";
        let a = artifact(bytes);
        let cancel = AtomicBool::new(false);
        save_stream(&bytes[..], &a, &dest, &cancel, |_| {}).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), bytes);
        assert_eq!(
            save_stream(&b"changed"[..], &a, &dest, &cancel, |_| {}).unwrap_err(),
            "exists"
        );
        assert_eq!(fs::read(&dest).unwrap(), bytes);
        let fail = dir.path().join("bad.zip");
        let mut wrong = bytes.to_vec();
        wrong[0] ^= 1;
        assert_eq!(
            save_stream(&wrong[..], &a, &fail, &cancel, |_| {}).unwrap_err(),
            "checksum"
        );
        assert!(!fail.exists());
        assert_eq!(
            save_stream(&bytes[..3], &a, &fail, &cancel, |_| {}).unwrap_err(),
            "size"
        );
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            save_stream(&bytes[..], &a, &fail, &cancel, |_| {}).unwrap_err(),
            "cancelled"
        );
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "Partial files must be removed"
        );
    }
    #[test]
    fn cache_is_bound_to_current_version_and_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("cache")).unwrap();
        let path = cache_path(dir.path(), &options());
        crate::settings::write_json(
            &path,
            &json!(Cache {
                etag: Some("\"test\"".into()),
                report: report()
            }),
        )
        .unwrap();
        assert!(cached(dir.path(), &options()).is_some());
        let mut r = report();
        r.current.version = "0.0.1".into();
        crate::settings::write_json(
            &path,
            &json!(Cache {
                etag: None,
                report: r
            }),
        )
        .unwrap();
        assert!(cached(dir.path(), &options()).is_none());
    }

    #[test]
    fn interrupted_download_never_commits_and_removes_partial() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("interrupted.zip");
        let bytes = vec![42u8; 200_000];
        let artifact = artifact(&bytes);
        let cancel = AtomicBool::new(false);
        let result = save_stream(&bytes[..], &artifact, &dest, &cancel, |_| {
            cancel.store(true, Ordering::SeqCst)
        });
        assert_eq!(result.unwrap_err(), "cancelled");
        assert!(!dest.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        struct Interrupted;
        impl Read for Interrupted {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("connection lost"))
            }
        }
        cancel.store(false, Ordering::SeqCst);
        assert_eq!(
            save_stream(Interrupted, &artifact, &dest, &cancel, |_| {}).unwrap_err(),
            "network"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

//! One actor owns queue order and device reservations; workers own child processes.
use crate::{
    diagnostics::{self, Report},
    engine, settings,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Stage {
    pub label: String,
    pub args: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Spec {
    pub id: String,
    pub name: String,
    pub engine: PathBuf,
    pub stages: Vec<Stage>,
    #[serde(default)]
    pub context: Value,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageState {
    pub state: String,
    pub log_path: Option<PathBuf>,
    pub code: Option<i64>,
    pub seconds: Option<f64>,
    pub started_at: Option<u64>,
    pub message: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub spec: Spec,
    pub state: String,
    pub stages: Vec<StageState>,
    pub exclusive: bool,
    pub device_keys: BTreeSet<String>,
    pub file_keys: BTreeSet<String>,
    pub queued_at: u64,
    pub finished_at: Option<u64>,
    pub wait_reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub jobs: Vec<Job>,
    pub paused: bool,
    pub max_parallel: usize,
    pub active_count: usize,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            jobs: vec![],
            paused: false,
            max_parallel: 2,
            active_count: 0,
        }
    }
}
enum Request {
    Submit(Spec),
    Cancel(String),
    CancelAll,
    Pause(bool),
    Move(String, i32),
    Limit(usize),
    Inventory(Report),
    Event(String, usize, Value),
    Done(String, String),
    Shutdown,
}
type Emit = Arc<dyn Fn(Value) + Send + Sync>;
type Runner = Arc<dyn Fn(PathBuf, Vec<String>, PathBuf, Arc<AtomicBool>, Emit) + Send + Sync>;

pub struct Scheduler {
    sender: mpsc::Sender<Request>,
    snapshot: Arc<Mutex<Snapshot>>,
    worker: Option<JoinHandle<()>>,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 90
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn option<'a>(args: &'a [String], short: &str, long: &str) -> Result<Option<&'a str>, String> {
    let mut found = None;
    for (i, arg) in args.iter().enumerate() {
        let value = if arg == short || arg == long {
            Some(
                args.get(i + 1)
                    .ok_or_else(|| format!("Missing value for {arg}"))?
                    .as_str(),
            )
        } else {
            arg.strip_prefix(&format!("{long}="))
        };
        if let Some(value) = value {
            if found.is_some() || value.is_empty() {
                return Err(format!("Invalid or repeated {long}"));
            }
            found = Some(value);
        }
    }
    Ok(found)
}

fn resources(
    spec: &Spec,
    inventory: Option<&Report>,
) -> Result<(bool, BTreeSet<String>, BTreeSet<String>), String> {
    let inventory = inventory.filter(|r| r.engine == spec.engine);
    let mut exclusive = false;
    let mut devices = BTreeSet::new();
    let mut files = BTreeSet::new();
    for stage in &spec.stages {
        let args = &stage.args;
        engine::read_args(&json!(args))?;
        let session =
            option(args, "--session", "--session")?.ok_or("队列任务需要独立的 session")?;
        if !valid_id(session) {
            return Err("内部任务编号无效，请重新生成计划".into());
        }
        if !files.insert(format!("session:{session}")) {
            return Err("计划内的阶段不能共用 session".into());
        }
        for (short, long) in [
            ("--restore-file-path", "--restore-file-path"),
            ("-o", "--outfile"),
        ] {
            if let Some(path) = option(args, short, long)? {
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err("队列的检查点与输出文件必须使用绝对路径".into());
                }
                let normalized = path
                    .canonicalize()
                    .or_else(|_| {
                        path.parent()
                            .ok_or_else(|| std::io::Error::other("Missing parent"))?
                            .canonicalize()
                            .map(|parent| parent.join(path.file_name().unwrap_or_default()))
                    })
                    .unwrap_or(path);
                let mut key = normalized.to_string_lossy().replace('\\', "/");
                if cfg!(windows) {
                    key = key.to_lowercase();
                }
                files.insert(format!("file:{key}"));
            }
        }
        match option(args, "-d", "--backend-devices")? {
            None => exclusive = true,
            Some(ids) => {
                for id in ids.split(',') {
                    let id: u32 = id.parse().ok().filter(|n| *n > 0).ok_or("设备编号无效")?;
                    if let Some(report) = inventory.filter(|r| !r.devices.is_empty()) {
                        let device = report
                            .devices
                            .iter()
                            .find(|d| d.id == id)
                            .ok_or("所选设备未在最近检测中出现，请重新检测")?;
                        devices.insert(device.physical_key.clone());
                    } else {
                        exclusive = true;
                    }
                }
            }
        }
        if args
            .iter()
            .any(|a| a.starts_with("-d") && a != "-d" && !a.starts_with("--"))
        {
            exclusive = true;
        }
        if let Some(mode) = option(args, "-m", "--hash-type")?
            && [
                "70000", "70100", "70200", "73000", "74000", "75000", "79000",
            ]
            .contains(&mode)
        {
            devices.insert("cpu".into());
        }
    }
    Ok((exclusive, devices, files))
}
fn conflict(a: &Job, b: &Job) -> bool {
    a.exclusive
        || b.exclusive
        || !a.device_keys.is_disjoint(&b.device_keys)
        || !a.file_keys.is_disjoint(&b.file_keys)
}
fn stage_outcome(event: &Value) -> &'static str {
    if event["type"] == "error" {
        "error"
    } else if event["stopped"] == true {
        "stopped"
    } else {
        match event["code"].as_i64() {
            Some(0) => "found",
            Some(1) => "exhausted",
            Some(4) => "limited",
            _ => "error",
        }
    }
}

impl Scheduler {
    pub fn new(
        data: PathBuf,
        gate: Arc<AtomicBool>,
        emit: impl Fn(Value) + Send + Sync + 'static,
    ) -> Self {
        Self::with_runner(
            data,
            gate,
            Arc::new(emit),
            Arc::new(|exe, args, data, cancel, emit| {
                engine::run(exe, args, data, cancel, |v| emit(v))
            }),
        )
    }
    fn with_runner(data: PathBuf, gate: Arc<AtomicBool>, emit: Emit, runner: Runner) -> Self {
        let path = data.join("task-queue.json");
        let mut initial: Snapshot = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        initial.max_parallel = initial.max_parallel.clamp(1, 4);
        initial.active_count = 0;
        for job in &mut initial.jobs {
            if ["queued", "running"].contains(&job.state.as_str()) {
                job.state = "interrupted".into();
                job.wait_reason = "重启后未自动开始，请载入计划或从检查点继续".into();
                for stage in &mut job.stages {
                    if stage.state == "running" {
                        stage.state = "interrupted".into();
                    }
                }
            }
        }
        let snapshot = Arc::new(Mutex::new(initial.clone()));
        let shared = snapshot.clone();
        let (sender, receiver) = mpsc::channel();
        let tx = sender.clone();
        let worker = std::thread::spawn(move || {
            let mut state = initial;
            let mut inventory: Option<Report> = None;
            let mut active: HashMap<String, Arc<AtomicBool>> = HashMap::new();
            let mut owns_gate = false;
            let mut shutdown = false;
            let publish = |state: &Snapshot| {
                *shared.lock().unwrap() = state.clone();
                if let Err(message) = settings::write_json(&path, &json!(state)) {
                    emit(json!({"type":"notice","message":message}));
                }
                emit(json!({"type":"queue_state","queue":state}));
            };
            loop {
                let mut changed = false;
                if let Ok(request) = receiver.recv_timeout(Duration::from_millis(40)) {
                    match request {
                        Request::Submit(spec) => {
                            let submitted_id = spec.id.clone();
                            let result = (|| -> Result<Job, String> {
                                if !valid_id(&spec.id)
                                    || spec.name.len() > 512
                                    || !(1..=100).contains(&spec.stages.len())
                                {
                                    return Err("无效的队列任务或阶段数量".into());
                                }
                                if !crate::platform::executable(&spec.engine) {
                                    return Err("未找到引擎文件".into());
                                }
                                if state.jobs.iter().any(|j| j.spec.id == spec.id) {
                                    return Err("该任务已在队列中".into());
                                }
                                let (exclusive, device_keys, file_keys) =
                                    resources(&spec, inventory.as_ref())?;
                                Ok(Job {
                                    stages: spec
                                        .stages
                                        .iter()
                                        .map(|_| StageState {
                                            state: "pending".into(),
                                            ..StageState::default()
                                        })
                                        .collect(),
                                    spec,
                                    state: "queued".into(),
                                    exclusive,
                                    device_keys,
                                    file_keys,
                                    queued_at: diagnostics::now(),
                                    finished_at: None,
                                    wait_reason: String::new(),
                                })
                            })();
                            match result {
                                Ok(job) => {
                                    if state.jobs.len() >= 100 {
                                        if let Some(i) = state.jobs.iter().position(|j| {
                                            !["running", "queued"].contains(&j.state.as_str())
                                        }) {
                                            state.jobs.remove(i);
                                        } else {
                                            emit(
                                                json!({"type":"queue_error","id":job.spec.id,"message":"队列最多保留 100 个任务"}),
                                            );
                                            continue;
                                        }
                                    }
                                    state.jobs.push(job);
                                    changed = true;
                                }
                                Err(message) => emit(
                                    json!({"type":"queue_error","id":submitted_id,"message":message}),
                                ),
                            }
                        }
                        Request::Cancel(id) => {
                            if let Some(cancel) = active.get(&id) {
                                cancel.store(true, Ordering::SeqCst);
                            }
                            if let Some(job) = state
                                .jobs
                                .iter_mut()
                                .find(|j| j.spec.id == id && j.state == "queued")
                            {
                                job.state = "stopped".into();
                                job.finished_at = Some(diagnostics::now());
                            }
                            changed = true;
                        }
                        Request::CancelAll | Request::Shutdown => {
                            shutdown = matches!(request, Request::Shutdown);
                            for cancel in active.values() {
                                cancel.store(true, Ordering::SeqCst);
                            }
                            for job in &mut state.jobs {
                                if job.state == "queued" {
                                    job.state = "stopped".into();
                                    job.finished_at = Some(diagnostics::now());
                                }
                            }
                            changed = true;
                        }
                        Request::Pause(paused) => {
                            state.paused = paused;
                            changed = true;
                        }
                        Request::Limit(limit) => {
                            state.max_parallel = limit.clamp(1, 4);
                            changed = true;
                        }
                        Request::Move(id, delta) => {
                            let waiting: Vec<_> = state
                                .jobs
                                .iter()
                                .enumerate()
                                .filter(|(_, j)| j.state == "queued")
                                .map(|(i, _)| i)
                                .collect();
                            if let Some(i) =
                                waiting.iter().position(|&i| state.jobs[i].spec.id == id)
                            {
                                let next = (i as i32 + delta.signum())
                                    .clamp(0, waiting.len().saturating_sub(1) as i32)
                                    as usize;
                                state.jobs.swap(waiting[i], waiting[next]);
                                changed = true;
                            }
                        }
                        Request::Inventory(report) => {
                            if active.is_empty() {
                                inventory = Some(report);
                                for job in &mut state.jobs {
                                    if job.state == "queued"
                                        && let Ok((exclusive, devices, files)) =
                                            resources(&job.spec, inventory.as_ref())
                                    {
                                        job.exclusive = exclusive;
                                        job.device_keys = devices;
                                        job.file_keys = files;
                                    }
                                }
                                changed = true;
                            }
                        }
                        Request::Event(id, index, event) => {
                            if let Some(job) = state.jobs.iter_mut().find(|j| j.spec.id == id)
                                && let Some(stage) = job.stages.get_mut(index)
                            {
                                match event["type"].as_str() {
                                    Some("stage_start") => {
                                        stage.state = "running".into();
                                        stage.started_at = Some(diagnostics::now());
                                        changed = true;
                                    }
                                    Some("started") => {
                                        stage.log_path =
                                            event["logPath"].as_str().map(PathBuf::from);
                                        changed = true;
                                    }
                                    Some("finished" | "error") => {
                                        stage.state = stage_outcome(&event).into();
                                        stage.code = event["code"].as_i64();
                                        stage.seconds = event["seconds"].as_f64();
                                        stage.message =
                                            event["message"].as_str().map(str::to_owned);
                                        changed = true;
                                    }
                                    _ => {}
                                }
                            }
                            emit(json!({"type":"queue_event","id":id,"stage":index,"event":event}));
                        }
                        Request::Done(id, outcome) => {
                            active.remove(&id);
                            if let Some(job) = state.jobs.iter_mut().find(|j| j.spec.id == id) {
                                job.state = outcome;
                                job.finished_at = Some(diagnostics::now());
                                if job.state == "found" {
                                    for s in &mut job.stages {
                                        if s.state == "pending" {
                                            s.state = "skipped".into();
                                        }
                                    }
                                }
                            }
                            changed = true;
                        }
                    }
                }
                if !shutdown && !state.paused {
                    if !owns_gate
                        && active.is_empty()
                        && state.jobs.iter().any(|j| j.state == "queued")
                    {
                        owns_gate = gate
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok();
                    }
                    if owns_gate {
                        for index in 0..state.jobs.len() {
                            if active.len() >= state.max_parallel {
                                break;
                            }
                            if state.jobs[index].state != "queued" {
                                continue;
                            }
                            let blocked = state
                                .jobs
                                .iter()
                                .any(|j| j.state == "running" && conflict(&state.jobs[index], j));
                            if blocked {
                                state.jobs[index].wait_reason = "等待所选设备或输出文件释放".into();
                                if state.jobs[index].exclusive {
                                    break;
                                }
                                continue;
                            }
                            let job = &mut state.jobs[index];
                            job.state = "running".into();
                            job.wait_reason.clear();
                            let cancel = Arc::new(AtomicBool::new(false));
                            active.insert(job.spec.id.clone(), cancel.clone());
                            let spec = job.spec.clone();
                            let tx = tx.clone();
                            let data = data.clone();
                            let run = runner.clone();
                            std::thread::spawn(move || {
                                let mut final_state = "exhausted";
                                let mut limited = false;
                                for (index, stage) in spec.stages.iter().enumerate() {
                                    if cancel.load(Ordering::SeqCst) {
                                        final_state = "stopped";
                                        break;
                                    }
                                    let _ = tx.send(Request::Event(
                                        spec.id.clone(),
                                        index,
                                        json!({"type":"stage_start"}),
                                    ));
                                    let result = Arc::new(Mutex::new(None));
                                    let captured = result.clone();
                                    let events = tx.clone();
                                    let id = spec.id.clone();
                                    run(
                                        spec.engine.clone(),
                                        stage.args.clone(),
                                        data.clone(),
                                        cancel.clone(),
                                        Arc::new(move |event| {
                                            if ["finished", "error"]
                                                .contains(&event["type"].as_str().unwrap_or(""))
                                            {
                                                *captured.lock().unwrap() = Some(event.clone());
                                            }
                                            let _ = events.send(Request::Event(
                                                id.clone(),
                                                index,
                                                event,
                                            ));
                                        }),
                                    );
                                    // Release reservations only after engine::run returns and its
                                    // process/job guards have completed child cleanup.
                                    let result = result.lock().unwrap();
                                    let outcome =
                                        result.as_ref().map(stage_outcome).unwrap_or("error");
                                    if outcome == "limited" {
                                        limited = true;
                                    }
                                    if !["exhausted", "limited"].contains(&outcome) {
                                        final_state = outcome;
                                        break;
                                    }
                                }
                                if final_state == "exhausted" && limited {
                                    final_state = "limited";
                                }
                                let _ = tx.send(Request::Done(spec.id, final_state.into()));
                            });
                            changed = true;
                        }
                    }
                }
                if active.is_empty() && owns_gate {
                    gate.store(false, Ordering::SeqCst);
                    owns_gate = false;
                }
                state.active_count = active.len();
                if changed {
                    publish(&state);
                }
                if shutdown && active.is_empty() {
                    break;
                }
            }
        });
        Self {
            sender,
            snapshot,
            worker: Some(worker),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
    pub fn busy(&self) -> bool {
        self.snapshot
            .lock()
            .unwrap()
            .jobs
            .iter()
            .any(|j| ["queued", "running"].contains(&j.state.as_str()))
    }
    pub fn submit(&self, spec: Spec) {
        let _ = self.sender.send(Request::Submit(spec));
    }
    pub fn cancel(&self, id: String) {
        let _ = self.sender.send(Request::Cancel(id));
    }
    pub fn cancel_all(&self) {
        let _ = self.sender.send(Request::CancelAll);
    }
    pub fn pause(&self, paused: bool) {
        let _ = self.sender.send(Request::Pause(paused));
    }
    pub fn move_job(&self, id: String, delta: i32) {
        let _ = self.sender.send(Request::Move(id, delta));
    }
    pub fn limit(&self, limit: usize) {
        let _ = self.sender.send(Request::Limit(limit));
    }
    pub fn inventory(&self, report: Report) {
        let _ = self.sender.send(Request::Inventory(report));
    }
}
impl Drop for Scheduler {
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
    use crate::diagnostics::Device;
    use std::time::Instant;

    fn folder(name: &str) -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/artifacts/queue-{name}-{}-{}",
            std::process::id(),
            diagnostics::now()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn spec(data: &std::path::Path, id: &str, device: Option<u32>) -> Spec {
        let mut args = vec![
            "--session".into(),
            id.into(),
            "--restore-file-path".into(),
            data.join(format!("{id}.restore"))
                .to_string_lossy()
                .into_owned(),
        ];
        if let Some(device) = device {
            args.extend(["-d".into(), device.to_string()]);
        }
        Spec {
            id: id.into(),
            name: id.into(),
            engine: std::env::current_exe().unwrap(),
            stages: vec![Stage {
                label: "fixture".into(),
                args,
            }],
            context: Value::Null,
        }
    }
    fn wait_until(queue: &Scheduler, predicate: impl Fn(&Snapshot) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if predicate(&queue.snapshot()) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Timed out: {:?}",
                queue.snapshot()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn inventory() -> Report {
        Report {
            engine: std::env::current_exe().unwrap(),
            devices: [(1, "pci:one"), (2, "pci:one"), (3, "pci:two")]
                .map(|(id, key)| Device {
                    id,
                    physical_key: key.into(),
                    name: key.into(),
                    ..Device::default()
                })
                .to_vec(),
            ..Report::default()
        }
    }
    #[test]
    fn different_gpus_overlap_aliases_wait_and_cancellation_is_per_job() {
        let data = folder("devices");
        let gate = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let runner: Runner = Arc::new(move |_, args, _, cancel, emit| {
            let id = option(&args, "--session", "--session")
                .unwrap()
                .unwrap()
                .to_owned();
            sender.send(id).unwrap();
            while !cancel.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            emit(json!({"type":"finished","code":2,"stopped":true,"seconds":0.1}));
        });
        let queue = Scheduler::with_runner(data.clone(), gate.clone(), Arc::new(|_| {}), runner);
        queue.pause(true);
        wait_until(&queue, |s| s.paused);
        queue.inventory(inventory());
        queue.submit(spec(&data, "first", Some(1)));
        queue.submit(spec(&data, "alias", Some(2)));
        queue.submit(spec(&data, "other", Some(3)));
        wait_until(&queue, |s| s.jobs.len() == 3);
        queue.pause(false);
        let mut starts = vec![
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
        ];
        starts.sort();
        assert_eq!(starts, ["first", "other"]);
        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        queue.cancel("first".into());
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            "alias"
        );
        wait_until(&queue, |s| {
            s.jobs
                .iter()
                .any(|j| j.spec.id == "first" && j.state == "stopped")
        });
        assert_eq!(
            queue
                .snapshot()
                .jobs
                .iter()
                .find(|j| j.spec.id == "other")
                .unwrap()
                .state,
            "running"
        );
        queue.cancel_all();
        wait_until(&queue, |s| {
            s.active_count == 0 && s.jobs.iter().all(|j| j.state == "stopped")
        });
        assert!(!gate.load(Ordering::SeqCst));
    }
    #[test]
    fn pause_reorder_and_success_skip_later_stages() {
        let data = folder("order");
        let (sender, receiver) = mpsc::channel();
        let runner: Runner = Arc::new(move |_, args, _, _, emit| {
            sender
                .send(
                    option(&args, "--session", "--session")
                        .unwrap()
                        .unwrap()
                        .to_owned(),
                )
                .unwrap();
            emit(json!({"type":"finished","code":0,"stopped":false,"seconds":0.01}));
        });
        let queue = Scheduler::with_runner(
            data.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
            runner,
        );
        queue.pause(true);
        wait_until(&queue, |s| s.paused);
        queue.limit(1);
        let mut a = spec(&data, "a", None);
        let mut later = a.stages[0].clone();
        later.args[1] = "a-later".into();
        a.stages.push(later);
        queue.submit(a);
        queue.submit(spec(&data, "b", None));
        wait_until(&queue, |s| s.jobs.len() == 2);
        queue.move_job("b".into(), -1);
        wait_until(&queue, |s| s.jobs[0].spec.id == "b");
        assert!(receiver.recv_timeout(Duration::from_millis(80)).is_err());
        queue.pause(false);
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), "b");
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), "a");
        wait_until(&queue, |s| {
            s.active_count == 0 && s.jobs.iter().all(|j| j.state == "found")
        });
        assert_eq!(
            queue
                .snapshot()
                .jobs
                .iter()
                .find(|j| j.spec.id == "a")
                .unwrap()
                .stages[1]
                .state,
            "skipped"
        );
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn unknown_devices_serialize_and_legacy_engine_lease_is_respected() {
        let data = folder("gate");
        let gate = Arc::new(AtomicBool::new(true));
        let (sender, receiver) = mpsc::channel();
        let runner: Runner = Arc::new(move |_, args, _, _, emit| {
            sender
                .send(
                    option(&args, "--session", "--session")
                        .unwrap()
                        .unwrap()
                        .to_owned(),
                )
                .unwrap();
            emit(json!({"type":"finished","code":1,"stopped":false}));
        });
        let queue = Scheduler::with_runner(data.clone(), gate.clone(), Arc::new(|_| {}), runner);
        queue.submit(spec(&data, "a", Some(1)));
        queue.submit(spec(&data, "b", Some(3)));
        wait_until(&queue, |s| s.jobs.len() == 2);
        assert!(queue.snapshot().jobs.iter().all(|j| j.exclusive));
        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        gate.store(false, Ordering::SeqCst);
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), "a");
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), "b");
        wait_until(&queue, |s| s.jobs.iter().all(|j| j.state == "exhausted"));
    }
    #[test]
    fn restored_work_never_starts_automatically_and_shared_outputs_conflict() {
        let data = folder("restore");
        let a = spec(&data, "a", Some(1));
        let mut b = spec(&data, "b", Some(3));
        let output = data.join("shared.txt").to_string_lossy().into_owned();
        let mut a = a;
        a.stages[0].args.extend(["-o".into(), output.clone()]);
        b.stages[0].args.extend(["--outfile".into(), output]);
        let make = |spec: Spec| {
            let (exclusive, device_keys, file_keys) = resources(&spec, Some(&inventory())).unwrap();
            Job {
                spec,
                exclusive,
                device_keys,
                file_keys,
                state: "running".into(),
                stages: vec![StageState {
                    state: "running".into(),
                    ..StageState::default()
                }],
                queued_at: 0,
                finished_at: None,
                wait_reason: String::new(),
            }
        };
        let a = make(a);
        let b = make(b);
        assert!(conflict(&a, &b));
        settings::write_json(
            &data.join("task-queue.json"),
            &json!(Snapshot {
                jobs: vec![a, b],
                ..Snapshot::default()
            }),
        )
        .unwrap();
        let queue = Scheduler::with_runner(
            data,
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
            Arc::new(|_, _, _, _, _| panic!("Restored work started")),
        );
        assert!(
            queue
                .snapshot()
                .jobs
                .iter()
                .all(|j| j.state == "interrupted")
        );
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(queue.snapshot().active_count, 0);
    }
}

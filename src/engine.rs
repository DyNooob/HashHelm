use serde_json::{Value, json};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

static RUN_ID: AtomicU64 = AtomicU64::new(0);

pub fn root() -> PathBuf {
    crate::platform::resource_root()
}
pub fn engine_path(root: &Path) -> PathBuf {
    crate::platform::engine_path(root)
}

fn cache_args(args: Vec<String>, data: &Path) -> Result<Vec<String>, String> {
    if args
        .iter()
        .any(|a| a == "--cache-path" || a.starts_with("--cache-path="))
    {
        return Ok(args);
    }
    let cache = data.join("cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut managed = vec![
        "--cache-path".to_owned(),
        cache.to_string_lossy().into_owned(),
    ];
    managed.extend(args);
    Ok(managed)
}

fn prepare_child(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id() as libc::pid_t;
        // Only async-signal-safe syscalls run between fork and exec.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::other(
                        "UI parent exited before engine startup",
                    ));
                }
                Ok(())
            });
        }
    }
}
pub fn read_args(value: &Value) -> Result<Vec<String>, String> {
    let array = value.as_array().ok_or("参数必须为 JSON 字符串数组")?;
    let args: Vec<String> = array
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or("每个参数必须是字符串".to_owned())
        })
        .collect::<Result<_, _>>()?;
    if args.iter().any(|s| s.contains('\0')) {
        return Err("参数包含空字符".into());
    }
    if args.is_empty() {
        return Err("请先配置任务".into());
    }
    if args.iter().any(|a| a == "--restore-position") {
        let restore = args
            .windows(2)
            .find(|w| w[0] == "--restore-file-path")
            .map(|w| w[1].as_str())
            .or_else(|| {
                args.iter()
                    .find_map(|a| a.strip_prefix("--restore-file-path="))
            });
        if !restore.is_some_and(|p| Path::new(p).is_file()) {
            return Err("未找到该任务的检查点；请载入配置后重新开始。继续任务会沿用原策略，不会自动缩小密码范围。".into());
        }
    }
    Ok(args)
}

pub fn read_log(path: &Path, data: &Path) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let data = data.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(data.join("runs")) && path != data.join("latest.log") {
        return Err("只能读取当前工作区的运行日志".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(2 * 1024 * 1024)))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Planning-only operations; never consume a hash target or start a cracking run.
pub fn plan_probe(
    exe: &Path,
    data: &Path,
    args: Vec<String>,
    action: &str,
    password: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    read_args(&json!(args))?;
    let mut value = false;
    for a in &args {
        if value {
            value = false;
            continue;
        }
        if [
            "-m",
            "-a",
            "-1",
            "-2",
            "-3",
            "-4",
            "-5",
            "-6",
            "-7",
            "-8",
            "-r",
            "--increment-min",
            "--increment-max",
            "--hash-recipe",
        ]
        .contains(&a.as_str())
        {
            value = true;
            continue;
        }
        if a.starts_with('-')
            && ![
                "-S",
                "-O",
                "--rules-concat",
                "--increment",
                "--markov-disable",
            ]
            .contains(&a.as_str())
        {
            return Err(format!("范围检查不支持参数：{a}"));
        }
    }
    if value {
        return Err("范围检查参数缺少值".into());
    }
    let flag = match action {
        "total" => "--total-candidates",
        "keyspace" => "--keyspace",
        "lookup" => "--lookup",
        _ => return Err("未知的范围检查方式".into()),
    };
    if action == "lookup"
        && (password.is_empty() || password.len() > 1024 || password.contains('\0'))
    {
        return Err("请填写有效的示例密码（最多 1024 字节）".into());
    }
    let cache = data.join("cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut command = Command::new(exe);
    command
        .current_dir(crate::platform::engine_workdir(exe))
        .args(args)
        .arg(flag);
    if action == "lookup" {
        command.arg(password);
    }
    command
        .args([
            "--restore-disable",
            "--potfile-disable",
            "--logfile-disable",
            "--quiet",
            "--cache-path",
        ])
        .arg(cache)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = query_output(command, cancel, Duration::from_secs(30))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(if text.trim().is_empty() {
            format!("范围检查失败：{}", output.status)
        } else {
            text
        });
    }
    Ok(text)
}

// Closing the UI, including a crash, closes this handle and terminates its child.
#[cfg(windows)]
struct ChildJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ChildJob {
    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
    }
    fn attach(child: &std::process::Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(job, child.as_raw_handle()) == 0
            {
                CloseHandle(job);
                return None;
            }
            Some(Self(job))
        }
    }
}
#[cfg(windows)]
impl Drop for ChildJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

pub fn results(exe: &Path, target: &Path, mode: u32, potfile: &Path) -> Result<Vec<Value>, String> {
    results_with_cancel(exe, target, mode, potfile, &AtomicBool::new(false))
}

pub fn inspect(
    exe: &Path,
    data: &Path,
    cancel: &AtomicBool,
) -> Result<crate::diagnostics::Report, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("设备检测已取消".into());
    }
    let cache = data.join("cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut command = Command::new(exe);
    command
        .current_dir(crate::platform::engine_workdir(exe))
        .args(["-I", "--logfile-disable", "--cache-path"])
        .arg(cache)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = query_output(command, cancel, Duration::from_secs(30))?;
    let raw = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut report = crate::diagnostics::parse(exe.to_owned(), raw);
    if !output.status.success() {
        report.issues.push(crate::diagnostics::Issue {
            code: "engine_exit".into(),
            severity: "error".into(),
            title: "设备检测未正常完成".into(),
            detail: format!("Hashcat: {}", output.status),
        });
    }
    Ok(report)
}

pub fn results_with_cancel(
    exe: &Path,
    target: &Path,
    mode: u32,
    potfile: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<Value>, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("结果读取已取消".into());
    }
    if !target.is_file() {
        return Err("原哈希文件不存在，请重新导入目标文件".into());
    }
    if std::fs::metadata(potfile).map(|m| m.len()).unwrap_or(0) == 0 {
        return Ok(vec![]);
    }
    let cache = potfile
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut command = Command::new(exe);
    command
        .current_dir(crate::platform::engine_workdir(exe))
        .args([
            "-m",
            &mode.to_string(),
            "--show",
            "--quiet",
            "--outfile-json",
            "--logfile-disable",
            "--potfile-path",
        ])
        .arg(potfile)
        .arg("--cache-path")
        .arg(&cache)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = query_output(command, cancel, Duration::from_secs(120))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            format!("结果查询失败：{}", output.status)
        } else {
            message
        });
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).map_err(|e| format!("结果格式无效：{e}")))
        .collect()
}

// Drain both pipes while polling so neither a full pipe nor a stuck query can
// keep the UI waiting forever. Helpers must be terminated before joining readers.
fn query_output(
    command: Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    query_input(command, cancel, timeout, &[])
}

pub fn query_input(
    mut command: Command,
    cancel: &AtomicBool,
    timeout: Duration,
    input: &[u8],
) -> Result<std::process::Output, String> {
    prepare_child(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let job = ChildJob::attach(&child);
    #[cfg(windows)]
    if job.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Unable to attach the engine process watchdog".into());
    }
    #[cfg(unix)]
    let group = match crate::platform::ProcessGroup::attach(&child) {
        Ok(group) => group,
        Err(error) => {
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Cannot start engine watchdog: {error}"));
        }
    };
    let stdout = child.stdout.take().ok_or("Missing query stdout")?;
    let stderr = child.stderr.take().ok_or("Missing query stderr")?;
    let input = input.to_vec();
    let writer = child
        .stdin
        .take()
        .map(|mut stdin| std::thread::spawn(move || stdin.write_all(&input)));
    let read = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 65536];
            let mut overflow = false;
            loop {
                let n = pipe.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                if bytes.len() + n <= 8 * 1024 * 1024 {
                    bytes.extend_from_slice(&buffer[..n]);
                } else {
                    overflow = true;
                }
            }
            if overflow {
                Err(std::io::Error::other("查询输出超过 8 MiB 上限"))
            } else {
                Ok(bytes)
            }
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err("结果读取已取消".to_owned());
        }
        if started.elapsed() >= timeout {
            break Err("结果读取超时，请重试或在完整命令中使用 --show".to_owned());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => break Err(error.to_string()),
        }
    };
    #[cfg(windows)]
    if let Some(job) = &job {
        job.terminate();
    }
    #[cfg(unix)]
    group.kill();
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(writer) = writer {
        let result = writer
            .join()
            .map_err(|_| "Query input writer failed".to_owned())?;
        if status.is_ok() {
            result.map_err(|e| e.to_string())?;
        }
    }
    let stdout = out
        .join()
        .map_err(|_| "Query output reader failed".to_owned())?;
    let stderr = err
        .join()
        .map_err(|_| "Query error reader failed".to_owned())?;
    Ok(std::process::Output {
        status: status?,
        stdout: stdout.map_err(|e| e.to_string())?,
        stderr: stderr.map_err(|e| e.to_string())?,
    })
}

/// No shell is involved: paths, masks and recipes remain literal argv entries.
pub fn run(
    exe: PathBuf,
    args: Vec<String>,
    data: PathBuf,
    cancel: Arc<AtomicBool>,
    emit: impl Fn(Value),
) {
    let start = Instant::now();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let _ = std::fs::create_dir_all(data.join("runs"));
    let log_path = data.join("runs").join(format!(
        "{stamp}-{}-{}.log",
        std::process::id(),
        RUN_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let meta_path = log_path.with_extension("json");
    match crate::sessions::prepare(&exe, &args) {
        Ok(Some(message)) => emit(json!({"type":"lines","lines":[message]})),
        Ok(None) => {}
        Err(message) => {
            emit(json!({"type":"error","message":message}));
            return;
        }
    }
    let args = match cache_args(args, &data) {
        Ok(args) => args,
        Err(message) => {
            emit(json!({"type":"error","message":format!("无法创建运行缓存：{message}")}));
            return;
        }
    };
    let _ = std::fs::write(
        &meta_path,
        json!({"engine":exe,"args":args,"state":"running","logPath":log_path}).to_string(),
    );
    let mut command = Command::new(&exe);
    command
        .args(&args)
        .current_dir(crate::platform::engine_workdir(&exe))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    prepare_child(&mut command);
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::write(&meta_path, json!({"engine":exe,"args":args,"state":"spawn_failed","error":e.to_string(),"logPath":log_path}).to_string());
            emit(json!({"type":"error","message":format!("无法启动 Hashcat：{e}")}));
            return;
        }
    };
    #[cfg(windows)]
    let _job = ChildJob::attach(&child);
    #[cfg(windows)]
    if _job.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        let message = "Unable to attach the engine process watchdog";
        let _=std::fs::write(&meta_path,json!({"engine":exe,"args":args,"state":"watchdog_failed","error":message,"logPath":log_path}).to_string());
        emit(json!({"type":"error","message":message}));
        return;
    }
    #[cfg(unix)]
    let process_group = match crate::platform::ProcessGroup::attach(&child) {
        Ok(group) => group,
        Err(error) => {
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::write(&meta_path,json!({"engine":exe,"args":args,"state":"watchdog_failed","error":error.to_string(),"logPath":log_path}).to_string());
            emit(
                json!({"type":"error","message":format!("Cannot start engine watchdog: {error}")}),
            );
            return;
        }
    };
    emit(json!({"type":"started","pid":child.id(),"logPath":log_path}));
    let (tx, rx) = mpsc::sync_channel::<String>(512);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let t = tx.clone();
    let out = std::thread::spawn(move || {
        for line in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
            if t.send(
                String::from_utf8_lossy(&line)
                    .trim_end_matches('\r')
                    .to_string(),
            )
            .is_err()
            {
                break;
            }
        }
    });
    let err = std::thread::spawn(move || {
        for line in BufReader::new(stderr).split(b'\n').map_while(Result::ok) {
            if tx
                .send(
                    String::from_utf8_lossy(&line)
                        .trim_end_matches('\r')
                        .to_string(),
                )
                .is_err()
            {
                break;
            }
        }
    });
    let mut log = std::fs::File::create(&log_path).ok();
    let mut stopped = false;
    loop {
        if cancel.swap(false, Ordering::SeqCst) {
            #[cfg(windows)]
            if let Some(job) = _job.as_ref() {
                job.terminate();
            }
            #[cfg(unix)]
            process_group.kill();
            let _ = child.kill();
            stopped = true;
        }
        let mut batch = Vec::new();
        for _ in 0..160 {
            match rx.try_recv() {
                Ok(line) => {
                    if let Some(log) = log.as_mut() {
                        let _ = writeln!(log, "{line}");
                    }
                    batch.push(line);
                }
                Err(_) => break,
            }
        }
        if !batch.is_empty() {
            emit(json!({"type":"lines","lines":batch}));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // Helpers can inherit the output pipe. Stop them before EOF draining.
                #[cfg(unix)]
                process_group.kill();
                #[cfg(windows)]
                if let Some(job) = _job.as_ref() {
                    job.terminate();
                }
                // Drain before joining: a verbose child can fill the bounded channel.
                let mut remaining = Vec::new();
                for line in rx.iter() {
                    if let Some(log) = log.as_mut() {
                        let _ = writeln!(log, "{line}");
                    }
                    remaining.push(line);
                    if remaining.len() >= 160 {
                        emit(json!({"type":"lines","lines":remaining}));
                        remaining = Vec::new();
                    }
                }
                let _ = out.join();
                let _ = err.join();
                if let Some(hint) = crate::diagnostics::exit_hint(status.code()) {
                    let line = format!("HashHelm: {hint}");
                    if let Some(log) = log.as_mut() {
                        let _ = writeln!(log, "{line}");
                    }
                    remaining.push(line);
                }
                if !remaining.is_empty() {
                    emit(json!({"type":"lines","lines":remaining}));
                }
                let _ = std::fs::copy(&log_path, data.join("latest.log"));
                let _ = std::fs::write(&meta_path, json!({"engine":exe,"args":args,"state":"finished","code":status.code(),"stopped":stopped,"seconds":start.elapsed().as_secs_f64(),"logPath":log_path}).to_string());
                emit(
                    json!({"type":"finished","code":status.code(),"stopped":stopped,"seconds":start.elapsed().as_secs_f64(),"logPath":log_path}),
                );
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(80)),
            Err(e) => {
                #[cfg(windows)]
                if let Some(job) = _job.as_ref() {
                    job.terminate();
                }
                #[cfg(unix)]
                process_group.kill();
                let _ = child.kill();
                let _ = child.wait();
                emit(json!({"type":"error","message":e.to_string()}));
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_cache_defaults_to_workspace_and_preserves_explicit_paths() {
        let data = root().join("tests/artifacts/cache-default");
        let input = vec!["-hh".to_owned(), "$(whoami)".to_owned()];
        let args = cache_args(input.clone(), &data).unwrap();
        assert_eq!(
            &args[..2],
            &["--cache-path", data.join("cache").to_str().unwrap()]
        );
        assert_eq!(&args[2..], input);
        assert!(data.join("cache").is_dir());
        for explicit in [
            vec!["--cache-path".to_owned(), "/custom cache".to_owned()],
            vec!["--cache-path=/custom cache".to_owned()],
        ] {
            assert_eq!(cache_args(explicit.clone(), &data).unwrap(), explicit);
        }
    }

    #[test]
    fn logs_are_bounded_and_confined_to_workspace() {
        let data = root().join("tests/artifacts/log-reader");
        std::fs::create_dir_all(data.join("runs")).unwrap();
        let path = data.join("runs/large.log");
        let mut bytes = vec![b'a'; 2 * 1024 * 1024 + 32];
        bytes.extend_from_slice(b"END-OF-LOG");
        std::fs::write(&path, bytes).unwrap();
        let tail = read_log(&path, &data).unwrap();
        assert_eq!(tail.len(), 2 * 1024 * 1024);
        assert!(tail.ends_with("END-OF-LOG"));
        let unrelated = data.join("source.txt");
        std::fs::write(&unrelated, "source file must not be exposed as a log").unwrap();
        assert!(read_log(&unrelated, &data).is_err());
    }
    #[test]
    fn preserves_literal_arguments() {
        let args = json!([
            "-m",
            "4000",
            "--hash-recipe",
            "md5(md5(salt) . pass)",
            "D:\\有 空格\\hash.txt",
            "?d?w!",
            "$(whoami)"
        ]);
        assert_eq!(read_args(&args).unwrap()[6], "$(whoami)");
        assert_eq!(read_args(&args).unwrap()[4], "D:\\有 空格\\hash.txt");
    }
    #[test]
    fn rejects_invalid_arguments() {
        assert!(read_args(&json!(["x", 2])).is_err());
        assert!(read_args(&json!(["a\u{0}b"])).is_err());
        assert!(read_args(&json!([])).is_err());
        assert!(
            read_args(&json!([
                "--restore-position",
                "--restore-file-path",
                "missing-task.restore"
            ]))
            .is_err()
        );
    }

    #[test]
    fn result_query_is_explicit_and_preserves_run_log() {
        let root = root();
        let data = root.join("tests/artifacts/result-query");
        std::fs::create_dir_all(&data).unwrap();
        let target = data.join("md5.hash");
        let pot = data.join("results.potfile");
        let log = data.join("latest.log");
        std::fs::write(&target, "8743b52063cd84097a65d1633f5c74f5\n").unwrap();
        std::fs::write(&pot, "").unwrap();
        std::fs::write(&log, "previous run must survive").unwrap();
        assert!(
            results(&engine_path(&root), &target, 0, &pot)
                .unwrap()
                .is_empty()
        );
        std::fs::write(&pot, "8743b52063cd84097a65d1633f5c74f5:hashcat\n").unwrap();
        assert_eq!(
            results(&engine_path(&root), &target, 0, &pot).unwrap(),
            vec![
                json!({"hash_hex":"3837343362353230363363643834303937613635643136333366356337346635","password_hex":"68617368636174"})
            ]
        );
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "previous run must survive"
        );
    }

    #[test]
    #[ignore = "child process fixture only"]
    fn sleeping_child_fixture() {
        std::thread::sleep(Duration::from_secs(10));
    }

    fn slow_query() -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "engine::tests::sleeping_child_fixture",
                "--ignored",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    #[test]
    fn result_query_timeout_reaps_child() {
        let started = Instant::now();
        let error = query_output(
            slow_query(),
            &AtomicBool::new(false),
            Duration::from_millis(150),
        )
        .unwrap_err();
        assert!(error.contains("超时"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn result_query_can_be_cancelled_while_reading() {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let setter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            signal.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let error = query_output(slow_query(), &cancel, Duration::from_secs(10)).unwrap_err();
        setter.join().unwrap();
        assert_eq!(error, "结果读取已取消");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(windows)]
    #[test]
    fn closing_job_terminates_child() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "engine::tests::sleeping_child_fixture",
                "--ignored",
            ])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let job = ChildJob::attach(&child);
        if job.is_none() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Unable to attach child job")
        }
        drop(job);
        let deadline = Instant::now() + Duration::from_secs(3);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Child survived job close")
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn real_engine_help_and_cancel() {
        let data = root().join("tests/artifacts/engine");
        std::fs::create_dir_all(&data).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        // The output is larger than the bounded channel: this checks EOF draining.
        let captured = std::sync::Mutex::new(&mut events);
        run(
            engine_path(&root()),
            vec!["-hh".into()],
            data.clone(),
            cancel.clone(),
            |v| {
                captured.lock().unwrap().push(v);
            },
        );
        assert!(
            events
                .iter()
                .any(|v| v["type"] == "finished" && v["code"] == 0)
        );
        let count: usize = events
            .iter()
            .filter_map(|v| v["lines"].as_array().map(Vec::len))
            .sum();
        assert!(count > 512);
        let finished = std::sync::Mutex::new(false);
        run(
            engine_path(&root()),
            vec!["-hh".into()],
            data,
            cancel.clone(),
            |v| {
                if v["type"] == "started" {
                    cancel.store(true, Ordering::SeqCst);
                }
                if v["type"] == "finished" {
                    *finished.lock().unwrap() = true;
                    assert_eq!(v["stopped"], true);
                }
            },
        );
        assert!(*finished.lock().unwrap());
    }
}

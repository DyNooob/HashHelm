use serde_json::{Value, json};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub fn root() -> PathBuf {
    crate::platform::resource_root()
}
pub fn engine_path(root: &Path) -> PathBuf {
    crate::platform::engine_path(root)
}

fn cache_args(args: Vec<String>, data: &Path) -> Result<Vec<String>, String> {
    if args.iter().any(|a| a == "--cache-path" || a.starts_with("--cache-path=")) {
        return Ok(args);
    }
    let cache = data.join("cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut managed = vec!["--cache-path".to_owned(), cache.to_string_lossy().into_owned()];
    managed.extend(args);
    Ok(managed)
}

fn prepare_child(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    #[cfg(unix)] {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(target_os = "linux")] {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id() as libc::pid_t;
        // Only async-signal-safe syscalls run between fork and exec.
        unsafe { command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 { return Err(std::io::Error::last_os_error()); }
            if libc::getppid() != parent { return Err(std::io::Error::other("UI parent exited before engine startup")); }
            Ok(())
        }); }
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

// Closing the UI, including a crash, closes this handle and terminates its child.
#[cfg(windows)]
struct ChildJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ChildJob {
    fn terminate(&self) {
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1); }
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
    if !target.is_file() {
        return Err("原哈希文件不存在，请重新导入目标文件".into());
    }
    if std::fs::metadata(potfile).map(|m| m.len()).unwrap_or(0) == 0 {
        return Ok(vec![]);
    }
    let mut command = Command::new(exe);
    let args = cache_args(Vec::new(), potfile.parent().unwrap_or(Path::new(".")))?;
    command
        .current_dir(crate::platform::engine_workdir(exe))
        .args(args)
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
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    prepare_child(&mut command);
    let child = command.spawn().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let _job = ChildJob::attach(&child);
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).map_err(|e| format!("结果格式无效：{e}")))
        .collect()
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
    let args = match cache_args(args, &data) {
        Ok(args) => args,
        Err(message) => {
            emit(json!({"type":"error","message":format!("无法创建引擎缓存：{message}")}));
            return;
        }
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let _ = std::fs::create_dir_all(data.join("runs"));
    let log_path = data.join("runs").join(format!("{stamp}.log"));
    let meta_path = log_path.with_extension("json");
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
    #[cfg(unix)]
    let process_group = match crate::platform::ProcessGroup::attach(&child) {
        Ok(group) => group,
        Err(error) => {
            unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL); }
            let _ = child.kill(); let _ = child.wait();
            let _ = std::fs::write(&meta_path,json!({"engine":exe,"args":args,"state":"watchdog_failed","error":error.to_string(),"logPath":log_path}).to_string());
            emit(json!({"type":"error","message":format!("Cannot start engine watchdog: {error}")}));
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
            if let Some(job) = _job.as_ref() { job.terminate(); }
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
                if let Some(job) = _job.as_ref() { job.terminate(); }
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
                if let Some(job) = _job.as_ref() { job.terminate(); }
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
        assert_eq!(&args[..2], &["--cache-path", data.join("cache").to_str().unwrap()]);
        assert_eq!(&args[2..], input);
        assert!(data.join("cache").is_dir());
        for explicit in [vec!["--cache-path".to_owned(), "/custom cache".to_owned()], vec!["--cache-path=/custom cache".to_owned()]] {
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

    #[cfg(windows)]
    #[test]
    #[ignore = "child process fixture only"]
    fn sleeping_child_fixture() {
        std::thread::sleep(Duration::from_secs(10));
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

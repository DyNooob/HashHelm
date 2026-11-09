//! Integration harness; not included in release packages. No GUI dependencies.
use hashhelm::{engine, platform};
use std::{path::PathBuf, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(code) = platform::guardian_entry() { std::process::exit(code); }
    #[cfg(unix)]
    if std::env::args().nth(1).as_deref() == Some("--watchdog-owner") {
        use std::os::unix::process::CommandExt;
        let path = std::env::args_os().nth(2).ok_or("Missing watchdog fixture path")?;
        let child = std::process::Command::new("/bin/sh").args(["-c", "sleep 30 & wait"]).process_group(0).spawn()?;
        let _group = platform::ProcessGroup::attach(&child)?;
        std::fs::write(path,child.id().to_string())?;
        // Deliberately skip Drop, as a crashed UI would. The pipe must do cleanup.
        std::process::exit(0);
    }
    let root = engine::root();
    let exe = std::env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(|| engine::engine_path(&root));
    let data = root.join("tests/artifacts/runtime-check");
    std::fs::create_dir_all(data.join("cache"))?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let events = Mutex::new(Vec::new());
    engine::run(exe.clone(), vec!["-hh".into()], data.clone(), cancelled.clone(), |e| {events.lock().unwrap().push(e);});
    let events = events.into_inner().unwrap();
    assert!(events.iter().any(|v|v["type"]=="finished" && v["code"]==0), "Engine help did not complete: {events:?}");
    assert!(events.iter().filter_map(|v|v["lines"].as_array().map(Vec::len)).sum::<usize>()>512, "Large output was not drained");
    let stopped = Mutex::new(false);
    engine::run(exe.clone(), vec!["-hh".into()], data.clone(), cancelled.clone(), |e| {
        if e["type"]=="started" {cancelled.store(true, Ordering::SeqCst);}
        if e["type"]=="finished" {*stopped.lock().unwrap()=e["stopped"]==true;}
    });
    assert!(*stopped.lock().unwrap(), "Cancellation was not reported");
    let target = data.join("md5.hash"); let pot = data.join("results.potfile");
    std::fs::write(&target,"8743b52063cd84097a65d1633f5c74f5\n")?;
    std::fs::write(&pot,"8743b52063cd84097a65d1633f5c74f5:hashcat\n")?;
    let result = engine::results(&exe,&target,0,&pot)?;
    assert_eq!(result[0]["password_hex"], "68617368636174");
    #[cfg(unix)] { check_process_group()?; check_crashed_owner(&data)?; }
    println!("Native runtime: engine output, cancellation, literal JSON results and process cleanup passed.");
    Ok(())
}

#[cfg(unix)]
fn check_crashed_owner(data: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::{process::{Command, Stdio}, time::{Duration, Instant}};
    let pid_file=data.join("crashed-owner.pid");
    let owner=Command::new(std::env::current_exe()?).arg("--watchdog-owner").arg(&pid_file).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    assert!(owner.success());
    let pid:i32=std::fs::read_to_string(pid_file)?.parse()?;
    let deadline=Instant::now()+Duration::from_secs(4);
    loop {
        if unsafe {libc::kill(pid,0)} == -1 {return Ok(());}
        // In a container, PID 1 can defer reaping a dead grandchild. A zombie
        // has already stopped; it cannot retain GPU work or child execution.
        let state=Command::new("ps").args(["-o","stat=","-p",&pid.to_string()]).output()?;
        if String::from_utf8_lossy(&state.stdout).trim_start().starts_with('Z') {return Ok(());}
        if Instant::now()>deadline {unsafe{libc::kill(-pid,libc::SIGKILL);}return Err("Watchdog did not terminate the crashed UI's process group".into());}
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
fn check_process_group() -> Result<(), Box<dyn std::error::Error>> {
    use std::{os::unix::process::CommandExt, process::{Command, Stdio}, time::{Duration, Instant}};
    let mut child=Command::new("/bin/sh").args(["-c","sleep 30 & wait"]).process_group(0).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    let group=platform::ProcessGroup::attach(&child)?;
    drop(group);
    let deadline=Instant::now()+Duration::from_secs(3);
    while child.try_wait()?.is_none() {if Instant::now()>deadline {let _=child.kill();return Err("Engine process group survived watchdog drop".into());}std::thread::sleep(Duration::from_millis(20));}
    Ok(())
}

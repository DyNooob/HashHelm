//! Read checkpoint positions, never commands stored in a Hashcat restore file.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path, time::UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Input {
    path: String,
    bytes: u64,
    modified: String,
}
#[derive(Serialize, Deserialize)]
struct Record {
    schema: u32,
    args: Vec<String>,
    inputs: Vec<Input>,
}
fn stamp(path: &Path) -> Result<Input, String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{}: 输入不再是文件", path.display()));
    }
    Ok(Input {
        path: path.to_string_lossy().into_owned(),
        bytes: meta.len(),
        modified: meta
            .modified()
            .map_err(|e| e.to_string())?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string(),
    })
}
pub fn restore_path(args: &[String]) -> Option<&str> {
    args.windows(2)
        .find(|a| a[0] == "--restore-file-path")
        .map(|a| a[1].as_str())
        .or_else(|| {
            args.iter()
                .find_map(|a| a.strip_prefix("--restore-file-path="))
        })
}
fn signature(args: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let name = a.split('=').next().unwrap_or(a);
        if ["--runtime", "-d", "--backend-devices", "--status-timer"].contains(&name) {
            i += if a.contains('=') { 1 } else { 2 };
            continue;
        }
        if a != "--restore-position" {
            out.push(a.clone());
        }
        i += 1;
    }
    out
}
fn read_record(path: &Path) -> Result<Record, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = vec![];
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("会话记录过大".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("会话记录损坏：{e}"))
}
pub fn inspect(path: &Path) -> Result<Value, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("检查点不是普通文件".into());
    }
    // Pinned Hashcat 8, 64-bit little-endian targets: native restore_data_t is 296 bytes.
    let mut header = [0u8; 296];
    file.read_exact(&mut header)
        .map_err(|_| "检查点文件不完整")?;
    let u32_at = |i| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
    if u32_at(0) != 720 {
        return Err("检查点版本不兼容，请使用原 Hashcat 版本继续或重新生成计划".into());
    }
    let cwd = &header[4..260];
    let end = cwd
        .iter()
        .position(|b| *b == 0)
        .ok_or("检查点工作目录无效")?;
    let cwd = std::str::from_utf8(&cwd[..end]).map_err(|_| "检查点目录编码无效")?;
    let working_directory_exists = Path::new(cwd).is_dir();
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    let sidecar = path.with_extension("restore.hashhelm.json");
    let tracked = if sidecar.exists() {
        Some(read_record(&sidecar)?.inputs.len())
    } else {
        None
    };
    Ok(
        json!({"version":720,"position":u64::from_le_bytes(header[272..280].try_into().unwrap()).to_string(),"dictionaryIndex":u32_at(260),"maskIndex":u32_at(264),"modified":modified,"workingDirectoryExists":working_directory_exists,"trackedInputs":tracked}),
    )
}
/// Metadata protects against accidentally resuming with an edited/replaced dictionary.
/// This is a size/mtime check, not a cryptographic content-integrity assertion.
pub fn prepare(exe: &Path, args: &[String]) -> Result<Option<String>, String> {
    let Some(path) = restore_path(args) else {
        return Ok(None);
    };
    if args.iter().any(|a| {
        [
            "--restore-disable",
            "--restore",
            "--show",
            "--left",
            "--stdout",
            "--keyspace",
            "--total-candidates",
            "--lookup",
        ]
        .contains(&a.as_str())
    }) {
        return Ok(None);
    }
    let path = Path::new(path);
    let sidecar = path.with_extension("restore.hashhelm.json");
    if args.iter().any(|a| a == "--restore-position") {
        let info = inspect(path)?;
        if info["workingDirectoryExists"] != true {
            return Err("检查点原工作目录已不存在，请恢复原目录或重新生成计划".into());
        }
        if !sidecar.exists() {
            return Ok(Some(
                "旧任务没有输入校验记录；请确认字典、规则和目标文件未更改。".into(),
            ));
        }
        let record = read_record(&sidecar)?;
        if record.schema != 1 || record.args != signature(args) {
            return Err(
                "续跑参数与原任务不一致，请重新生成计划。续跑仅允许调整时间上限与设备。".into(),
            );
        }
        for input in record.inputs {
            if stamp(Path::new(&input.path))? != input {
                return Err(format!("输入文件已更改，不能沿用旧位置：{}", input.path));
            }
        }
        return Ok(None);
    }
    let mut inputs = vec![];
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        let name = arg.split('=').next().unwrap_or(arg);
        if [
            "--restore-file-path",
            "--potfile-path",
            "--cache-path",
            "--outfile",
            "-o",
            "--debug-file",
            "--induction-dir",
        ]
        .contains(&name)
        {
            skip = !arg.contains('=');
            continue;
        }
        let arg = arg.strip_prefix("hintfile=").unwrap_or(arg);
        let p = Path::new(arg);
        let p = if p.is_absolute() {
            p.to_owned()
        } else {
            crate::platform::engine_workdir(exe).join(p)
        };
        if p.is_file() {
            let input = stamp(&p)?;
            if !inputs.contains(&input) {
                inputs.push(input);
            }
        }
    }
    for (i, arg) in args.iter().enumerate() {
        let files = arg.strip_prefix("--veracrypt-keyfiles=").or_else(|| {
            if arg == "--veracrypt-keyfiles" {
                args.get(i + 1).map(String::as_str)
            } else {
                None
            }
        });
        if let Some(files) = files {
            for file in files.split(',') {
                let p = Path::new(file);
                let p = if p.is_absolute() {
                    p.to_owned()
                } else {
                    crate::platform::engine_workdir(exe).join(p)
                };
                let input = stamp(&p)?;
                if !inputs.contains(&input) {
                    inputs.push(input);
                }
            }
        }
    }
    let record = Record {
        schema: 1,
        args: signature(args),
        inputs,
    };
    crate::settings::write_json(&sidecar, &json!(record))?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn position_is_exact_and_untrusted_commands_are_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("case.restore");
        let mut bytes = vec![0; 296];
        bytes[..4].copy_from_slice(&720u32.to_le_bytes());
        bytes[4..5].copy_from_slice(b".");
        bytes[272..280].copy_from_slice(&u64::MAX.to_le_bytes());
        bytes.extend_from_slice(b"malicious command must never execute");
        fs::write(&path, &bytes).unwrap();
        assert_eq!(inspect(&path).unwrap()["position"], u64::MAX.to_string());
        bytes[..4].copy_from_slice(&600u32.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        assert!(inspect(&path).is_err());
        fs::write(&path, [0; 20]).unwrap();
        assert!(inspect(&path).is_err());
    }
    #[test]
    fn resume_allows_runtime_devices_but_rejects_input_or_strategy_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("case.restore");
        let dict = dir.path().join("words.txt");
        fs::write(&dict, "public test words").unwrap();
        let mut bytes = vec![0; 296];
        bytes[..4].copy_from_slice(&720u32.to_le_bytes());
        let cwd = dir.path().to_str().unwrap().as_bytes();
        bytes[4..4 + cwd.len()].copy_from_slice(cwd);
        fs::write(&path, bytes).unwrap();
        let exe = dir.path().join("hashcat");
        let mut args = vec![
            "--restore-file-path".into(),
            path.to_str().unwrap().into(),
            "--runtime".into(),
            "5".into(),
            "-d".into(),
            "1".into(),
            dict.to_str().unwrap().into(),
        ];
        prepare(&exe, &args).unwrap();
        args[3] = "60".into();
        args[5] = "2".into();
        args.push("--restore-position".into());
        assert!(prepare(&exe, &args).unwrap().is_none());
        args.push("--markov-disable".into());
        assert!(prepare(&exe, &args).is_err());
        args.pop();
        fs::write(&dict, "changed length and content").unwrap();
        assert!(prepare(&exe, &args).is_err());
    }
}

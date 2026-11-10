//! Bounded, cancellable input statistics; these are not unique-password counts.
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::{BufRead, BufReader, Read},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
static CACHE: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();

fn count(path: &str, rules: bool, cancel: &AtomicBool, started: Instant) -> Value {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) => return json!({"count":null,"reason":e.to_string()}),
    };
    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(e) => return json!({"count":null,"reason":e.to_string()}),
    };
    if !metadata.is_file() {
        return json!({"count":null,"reason":"目录或特殊输入的数量由引擎确定"});
    }
    let key = format!(
        "{path}:{rules}:{}:{:?}",
        metadata.len(),
        metadata.modified().ok()
    );
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(result) = cache.lock().unwrap().get(&key) {
        return result.clone();
    }
    if metadata.len() > MAX_BYTES {
        return json!({"count":null,"bytes":metadata.len(),"reason":"文件超过 64 MiB，跳过扫描以保持响应"});
    }
    let mut reader = BufReader::new(file.take(MAX_BYTES + 1));
    let mut line = Vec::new();
    let mut lines = 0u64;
    let mut bytes = 0u64;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return json!({"count":null,"reason":"工作量分析已取消"});
        }
        if started.elapsed().as_secs() >= 10 {
            return json!({"count":null,"reason":"统计超过 10 秒，实际数量由引擎确定"});
        }
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(n) => bytes += n as u64,
            Err(e) => return json!({"count":null,"reason":e.to_string()}),
        }
        if bytes > MAX_BYTES {
            return json!({"count":null,"reason":"文件在读取时增长，已停止统计"});
        }
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if !(line.is_empty() || rules && line[0] == b'#') {
            lines += 1;
        }
    }
    let result = json!({"count":lines.to_string(),"bytes":bytes,"basis":if rules {"rule-lines"} else {"nonempty-lines"}});
    let mut entries = cache.lock().unwrap();
    if entries.len() >= 256 {
        entries.clear();
    }
    entries.insert(key, result.clone());
    result
}

pub fn analyze(stages: &[Value], cancel: &AtomicBool) -> Result<Value, String> {
    if stages.len() > 100 {
        return Err("一个计划最多 100 个阶段".into());
    }
    let started = Instant::now();
    let mut paths = BTreeSet::new();
    for stage in stages {
        let c = &stage["config"];
        for name in ["wordlist", "wordlist2"] {
            if let Some(path) = c[name].as_str().filter(|s| !s.is_empty()) {
                paths.insert((path.to_owned(), false));
            }
        }
        for path in c["rules"]
            .as_str()
            .unwrap_or("")
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            paths.insert((path.to_owned(), true));
        }
    }
    if paths.len() > 128 {
        return Err("一次最多分析 128 个不同输入文件".into());
    }
    let mut words = serde_json::Map::new();
    let mut rules = serde_json::Map::new();
    for (path, is_rule) in paths {
        if cancel.load(Ordering::SeqCst) {
            return Err("工作量分析已取消".into());
        }
        let value = count(&path, is_rule, cancel, started);
        if is_rule {
            rules.insert(path, value);
        } else {
            words.insert(path, value);
        }
    }
    Ok(json!({"words":words,"rules":rules,"scannedAt":crate::diagnostics::now()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    #[test]
    fn counts_preserve_literal_words_and_invalidate_on_change() {
        let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/artifacts/preflight");
        fs::create_dir_all(&data).unwrap();
        let path = data.join("words.txt");
        fs::write(&path, b"#literal password\r\nabc \n\n").unwrap();
        let cancel = AtomicBool::new(false);
        let p = path.to_str().unwrap();
        assert_eq!(count(p, false, &cancel, Instant::now())["count"], "2");
        assert_eq!(count(p, true, &cancel, Instant::now())["count"], "1");
        fs::write(&path, "new\n").unwrap();
        assert_eq!(count(p, false, &cancel, Instant::now())["count"], "1");
        cancel.store(true, Ordering::SeqCst);
        assert!(analyze(&[json!({"config":{"wordlist":p}})], &cancel).is_err());
    }
}

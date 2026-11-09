//! Deterministic, bounded, offline candidate generation from user-supplied hints.
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub names: String,
    pub birthdays: String,
    pub words: String,
    pub suffixes: String,
    pub separators: String,
    pub limit: usize,
    pub min_length: usize,
    pub max_length: usize,
    pub extended: bool,
    pub leet: bool,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            names: String::new(),
            birthdays: String::new(),
            words: String::new(),
            suffixes: "!,@,#,123,1234,520,1314".into(),
            separators: String::new(),
            limit: 20000,
            min_length: 1,
            max_length: 32,
            extended: false,
            leet: false,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
    pub label: String,
    pub count: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generation {
    #[serde(skip)]
    pub words: Vec<String>,
    pub count: usize,
    pub truncated: bool,
    pub tiers: Vec<Tier>,
    pub preview: Vec<String>,
    pub path: Option<PathBuf>,
}
fn tokens(s: &str, max: usize) -> Result<Vec<String>, String> {
    if s.len() > 8192 {
        return Err("输入过长，请精简到相关线索".into());
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for raw in s.split([',', '，', ';', '；', '\n', '\r']) {
        let v = raw.trim();
        if v.is_empty() {
            continue;
        }
        if v.chars().count() > 64 || v.chars().any(char::is_control) {
            return Err("每条线索最多 64 个字符，不能包含控制字符".into());
        }
        if seen.insert(v.to_owned()) {
            result.push(v.to_owned());
        }
    }
    if result.len() > max {
        return Err(format!("该字段最多允许 {max} 条线索，请减少无关项"));
    }
    Ok(result)
}
fn unique(out: &mut Vec<String>, s: String) {
    if !s.is_empty() && !out.contains(&s) {
        out.push(s)
    }
}
fn title(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}
pub fn transliterate(s: &str) -> (String, String, String) {
    let mut full = String::new();
    let mut initials = String::new();
    let mut camel = String::new();
    for c in s.chars() {
        if let Some(py) = c.to_pinyin() {
            let p = py.plain();
            full.push_str(p);
            camel.push_str(&title(p));
            if let Some(first) = p.chars().next() {
                initials.push(first)
            }
        } else {
            full.push(c);
            camel.push(c);
            if !c.is_whitespace() {
                initials.push(c)
            }
        }
    }
    (full, initials, camel)
}
fn forms(s: &str, leet: bool) -> Vec<String> {
    let mut base = vec![s.to_owned()];
    let compact = s.split_whitespace().collect::<String>();
    unique(&mut base, compact);
    let (py, initials, camel) = transliterate(s);
    unique(&mut base, py);
    unique(&mut base, camel);
    if s.chars().any(|c| c.to_pinyin().is_some()) {
        unique(&mut base, initials);
        let rest = s.chars().skip(1).collect::<String>();
        if !rest.is_empty() {
            let (given, given_initials, _) = transliterate(&rest);
            unique(&mut base, given);
            unique(&mut base, given_initials)
        }
    }
    for part in s.split_whitespace() {
        unique(&mut base, part.to_owned());
    }
    let mut out = Vec::new();
    for b in base {
        unique(&mut out, b.clone());
        unique(&mut out, b.to_lowercase());
        unique(&mut out, title(&b));
        unique(&mut out, b.to_uppercase());
        if leet && b.is_ascii() {
            let changed = b
                .to_lowercase()
                .replace('a', "@")
                .replace('e', "3")
                .replace('i', "1")
                .replace('o', "0")
                .replace('s', "$");
            unique(&mut out, changed);
        }
    }
    out
}
fn dates(s: &str) -> Result<Vec<String>, String> {
    let parts: Vec<&str> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|v| !v.is_empty())
        .collect();
    let (y, m, d) = if parts.len() == 1 && parts[0].len() == 8 {
        let p = parts[0];
        (&p[..4], &p[4..6], &p[6..])
    } else if parts.len() == 3 {
        (parts[0], parts[1], parts[2])
    } else if parts.len() == 1 && parts[0].len() == 4 {
        return Ok(vec![parts[0].to_owned(), parts[0][2..].to_owned()]);
    } else {
        return Err(format!(
            "生日「{s}」格式不正确，请使用 YYYY-MM-DD、YYYYMMDD 或仅年份"
        ));
    };
    let year: u32 = y.parse().map_err(|_| "生日年份无效")?;
    let month: u32 = m.parse().map_err(|_| "生日月份无效")?;
    let day: u32 = d.parse().map_err(|_| "生日日期无效")?;
    let leap = year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    if !(1000..=9999).contains(&year) || day == 0 || day > days {
        return Err(format!("生日「{s}」不是有效日期"));
    }
    let mut out = Vec::new();
    for value in [
        format!("{year:04}{month:02}{day:02}"),
        format!("{:02}{month:02}{day:02}", year % 100),
        format!("{month:02}{day:02}"),
        format!("{year:04}"),
        format!("{month}{day}"),
        format!("{year}{month}{day}"),
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{year:04}.{month:02}.{day:02}"),
        format!("{year:04}/{month:02}/{day:02}"),
        format!("{day:02}{month:02}"),
        format!("{:02}", year % 100),
    ] {
        unique(&mut out, value);
    }
    Ok(out)
}
struct Sink {
    words: Vec<String>,
    seen: HashSet<String>,
    counts: [usize; 3],
    samples: [Vec<String>; 3],
    cap: usize,
    min: usize,
    max: usize,
    truncated: bool,
}
impl Sink {
    fn add(&mut self, tier: usize, value: String) -> bool {
        let n = value.chars().count();
        if n < self.min || n > self.max || value.len() > 256 || self.seen.contains(&value) {
            return true;
        }
        if self.words.len() >= self.cap {
            self.truncated = true;
            return false;
        }
        self.seen.insert(value.clone());
        if self.samples[tier].len() < 8 {
            self.samples[tier].push(value.clone())
        }
        self.words.push(value);
        self.counts[tier] += 1;
        true
    }
    fn finish(self) -> Generation {
        let count = self.words.len();
        let mut preview: Vec<String> = self.samples[0].iter().take(4).cloned().collect();
        preview.extend(self.samples[1].iter().cloned());
        let decorated = self
            .words
            .iter()
            .skip(self.counts[0] + self.counts[1])
            .filter(|s| {
                s.chars().any(|c| c.is_ascii_digit())
                    && s.chars().any(char::is_alphabetic)
                    && s.chars().last().is_some_and(|c| !c.is_alphanumeric())
            })
            .take(8)
            .cloned()
            .collect::<Vec<_>>();
        if decorated.is_empty() {
            preview.extend(self.samples[2].iter().cloned());
        } else {
            preview.extend(decorated);
        }
        Generation {
            words: self.words,
            count,
            truncated: self.truncated,
            tiers: [
                "原始线索与常见变体",
                "姓名与日期、关键词组合",
                "重复、符号与常见后缀",
            ]
            .into_iter()
            .zip(self.counts)
            .map(|(label, count)| Tier {
                label: label.into(),
                count,
            })
            .collect(),
            preview,
            path: None,
        }
    }
}
pub fn generate(profile: &Profile) -> Result<Generation, String> {
    let limit = if profile.limit == 0 {
        20000
    } else {
        profile.limit
    };
    let min = profile.min_length.max(1);
    let max = if profile.max_length == 0 {
        32
    } else {
        profile.max_length
    };
    if limit > 200000 || max > 128 || min > max {
        return Err("候选上限需为 1–200000，长度范围需在 1–128 字符之间".into());
    }
    let names = tokens(&profile.names, 12)?;
    let words = tokens(&profile.words, 12)?;
    let birthdays = tokens(&profile.birthdays, 8)?;
    if names.is_empty() && words.is_empty() && birthdays.is_empty() {
        return Err("请至少输入姓名、生日或一个关键词".into());
    }
    let mut roots = Vec::new();
    for token in names.iter().chain(words.iter()) {
        for v in forms(token, profile.leet) {
            unique(&mut roots, v)
        }
    }
    let mut numbers = Vec::new();
    for date in birthdays {
        for value in dates(&date)? {
            unique(&mut numbers, value)
        }
    }
    let suffixes = tokens(&profile.suffixes, 16)?;
    let mut separators = vec![String::new()];
    for sep in tokens(&profile.separators, 5)? {
        if sep.chars().count() > 3 {
            return Err("连接符最多 3 个字符".into());
        }
        unique(&mut separators, sep)
    }
    let mut sink = Sink {
        words: Vec::new(),
        seen: HashSet::new(),
        counts: [0; 3],
        samples: std::array::from_fn(|_| Vec::new()),
        cap: limit,
        min,
        max,
        truncated: false,
    };
    macro_rules! emit {
        ($tier:expr,$value:expr) => {
            if !sink.add($tier, $value) {
                return Ok(sink.finish());
            }
        };
    }
    for root in roots.iter().chain(numbers.iter()) {
        emit!(0, root.clone());
    }
    for root in &roots {
        for number in &numbers {
            for sep in &separators {
                emit!(1, format!("{root}{sep}{number}"));
                emit!(1, format!("{number}{sep}{root}"));
            }
        }
    }
    // Decorate a bounded snapshot, preserving the stronger supplied hints first.
    let focused = sink.words.clone();
    for root in &focused {
        for suffix in &suffixes {
            emit!(2, format!("{root}{suffix}"));
        }
    }
    if profile.extended {
        for root in &roots {
            for sep in &separators {
                emit!(2, format!("{root}{sep}{root}"));
            }
            for suffix in &suffixes {
                emit!(2, format!("{suffix}{root}"));
            }
        }
        for a in &roots {
            for b in &roots {
                if a == b {
                    continue;
                }
                for sep in &separators {
                    emit!(2, format!("{a}{sep}{b}"));
                }
            }
        }
    }
    let result = sink.finish();
    if result.count == 0 {
        return Err("没有候选符合长度限制，请调整条件".into());
    }
    Ok(result)
}
pub fn save(profile: &Profile, data: &Path) -> Result<Generation, String> {
    let mut result = generate(profile)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir = data.join("profiles").join(stamp.to_string());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("candidates.txt");
    let mut file = BufWriter::new(fs::File::create(&path).map_err(|e| e.to_string())?);
    for word in &result.words {
        writeln!(file, "{word}").map_err(|e| e.to_string())?
    }
    file.flush().map_err(|e| e.to_string())?;
    result.path = Some(path);
    fs::write(
        dir.join("summary.json"),
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn dictionary_catalog(root: &Path) -> Vec<serde_json::Value> {
    let defs = [
        (
            "chinese10k",
            "中文常见密码 · 1 万条",
            "chinese-common-10000.txt",
            "常见口令排序，不等于纯汉字词表",
        ),
        (
            "chinese100k",
            "中文常见密码 · 10 万条",
            "chinese-common-100000.txt",
            "扩大候选，慢算法建议设置阶段时间上限",
        ),
        (
            "common10k",
            "通用常见密码 · 1 万条",
            "common-10000.txt",
            "通用小字典",
        ),
        (
            "hanzi",
            "中文词语",
            "chinese-words.txt",
            "汉字词表；不是口令概率排名",
        ),
        (
            "pinyin",
            "中文词语拼音",
            "chinese-pinyin.txt",
            "由中文词表转写，无声调；多音字可手动补充",
        ),
    ];
    defs.into_iter().filter_map(|(id,label,file,note)|{let path=root.join("assets/wordlists").join(file);let text=fs::read_to_string(&path).ok()?;Some(serde_json::json!({"id":id,"label":label,"path":path,"count":text.lines().filter(|l|!l.is_empty()).count(),"note":note}))}).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jenny_dates_and_symbols() {
        let p = Profile {
            names: "Jenny".into(),
            birthdays: "2000年1月3日".into(),
            separators: "_,@".into(),
            extended: true,
            ..Default::default()
        };
        let g = generate(&p).unwrap();
        for s in [
            "Jenny20000103!",
            "jenny0103",
            "JENNY2000",
            "20000103Jenny",
            "Jenny_Jenny",
            "JennyJenny",
        ] {
            assert!(g.words.iter().any(|v| v == s), "missing {s}");
        }
        assert_eq!(g.words.len(), g.words.iter().collect::<HashSet<_>>().len());
    }
    #[test]
    fn chinese_and_calendar() {
        let p = Profile {
            names: "王小明".into(),
            birthdays: "2000-02-29".into(),
            ..Default::default()
        };
        let g = generate(&p).unwrap();
        for s in ["王小明20000229!", "wangxiaoming0229", "wxm2000"] {
            assert!(g.words.contains(&s.to_string()), "{s}")
        }
        assert!(
            generate(&Profile {
                birthdays: "2001-02-29".into(),
                ..p
            })
            .is_err()
        );
        assert!(dates("2100-02-29").is_err());
    }
    #[test]
    fn bounded_and_deterministic() {
        let p = Profile {
            names: "Jenny, Jenny".into(),
            birthdays: "2000-01-03".into(),
            limit: 30,
            ..Default::default()
        };
        let a = generate(&p).unwrap();
        let b = generate(&p).unwrap();
        assert_eq!(a.words, b.words);
        assert_eq!(a.count, 30);
        assert!(a.truncated);
        assert!(generate(&Profile::default()).is_err());
    }
}

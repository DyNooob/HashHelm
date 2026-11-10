//! Read-only extraction of password verifiers. Never decrypts or unpacks input.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
type Result<T> = std::result::Result<T, String>;
mod archives;
fn fail(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn bytes(b: &[u8], p: usize, n: usize) -> Result<&[u8]> {
    b.get(p..p.checked_add(n).ok_or("偏移超出范围")?)
        .ok_or("文件结构不完整或已损坏".into())
}
fn u16le(b: &[u8], p: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(b, p, 2)?.try_into().unwrap()))
}
fn u32le(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(b, p, 4)?.try_into().unwrap()))
}
fn u64le(b: &[u8], p: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(bytes(b, p, 8)?.try_into().unwrap()))
}
fn hex(b: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(b.len() * 2);
    for v in b {
        let _ = write!(s, "{v:02x}");
    }
    s
}
fn read_at(f: &mut File, offset: u64, n: usize) -> Result<Vec<u8>> {
    let end = offset.checked_add(n as u64).ok_or("偏移超出范围")?;
    if end > f.metadata().map_err(fail)?.len() {
        return Err("文件数据超出边界".into());
    }
    f.seek(SeekFrom::Start(offset)).map_err(fail)?;
    let mut b = vec![0; n];
    f.read_exact(&mut b).map_err(fail)?;
    Ok(b)
}
#[derive(Serialize)]
pub struct Member {
    pub index: usize,
    pub name: String,
    pub mode: u32,
    pub size: u64,
}
#[derive(Serialize)]
pub struct Extracted {
    pub hash: String,
    pub mode: u32,
    pub format: String,
    pub note: String,
    pub members: Vec<Member>,
    pub selected: Option<usize>,
}
fn extracted(hash: String, mode: u32, format: impl Into<String>) -> Extracted {
    Extracted {
        hash,
        mode,
        format: format.into(),
        note: "已提取密码验证信息，原文件未修改。找到密码后请用原软件打开验证。".into(),
        members: vec![],
        selected: None,
    }
}
pub fn extract(path: &Path, member: Option<usize>) -> Result<Extracted> {
    let mut f = File::open(path).map_err(fail)?;
    let len = f.metadata().map_err(fail)?.len();
    if len < 8 {
        return Err("文件太短，无法识别".into());
    }
    let sig = read_at(&mut f, 0, 8)?;
    if sig.starts_with(b"PK\x03\x04") || sig.starts_with(b"PK\x05\x06") {
        return zip(&mut f, member);
    }
    if sig.starts_with(b"Rar!\x1a\x07") {
        return archives::rar(&mut f, member);
    }
    if sig.starts_with(b"7z\xbc\xaf\x27\x1c") {
        return archives::seven(&mut f);
    }
    if sig == [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1] {
        return office(path);
    }
    if sig.starts_with(b"%PDF-") {
        if len > 128 * 1024 * 1024 {
            return Err("PDF 超过当前 128 MiB 解析上限，请使用外部提取工具".into());
        }
        return pdf(path);
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if [
        "rar", "7z", "doc", "xls", "ppt", "docx", "xlsx", "pptx", "pdf", "zip",
    ]
    .contains(&ext.as_str())
    {
        return Err(
            "该文件格式暂不支持、文件已损坏或扩展名与内容不符；可使用外部工具提取后导入哈希".into(),
        );
    }
    if len > 16 * 1024 * 1024 {
        return Err("哈希文本超过 16 MiB，请拆分后导入或使用完整命令".into());
    }
    let b = read_at(&mut f, 0, len as usize)?;
    let text = std::str::from_utf8(&b)
        .map_err(|_| "不是支持的加密文件或 UTF-8 哈希文本")?
        .trim_start_matches('\u{feff}')
        .trim();
    if text.is_empty() || text.chars().any(|c| c == '\0') {
        return Err("哈希文本为空或包含二进制内容".into());
    }
    // Text inputs remain untouched and retain the user's manual hash-mode selection.
    Ok(extracted(text.into(), 0, "哈希文本（请确认算法）"))
}
struct ZipEntry {
    index: usize,
    name: String,
    flags: u16,
    method: u16,
    crc: u32,
    time: u16,
    compressed: u64,
    plain: u64,
    offset: u64,
    strength: Option<u8>,
}
fn zip(f: &mut File, selection: Option<usize>) -> Result<Extracted> {
    let len = f.metadata().map_err(fail)?.len();
    let tail = read_at(f, len.saturating_sub(65557), len.min(65557) as usize)?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&p| {
            tail[p..].starts_with(b"PK\x05\x06")
                && u16le(&tail, p + 20).is_ok_and(|n| p + 22 + usize::from(n) == tail.len())
        })
        .ok_or("找不到 ZIP 中央目录；分卷、损坏或自解压 ZIP 暂不支持")?;
    if u16le(&tail, end + 4)? != 0 || u16le(&tail, end + 6)? != 0 {
        return Err("分卷 ZIP 暂不支持".into());
    }
    let mut count = u64::from(u16le(&tail, end + 10)?);
    let mut size = u64::from(u32le(&tail, end + 12)?);
    let mut offset = u64::from(u32le(&tail, end + 16)?);
    let mut zip64 = false;
    if count == u64::from(u16::MAX) || size == u64::from(u32::MAX) || offset == u64::from(u32::MAX)
    {
        zip64 = true;
        let locator_offset = len
            .saturating_sub(tail.len() as u64)
            .checked_add(end as u64)
            .and_then(|v| v.checked_sub(20))
            .ok_or("ZIP64 定位记录缺失")?;
        let locator = read_at(f, locator_offset, 20)?;
        if &locator[..4] != b"PK\x06\x07" {
            return Err("ZIP64 定位记录缺失".into());
        }
        if u32le(&locator, 4)? != 0 || u32le(&locator, 16)? != 1 {
            return Err("分卷 ZIP 暂不支持".into());
        }
        let record_offset = u64le(&locator, 8)?;
        let record = read_at(f, record_offset, 56)?;
        if &record[..4] != b"PK\x06\x06" || !(44..=1024 * 1024).contains(&u64le(&record, 4)?) {
            return Err("ZIP64 结束记录损坏".into());
        }
        if record_offset
            .checked_add(12)
            .and_then(|v| v.checked_add(u64le(&record, 4).ok()?))
            != Some(locator_offset)
        {
            return Err("ZIP64 结束记录偏移无效".into());
        }
        if u32le(&record, 16)? != 0
            || u32le(&record, 20)? != 0
            || u64le(&record, 24)? != u64le(&record, 32)?
        {
            return Err("分卷 ZIP 暂不支持".into());
        }
        count = u64le(&record, 32)?;
        size = u64le(&record, 40)?;
        offset = u64le(&record, 48)?;
    } else if u64::from(u16le(&tail, end + 8)?) != count {
        return Err("ZIP 中央目录记录数量不一致".into());
    }
    if size > 16 * 1024 * 1024 || count > 20000 {
        return Err("ZIP 目录过大，超过当前解析上限".into());
    }
    let cd = read_at(f, offset, size as usize)?;
    let mut p = 0;
    let mut entries = Vec::new();
    let mut unsupported = false;
    for index in 0..count as usize {
        if bytes(&cd, p, 4)? != b"PK\x01\x02" {
            return Err("ZIP 中央目录损坏".into());
        }
        let flags = u16le(&cd, p + 8)?;
        let method = u16le(&cd, p + 10)?;
        let n = usize::from(u16le(&cd, p + 28)?);
        let x = usize::from(u16le(&cd, p + 30)?);
        let comment = usize::from(u16le(&cd, p + 32)?);
        let name = String::from_utf8_lossy(bytes(&cd, p + 46, n)?).to_string();
        let extra = bytes(&cd, p + 46 + n, x)?;
        let mut strength = None;
        let mut plain = u64::from(u32le(&cd, p + 24)?);
        let mut compressed = u64::from(u32le(&cd, p + 20)?);
        let mut entry_offset = u64::from(u32le(&cd, p + 42)?);
        let mut zip64_extra = false;
        let mut q = 0;
        while q + 4 <= extra.len() {
            let tag = u16le(extra, q)?;
            let sz = usize::from(u16le(extra, q + 2)?);
            let b = bytes(extra, q + 4, sz)?;
            if tag == 0x0001 {
                zip64_extra = true;
                zip64 = true;
                let mut at = 0;
                if plain == u64::from(u32::MAX) {
                    plain = u64le(b, at)?;
                    at += 8;
                }
                if compressed == u64::from(u32::MAX) {
                    compressed = u64le(b, at)?;
                    at += 8;
                }
                if entry_offset == u64::from(u32::MAX) {
                    entry_offset = u64le(b, at)?;
                    at += 8;
                }
                if u16le(&cd, p + 34)? == u16::MAX && u32le(b, at)? != 0 {
                    return Err("分卷 ZIP 暂不支持".into());
                }
            }
            if tag == 0x9901 && b.len() >= 7 && &b[2..4] == b"AE" {
                strength = Some(b[4]);
            }
            q += 4 + sz;
        }
        if !zip64_extra && [plain, compressed, entry_offset].contains(&u64::from(u32::MAX)) {
            return Err("ZIP64 扩展字段缺失".into());
        }
        if u16le(&cd, p + 34)? != 0 && u16le(&cd, p + 34)? != u16::MAX {
            return Err("分卷 ZIP 暂不支持".into());
        }
        if flags & 1 != 0 && !name.ends_with('/') {
            if flags & 0x40 != 0
                || !(method == 0 || method == 8 || method == 99 && strength.is_some())
            {
                unsupported = true;
            } else {
                entries.push(ZipEntry {
                    index,
                    name,
                    flags,
                    method,
                    crc: u32le(&cd, p + 16)?,
                    time: u16le(&cd, p + 12)?,
                    compressed,
                    plain,
                    offset: entry_offset,
                    strength,
                });
            }
        }
        p = p.checked_add(46 + n + x + comment).ok_or("目录偏移溢出")?;
    }
    if entries.is_empty() {
        return Err(if unsupported {
            "ZIP 使用当前不支持的加密或压缩方式"
        } else {
            "ZIP 没有可恢复的加密文件；普通 Office 文档或仅编辑保护也不需要恢复打开密码"
        }
        .into());
    }
    let eligible = |e: &&ZipEntry| {
        e.compressed
            <= if e.strength.is_some() {
                8 * 1024 * 1024
            } else {
                300 * 1024
            }
    };
    let selected = if let Some(i) = selection {
        entries
            .iter()
            .find(|e| e.index == i)
            .ok_or("所选 ZIP 项不存在")?
    } else {
        entries
            .iter()
            .filter(eligible)
            .min_by_key(|e| e.compressed)
            .ok_or("ZIP 内没有足够小的加密项供当前 Hashcat 模式使用，请使用外部提取工具")?
    };
    if !eligible(&selected) {
        return Err("所选 ZIP 项过大，请选择较小的加密项或使用外部提取工具".into());
    }
    let local = read_at(f, selected.offset, 30)?;
    if &local[..4] != b"PK\x03\x04" {
        return Err("ZIP 本地文件头损坏".into());
    }
    let start = selected
        .offset
        .checked_add(30)
        .and_then(|n| n.checked_add(u64::from(u16le(&local, 26).ok()?)))
        .and_then(|n| n.checked_add(u64::from(u16le(&local, 28).ok()?)))
        .ok_or("ZIP 数据偏移溢出")?;
    let b = read_at(f, start, selected.compressed as usize)?;
    let (hash, mode) = if let Some(s) = selected.strength {
        let salt = match s {
            1 => 8,
            2 => 12,
            3 => 16,
            _ => return Err("未知的 ZIP AES 密钥长度".into()),
        };
        if b.len() < salt + 12 {
            return Err("ZIP AES 数据不完整".into());
        }
        let data = bytes(&b, salt + 2, b.len() - salt - 12)?;
        (
            format!(
                "$zip2$*0*{s}*0*{}*{}*{:x}*{}*{}*$/zip2$",
                hex(&b[..salt]),
                hex(&b[salt..salt + 2]),
                data.len(),
                hex(data),
                hex(&b[b.len() - 10..])
            ),
            13600,
        )
    } else {
        if b.len() < 12 {
            return Err("ZipCrypto 数据不完整".into());
        }
        let checksum = if selected.flags & 8 != 0 {
            selected.time
        } else {
            (selected.crc >> 16) as u16
        };
        (
            format!(
                "$pkzip$1*1*2*0*{:x}*{:x}*{:08x}*0*0*{:x}*{:x}*{:04x}*{}*$/pkzip$",
                selected.compressed,
                selected.plain,
                selected.crc,
                selected.method,
                b.len(),
                checksum,
                hex(&b)
            ),
            if selected.method == 0 { 17210 } else { 17200 },
        )
    };
    let mut result = extracted(
        hash,
        mode,
        if mode == 13600 {
            "ZIP · WinZip AES"
        } else {
            "ZIP · ZipCrypto"
        },
    );
    if zip64 {
        result.format = result.format.replacen("ZIP", "ZIP64", 1);
    }
    result.note = format!(
        "正在验证 ZIP 内的「{}」。其他加密项可能使用不同密码；找到后请解压验证。",
        selected.name
    );
    result.selected = Some(selected.index);
    result.members = entries
        .iter()
        .map(|e| Member {
            index: e.index,
            name: e.name.clone(),
            mode: if e.strength.is_some() {
                13600
            } else if e.method == 0 {
                17210
            } else {
                17200
            },
            size: e.compressed,
        })
        .collect();
    Ok(result)
}
fn stream(c: &mut cfb::CompoundFile<File>, name: &str) -> Result<Vec<u8>> {
    let s = c.open_stream(name).map_err(fail)?;
    if s.len() > 1024 * 1024 {
        return Err("Office 加密头超过 1 MiB 限制".into());
    }
    let mut b = Vec::new();
    s.take(1024 * 1024 + 1).read_to_end(&mut b).map_err(fail)?;
    Ok(b)
}
fn office(path: &Path) -> Result<Extracted> {
    let mut c = cfb::open(path).map_err(fail)?;
    if c.exists("/EncryptionInfo") {
        if !c.exists("/EncryptedPackage") {
            return Err("Office 缺少 EncryptedPackage 数据流".into());
        }
        return office_info(&stream(&mut c, "/EncryptionInfo")?);
    }
    if c.exists("/WordDocument") {
        let b = stream(&mut c, "/WordDocument")?;
        if u16le(&b, 0)? != 0xa5ec {
            return Err("不支持的旧版 Word 格式".into());
        }
        let flags = u16le(&b, 10)?;
        if flags & 0x0100 == 0 {
            return Err("Word 文档没有打开密码；编辑限制不是文件加密".into());
        }
        if flags & 0x8000 != 0 {
            return Err("旧 Word XOR 混淆暂不支持自动提取".into());
        }
        let table = if flags & 0x0200 != 0 {
            "/1Table"
        } else {
            "/0Table"
        };
        return old_office(&stream(&mut c, table)?, 0);
    }
    for name in ["/Workbook", "/Book"] {
        if c.exists(name) {
            let b = stream(&mut c, name)?;
            let mut p = 0;
            while p + 4 <= b.len() {
                let id = u16le(&b, p)?;
                let n = usize::from(u16le(&b, p + 2)?);
                let body = bytes(&b, p + 4, n)?;
                if id == 0x2f {
                    if u16le(body, 0)? != 1 {
                        return Err("旧 Excel XOR 混淆暂不支持自动提取".into());
                    }
                    return old_office(bytes(body, 2, n.saturating_sub(2))?, 1);
                }
                p += 4 + n;
            }
            return Err("Excel 没有可恢复的打开密码或采用不支持的格式".into());
        }
    }
    Err("该 OLE 文件不是支持的 Office 加密格式；旧版 PPT、Access、IRM 暂不支持自动提取".into())
}
fn office_info(b: &[u8]) -> Result<Extracted> {
    let major = u16le(b, 0)?;
    let minor = u16le(b, 2)?;
    if major == 4 && minor == 4 {
        let xml = std::str::from_utf8(bytes(b, 8, b.len().saturating_sub(8))?).map_err(fail)?;
        let doc = roxmltree::Document::parse(xml).map_err(fail)?;
        let node = doc
            .descendants()
            .find(|n| {
                n.has_tag_name((
                    "http://schemas.microsoft.com/office/2006/keyEncryptor/password",
                    "encryptedKey",
                ))
            })
            .ok_or("Office 没有口令加密器，可能使用证书或 IRM")?;
        let attr = |key| {
            node.attribute(key)
                .ok_or_else(|| format!("Office 缺少属性 {key}"))
        };
        let num = |key| -> Result<u32> { attr(key)?.parse().map_err(fail) };
        let decode = |key| -> Result<Vec<u8>> { STANDARD.decode(attr(key)?).map_err(fail) };
        let hash = attr("hashAlgorithm")?;
        let bits = num("keyBits")?;
        let (version, mode) = match (hash, bits) {
            ("SHA1", 128 | 256) => (2010, 9500),
            ("SHA512", 256) => (2013, 9600),
            _ => return Err("Office 的哈希算法或密钥长度组合暂不支持".into()),
        };
        if attr("cipherAlgorithm")? != "AES"
            || attr("cipherChaining")? != "ChainingModeCBC"
            || num("blockSize")? != 16
        {
            return Err("Office 当前仅支持 AES-CBC".into());
        }
        let salt = decode("saltValue")?;
        let verifier = decode("encryptedVerifierHashInput")?;
        let digest = decode("encryptedVerifierHashValue")?;
        let spins = num("spinCount")?;
        if salt.len() != 16
            || num("saltSize")? != 16
            || verifier.len() != 16
            || digest.len() < 32
            || spins == 0
            || spins > 10_000_000
        {
            return Err("Office 验证参数不完整或超出支持范围".into());
        }
        return Ok(extracted(
            format!(
                "$office$*{version}*{spins}*{bits}*16*{}*{}*{}",
                hex(&salt),
                hex(&verifier),
                hex(&digest[..32])
            ),
            mode,
            format!("Office · AES / {hash}（{version} 兼容模式）"),
        ));
    }
    if ![2, 3, 4].contains(&major) || minor != 2 {
        return Err("该 Office 加密版本暂不支持".into());
    }
    let header = u32le(b, 8)? as usize;
    if header < 32 {
        return Err("Office 标准加密头损坏".into());
    }
    let bits = u32le(b, 28)?;
    if u32le(b, 20)? != 0x660e || u32le(b, 24)? != 0x8004 || bits != 128 {
        return Err("Office 2007 当前仅支持 AES-128 / SHA1".into());
    }
    let p = 12 + header;
    if u32le(b, p)? != 16 || u32le(b, p + 36)? != 20 {
        return Err("Office 2007 验证结构损坏".into());
    }
    Ok(extracted(
        format!(
            "$office$*2007*20*128*16*{}*{}*{}",
            hex(bytes(b, p + 4, 16)?),
            hex(bytes(b, p + 20, 16)?),
            hex(bytes(b, p + 40, 20)?)
        ),
        9400,
        "Office · 2007 标准加密",
    ))
}
fn old_office(b: &[u8], kind: u32) -> Result<Extracted> {
    let major = u16le(b, 0)?;
    let minor = u16le(b, 2)?;
    if major == 1 && minor == 1 {
        return Ok(extracted(
            format!(
                "$oldoffice${kind}*{}*{}*{}",
                hex(bytes(b, 4, 16)?),
                hex(bytes(b, 20, 16)?),
                hex(bytes(b, 36, 16)?)
            ),
            9700,
            "Office ≤2003 · RC4",
        ));
    }
    if [2, 3, 4].contains(&major) && minor == 2 {
        let header = u32le(b, 8)? as usize;
        if header < 32 || u32le(b, 20)? != 0x6801 || u32le(b, 24)? != 0x8004 {
            return Err("旧 Office CryptoAPI 参数暂不支持".into());
        }
        let typ = match u32le(b, 28)? {
            40 => 3,
            128 => 4,
            _ => return Err("旧 Office RC4 密钥长度暂不支持".into()),
        };
        let p = 12 + header;
        if u32le(b, p)? != 16 || u32le(b, p + 36)? != 20 {
            return Err("旧 Office 验证结构损坏".into());
        }
        return Ok(extracted(
            format!(
                "$oldoffice${typ}*{}*{}*{}",
                hex(bytes(b, p + 4, 16)?),
                hex(bytes(b, p + 20, 16)?),
                hex(bytes(b, p + 40, 20)?)
            ),
            9800,
            "Office ≤2003 · RC4 CryptoAPI",
        ));
    }
    Err("旧 Office 加密版本暂不支持".into())
}
fn pdf(path: &Path) -> Result<Extracted> {
    let doc = lopdf::Document::load(path).map_err(fail)?;
    if doc.trailer.has(b"Encrypt") && doc.authenticate_password("").is_ok() {
        return Err("PDF 可用空密码打开，无需恢复打开密码；权限限制不等于文件打开密码".into());
    }
    let enc = doc
        .trailer
        .get(b"Encrypt")
        .map_err(|_| "PDF 没有密码加密")?;
    let enc = doc
        .dereference(enc)
        .map_err(fail)?
        .1
        .as_dict()
        .map_err(fail)?;
    if enc.get(b"Filter").and_then(|v| v.as_name()).map_err(fail)? != b"Standard" {
        return Err("PDF 证书或自定义加密暂不支持".into());
    }
    let int = |key: &[u8]| -> Result<i64> { enc.get(key).and_then(|v| v.as_i64()).map_err(fail) };
    let string =
        |key: &[u8]| -> Result<&[u8]> { enc.get(key).and_then(|v| v.as_str()).map_err(fail) };
    let v = int(b"V")?;
    let r = int(b"R")?;
    let bits = int(b"Length").unwrap_or(40);
    let permissions = int(b"P")? as i32;
    let metadata = enc
        .get(b"EncryptMetadata")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mode = match (r, v, bits) {
        (2, 1, 40) => 10400,
        (3, 1, 40) => 10510,
        (3 | 4, 2 | 4, 40..=128) => 10500,
        (5, 5, 256) => 10600,
        (6, 5, 256) => 10700,
        _ => return Err("该 PDF 加密修订版本或密钥长度暂不支持".into()),
    };
    let id = doc
        .trailer
        .get(b"ID")
        .and_then(|v| v.as_array())
        .ok()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str().ok())
        .unwrap_or(&[]);
    if r <= 4 && id.is_empty() {
        return Err("PDF 缺少文档 ID，无法可靠提取".into());
    }
    if r == 2 && id.len() != 16 {
        return Err("该旧版 PDF 的文档 ID 不是 16 字节，超出 Hashcat 10400 模式支持范围".into());
    }
    let u = string(b"U")?;
    let o = string(b"O")?;
    if u.len() > 127
        || o.len() > 127
        || u.len() < if r >= 5 { 48 } else { 32 }
        || o.len() < if r >= 5 { 48 } else { 32 }
    {
        return Err("PDF 验证数据长度异常".into());
    }
    let mut hash = format!(
        "$pdf${v}*{r}*{bits}*{permissions}*{}*{}*{}*{}*{}*{}*{}",
        u8::from(metadata),
        id.len(),
        hex(id),
        u.len(),
        hex(u),
        o.len(),
        hex(o)
    );
    if r >= 5 {
        let ue = string(b"UE")?;
        let oe = string(b"OE")?;
        hash.push_str(&format!(
            "*{}*{}*{}*{}",
            ue.len(),
            hex(ue),
            oe.len(),
            hex(oe)
        ));
    }
    Ok(extracted(hash, mode, format!("PDF · 加密修订 R{r}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truncated_data_is_rejected() {
        for n in 0..80 {
            assert!(office_info(&vec![0; n]).is_err());
        }
        assert!(old_office(&[1, 0, 1, 0], 0).is_err());
    }
    #[test]
    fn imports_generated_files() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import");
        if !base.exists() {
            return;
        }
        for (name, mode) in [
            ("zip64.zip", 17200),
            ("seven.7z", 11600),
            ("seven-headers.7z", 11600),
            ("seven-store.7z", 11600),
            ("seven-lzma.7z", 11600),
            ("rar3.rar", 23800),
            ("rar3-store.rar", 23700),
            ("rar3-headers.rar", 12500),
            ("rar5.rar", 13000),
            ("rar5-headers.rar", 13000),
            ("aes.zip", 13600),
            ("legacy.zip", 17200),
            ("stored.zip", 17210),
            ("word.docx", 9600),
            ("excel.xlsx", 9600),
            ("slides.pptx", 9600),
            ("office-2007.docx", 9400),
            ("office-2010.xlsx", 9500),
            ("old-word.doc", 9800),
            ("old-excel.xls", 9800),
            ("pdf-128.pdf", 10500),
            ("pdf-256.pdf", 10700),
        ] {
            let r = extract(&base.join(name), None).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(r.mode, mode, "{name}");
            assert!(!r.hash.contains('\n'));
        }
    }

    #[test]
    fn selects_zip_members_and_rejects_plain_or_invalid_files() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import");
        if !base.exists() {
            return;
        }
        let first = extract(&base.join("aes.zip"), Some(0)).unwrap();
        let second = extract(&base.join("aes.zip"), Some(1)).unwrap();
        assert_ne!(first.hash, second.hash);
        assert_eq!(second.selected, Some(1));
        assert!(extract(&base.join("aes.zip"), Some(99)).is_err());
        for name in [
            "plain-word.docx",
            "truncated.zip",
            "fake.docx",
            "broken-zip64.zip",
        ] {
            assert!(extract(&base.join(name), None).is_err(), "{name}");
        }
    }
}

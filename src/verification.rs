//! Independent file verification. Password bytes never enter argv or receipts.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

type Result<T> = std::result::Result<T, String>;
const MAX_OUTPUT: u64 = 1024 * 1024 * 1024;
const MAX_DOCUMENT: u64 = 128 * 1024 * 1024;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRecord {
    pub source: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub member: Option<usize>,
    #[serde(default)]
    pub member_name: Option<String>,
    pub format: String,
    #[serde(default)]
    pub container: Option<crate::containers::Record>,
}
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub record: SourceRecord,
    pub password_hex: String,
    pub destination: Option<PathBuf>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub status: String,
    pub source: PathBuf,
    pub source_sha256: String,
    pub scope: String,
    pub method: String,
    pub bytes: u64,
    pub destination: Option<PathBuf>,
    pub timestamp: u64,
    #[serde(default)]
    pub fingerprint_scope: String,
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn password(hex: &str) -> Result<Vec<u8>> {
    if hex.len() > 4096 || !hex.len().is_multiple_of(2) {
        return Err("密码字节格式无效".into());
    }
    hex.as_bytes()
        .chunks(2)
        .map(|b| {
            std::str::from_utf8(b)
                .map_err(err)
                .and_then(|s| u8::from_str_radix(s, 16).map_err(err))
        })
        .collect()
}
fn fingerprint_file(file: &mut File, cancel: &AtomicBool) -> Result<(String, u64)> {
    file.seek(SeekFrom::Start(0)).map_err(err)?;
    let mut hash = Sha256::new();
    let mut count = 0;
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("操作已取消".into());
        }
        let n = file.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        count += n as u64;
    }
    file.seek(SeekFrom::Start(0)).map_err(err)?;
    Ok((hex(&hash.finalize()), count))
}
pub fn record(
    source: &Path,
    member: Option<usize>,
    format: String,
    cancel: &AtomicBool,
) -> Result<SourceRecord> {
    let source = source.canonicalize().map_err(err)?;
    let (sha256, size) = fingerprint_file(&mut File::open(&source).map_err(err)?, cancel)?;
    Ok(SourceRecord {
        source,
        sha256,
        size,
        member,
        member_name: None,
        format,
        container: None,
    })
}
pub fn record_container(
    source: &Path,
    spec: crate::containers::Spec,
    cancel: &AtomicBool,
) -> Result<SourceRecord> {
    let source = source.canonicalize().map_err(err)?;
    let container = crate::containers::record(&source, spec, cancel)?;
    Ok(SourceRecord {
        source,
        sha256: container.header_sha256.clone(),
        size: container.source_size,
        member: None,
        member_name: None,
        format: "VeraCrypt · PBKDF2 卷头".into(),
        container: Some(container),
    })
}
pub fn load_record(path: &Path, data: &Path) -> Result<SourceRecord> {
    let path = path.canonicalize().map_err(err)?;
    let imports = data.join("imports").canonicalize().map_err(err)?;
    if !path.starts_with(imports)
        || path.file_name().and_then(|s| s.to_str()) != Some("source.json")
    {
        return Err("无效的原文件记录".into());
    }
    let file = File::open(path).map_err(err)?;
    let value: serde_json::Value = serde_json::from_reader(file.take(1024 * 1024)).map_err(err)?;
    serde_json::from_value(value["verification"].clone())
        .map_err(|_| "该记录没有原文件指纹，请重新导入文件".into())
}
struct Limited<'a> {
    out: &'a mut dyn Write,
    count: u64,
}
impl Write for Limited<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if self.count.saturating_add(b.len() as u64) > MAX_OUTPUT {
            return Err(std::io::Error::other("解密内容超过 1 GiB 上限"));
        }
        let n = self.out.write(b)?;
        self.count += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}
fn safe_name(name: &str) -> Result<String> {
    let name = name.replace('\\', "/");
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(':')
        || name.contains('\0')
        || name.split('/').any(|p| p == "..")
    {
        return Err("压缩包含不安全的文件路径，拒绝导出".into());
    }
    Ok(name)
}
fn zip_verify(
    file: File,
    pass: &[u8],
    member: usize,
    out: &mut dyn Write,
) -> Result<(String, String, u64)> {
    let mut archive = zip::ZipArchive::new(file).map_err(err)?;
    if archive.len() > 20000 {
        return Err("ZIP 项目过多".into());
    }
    let mut entry = archive.by_index_decrypt(member, pass).map_err(err)?;
    if !entry.encrypted() || entry.is_dir() {
        return Err("所选项不是加密文件".into());
    }
    let name = entry.name().to_owned();
    let mut out = Limited { out, count: 0 };
    let bytes = std::io::copy(&mut entry, &mut out).map_err(err)?;
    if bytes != entry.size() {
        return Err("ZIP 内容长度不完整".into());
    }
    Ok((
        format!("ZIP 所选项：{name}（其他项可能使用不同密码）"),
        "完整读取 / ZIP CRC32 或 AES 认证".into(),
        bytes,
    ))
}
fn office_verify(mut file: File, pass: &str, out: &mut dyn Write) -> Result<(String, String, u64)> {
    if file.metadata().map_err(err)?.len() > MAX_DOCUMENT {
        return Err("Office 验证上限为 128 MiB".into());
    }
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(err)?;
    let modern = cfb::CompoundFile::open(std::io::Cursor::new(&data))
        .map_err(err)?
        .exists("/EncryptedPackage");
    let plain = if modern {
        msoffice_crypto::decrypt_ooxml(&data, pass).map_err(err)?
    } else {
        msoffice_crypto::decrypt_binary_office(&data, pass).map_err(err)?
    };
    if plain.len() as u64 > MAX_DOCUMENT {
        return Err("解密文档超过 128 MiB 上限".into());
    }
    if modern {
        let mut package = zip::ZipArchive::new(std::io::Cursor::new(&plain)).map_err(err)?;
        if package.len() > 20000 {
            return Err("Office 项目过多".into());
        }
        if package.by_name("[Content_Types].xml").is_err() {
            return Err("解密结果不是有效 OOXML 文档".into());
        }
        for i in 0..package.len() {
            let mut entry = package.by_index(i).map_err(err)?;
            std::io::copy(
                &mut entry,
                &mut Limited {
                    out: &mut std::io::sink(),
                    count: 0,
                },
            )
            .map_err(err)?;
        }
    } else {
        cfb::CompoundFile::open(std::io::Cursor::new(&plain)).map_err(err)?;
    }
    out.write_all(&plain).map_err(err)?;
    Ok((
        "完整 Office 文档".into(),
        if modern {
            "Office 密码校验 / 解密 / OOXML CRC32"
        } else {
            "Office 密码校验 / 解密 / CFB 结构校验"
        }
        .into(),
        plain.len() as u64,
    ))
}
fn pdf_verify(file: File, pass: &[u8], out: &mut dyn Write) -> Result<(String, String, u64)> {
    if file.metadata().map_err(err)?.len() > MAX_DOCUMENT {
        return Err("PDF 验证上限为 128 MiB".into());
    }
    let mut doc = lopdf::Document::load_from(file).map_err(err)?;
    // Some writers include the redundant /Length 40 in V=1/R=2. The key
    // length is fixed by this revision; lopdf otherwise rejects that field.
    let encryption = doc
        .trailer
        .get(b"Encrypt")
        .and_then(lopdf::Object::as_reference)
        .map_err(err)?;
    let dictionary = doc
        .get_object_mut(encryption)
        .and_then(lopdf::Object::as_dict_mut)
        .map_err(err)?;
    if dictionary.get(b"V").and_then(lopdf::Object::as_i64).ok() == Some(1)
        && dictionary.get(b"R").and_then(lopdf::Object::as_i64).ok() == Some(2)
        && dictionary
            .get(b"Length")
            .and_then(lopdf::Object::as_i64)
            .ok()
            == Some(40)
    {
        dictionary.remove(b"Length");
    }
    doc.decrypt_raw(pass).map_err(err)?;
    if doc.is_encrypted() {
        return Err("PDF 仍处于加密状态".into());
    }
    let mut output = Limited { out, count: 0 };
    doc.save_to(&mut output).map_err(err)?;
    Ok((
        "PDF 打开密码".into(),
        "PDF 标准安全处理器验证及对象解密（不保证文件内容完整性）".into(),
        output.count,
    ))
}
fn seven_verify(
    file: File,
    pass: &str,
    out: &mut dyn Write,
    export: bool,
) -> Result<(String, String, u64)> {
    let mut archive =
        sevenz_rust2::ArchiveReader::new_with_memory_limit(file, pass.into(), 128 * 1024)
            .map_err(err)?;
    archive.set_thread_count(1);
    let mut zip = if export {
        Some(zip::ZipWriter::new(tempfile::tempfile().map_err(err)?))
    } else {
        None
    };
    let mut total = 0u64;
    let mut files = 0;
    let mut problem = None;
    archive
        .for_each_entries(|entry, reader| {
            let result = (|| -> Result<()> {
                files += 1;
                if files > 20000 || entry.size > MAX_OUTPUT.saturating_sub(total) {
                    return Err("压缩包超过 20000 项或 1 GiB 上限".into());
                }
                if entry.is_directory {
                    return Ok(());
                }
                if !entry.has_crc && entry.has_stream {
                    return Err("7z 项目缺少 CRC，无法确认内容".into());
                }
                let mut crc = crc32fast::Hasher::new();
                let mut n = 0;
                let mut b = [0u8; 65536];
                if let Some(zip) = zip.as_mut() {
                    zip.start_file(
                        safe_name(&entry.name)?,
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored),
                    )
                    .map_err(err)?;
                }
                loop {
                    let size = reader.read(&mut b).map_err(err)?;
                    if size == 0 {
                        break;
                    }
                    n += size as u64;
                    if n > entry.size || total.saturating_add(n) > MAX_OUTPUT {
                        return Err("7z 内容超出声明大小".into());
                    }
                    crc.update(&b[..size]);
                    if let Some(zip) = zip.as_mut() {
                        zip.write_all(&b[..size]).map_err(err)?;
                    }
                }
                if n != entry.size || (entry.has_crc && crc.finalize() != entry.crc as u32) {
                    return Err("7z 内容 CRC 校验失败".into());
                }
                total += n;
                Ok(())
            })();
            if let Err(error) = result {
                problem = Some(error);
                return Ok(false);
            }
            Ok(true)
        })
        .map_err(err)?;
    if let Some(error) = problem {
        return Err(error);
    }
    if files == 0 {
        return Err("压缩包没有可验证项目".into());
    }
    if let Some(zip) = zip {
        let mut file = zip.finish().map_err(err)?;
        file.seek(SeekFrom::Start(0)).map_err(err)?;
        std::io::copy(&mut file, out).map_err(err)?;
    }
    Ok((
        "7z 全部文件内容".into(),
        "7z 解密 / 解压 / 逐项 CRC32".into(),
        total,
    ))
}
fn rar_verify(
    path: &Path,
    pass: &str,
    out: &mut dyn Write,
    export: bool,
) -> Result<(String, String, u64)> {
    let mut archive = unrar::Archive::with_password(path, pass)
        .open_for_processing()
        .map_err(err)?;
    if !matches!(archive.volume_info(), unrar::VolumeInfo::None) {
        return Err("暂不支持分卷 RAR 验证".into());
    }
    let mut zip = if export {
        Some(zip::ZipWriter::new(tempfile::tempfile().map_err(err)?))
    } else {
        None
    };
    let mut total = 0u64;
    let mut files = 0;
    while let Some(header) = archive.read_header().map_err(err)? {
        let entry = header.entry();
        files += 1;
        if files > 20000 || entry.unpacked_size > MAX_OUTPUT.saturating_sub(total) {
            return Err("RAR 超过 20000 项或 1 GiB 上限".into());
        }
        if entry.is_split() {
            return Err("暂不支持分卷 RAR 验证".into());
        }
        if entry.is_directory() {
            archive = header.skip().map_err(err)?;
            continue;
        }
        total += entry.unpacked_size;
        if let Some(zip) = zip.as_mut() {
            if entry.unpacked_size > MAX_DOCUMENT {
                return Err("RAR 导出单项上限为 128 MiB".into());
            }
            let name = safe_name(&entry.filename.to_string_lossy())?;
            let (bytes, next) = header.read().map_err(err)?;
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .map_err(err)?;
            zip.write_all(&bytes).map_err(err)?;
            archive = next;
        } else {
            archive = header.test().map_err(err)?;
        }
    }
    if files == 0 {
        return Err("RAR 没有可验证项目".into());
    }
    if let Some(zip) = zip {
        let mut file = zip.finish().map_err(err)?;
        file.seek(SeekFrom::Start(0)).map_err(err)?;
        std::io::copy(&mut file, out).map_err(err)?;
    }
    Ok((
        "RAR 全部文件内容".into(),
        "UnRAR 解密 / 解压 / 内容校验".into(),
        total,
    ))
}
pub fn execute(request: Request, cancel: &AtomicBool) -> Result<Receipt> {
    let record = &request.record;
    if let Some(container) = &record.container {
        if request.destination.is_some() {
            return Err(
                "容器卷头验证不导出解密副本；请用恢复的密码和 PIM 在 VeraCrypt 中打开容器".into(),
            );
        }
        let pass = password(&request.password_hex)?;
        let method = crate::containers::verify(&record.source, container, &pass, cancel)?;
        return Ok(Receipt {
            status: "verified".into(),
            source: record.source.clone(),
            source_sha256: container.header_sha256.clone(),
            scope: "VeraCrypt 所选卷头（仅验证卷头，未校验或解密数据区）".into(),
            method,
            bytes: 512,
            destination: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            fingerprint_scope: "container_header".into(),
        });
    }
    let mut file = File::open(&record.source).map_err(err)?;
    let (sha, size) = fingerprint_file(&mut file, cancel)?;
    if sha != record.sha256 || size != record.size {
        return Err("原文件已改变，请重新导入后验证".into());
    }
    let pass = password(&request.password_hex)?;
    let mut signature = [0; 8];
    file.read_exact(&mut signature).map_err(err)?;
    file.seek(SeekFrom::Start(0)).map_err(err)?;
    let mut temp = if let Some(destination) = &request.destination {
        if destination.exists() {
            return Err("目标文件已存在，请另选名称（不会覆盖原文件）".into());
        }
        Some(
            tempfile::NamedTempFile::new_in(destination.parent().ok_or("无效的导出路径")?)
                .map_err(err)?,
        )
    } else {
        None
    };
    let export = temp.is_some();
    let mut sink = std::io::sink();
    let out: &mut dyn Write = match temp.as_mut() {
        Some(file) => file.as_file_mut(),
        None => &mut sink,
    };
    let (scope, method, bytes) = if signature.starts_with(b"PK") {
        zip_verify(
            file,
            &pass,
            record.member.ok_or("缺少 ZIP 所选项记录，请重新导入")?,
            out,
        )?
    } else if signature.starts_with(b"%PDF-") {
        pdf_verify(file, &pass, out)?
    } else {
        let pass = std::str::from_utf8(&pass)
            .map_err(|_| "此文件格式的原文件验证需要 UTF-8 密码；请使用原软件验证这些字节")?;
        if signature.starts_with(b"7z\xbc\xaf\x27\x1c") {
            seven_verify(file, pass, out, export)?
        } else if signature.starts_with(b"Rar!\x1a\x07") {
            rar_verify(&record.source, pass, out, export)?
        } else if signature == [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1] {
            office_verify(file, pass, out)?
        } else {
            return Err("该类型尚无独立原文件验证器".into());
        }
    };
    if cancel.load(Ordering::SeqCst) {
        return Err("操作已取消".into());
    }
    let after = fingerprint_file(&mut File::open(&record.source).map_err(err)?, cancel)?;
    if after != (record.sha256.clone(), record.size) {
        return Err("验证期间原文件发生改变，结果已作废".into());
    }
    if let (Some(temp), Some(destination)) = (temp, request.destination.as_ref()) {
        temp.as_file().sync_all().map_err(err)?;
        temp.persist_noclobber(destination).map_err(err)?;
    }
    Ok(Receipt {
        status: "verified".into(),
        fingerprint_scope: "file".into(),
        source: record.source.clone(),
        source_sha256: record.sha256.clone(),
        scope,
        method,
        bytes,
        destination: request.destination,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}

/// Invoked before any desktop initialization. JSON is sent on stdin, not argv.
pub fn worker_entry() -> Option<i32> {
    if !std::env::args().any(|a| a == "--hashhelm-file-worker") {
        return None;
    }
    let result = serde_json::from_reader::<_, Request>(std::io::stdin().take(64 * 1024))
        .map_err(err)
        .and_then(|r| execute(r, &AtomicBool::new(false)));
    let result = match result {
        Ok(receipt) => serde_json::json!({"receipt":receipt}),
        Err(message) => serde_json::json!({"error":message}),
    };
    let _ = writeln!(std::io::stdout(), "{result}");
    Some(0)
}

pub fn run(mut request: Request, cancel: &AtomicBool) -> Result<Receipt> {
    let destination = request.destination.take();
    let staging = if let Some(destination) = &destination {
        if destination.exists() {
            return Err("目标文件已存在，请另选名称".into());
        }
        Some(
            tempfile::Builder::new()
                .prefix(".hashhelm-export-")
                .tempdir_in(destination.parent().ok_or("无效导出路径")?)
                .map_err(err)?,
        )
    } else {
        None
    };
    request.destination = staging.as_ref().map(|dir| dir.path().join("output"));
    let mut command = std::process::Command::new(std::env::current_exe().map_err(err)?);
    command
        .arg("--hashhelm-file-worker")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let input = serde_json::to_vec(&request).map_err(err)?;
    let output =
        crate::engine::query_input(command, cancel, std::time::Duration::from_secs(120), &input)
            .map_err(|message| {
                if message.contains("超时") {
                    "原文件验证超过 120 秒，已停止；可使用原软件继续验证".into()
                } else {
                    message.replace("结果读取", "原文件验证")
                }
            })?;
    if !output.status.success() {
        return Err("原文件验证进程未正常完成".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "原文件验证器返回格式无效".to_owned())?;
    if let Some(error) = value["error"].as_str() {
        return Err(error.to_owned());
    }
    let mut receipt: Receipt = serde_json::from_value(value["receipt"].clone()).map_err(err)?;
    if let (Some(staging), Some(destination)) = (&staging, &destination) {
        if cancel.load(Ordering::SeqCst) {
            return Err("导出已取消".into());
        }
        let mut file = tempfile::NamedTempFile::new_in(destination.parent().ok_or("无效导出路径")?)
            .map_err(err)?;
        let mut input = File::open(staging.path().join("output")).map_err(err)?;
        let mut buffer = [0u8; 65536];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("导出已取消".into());
            }
            let n = input.read(&mut buffer).map_err(err)?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n]).map_err(err)?;
        }
        file.as_file().sync_all().map_err(err)?;
        if cancel.load(Ordering::SeqCst) {
            return Err("导出已取消".into());
        }
        file.persist_noclobber(destination).map_err(err)?;
    }
    receipt.destination = destination;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import")
    }
    fn request(
        source: &Path,
        pass: &str,
        member: Option<usize>,
        destination: Option<PathBuf>,
    ) -> Request {
        Request {
            record: record(source, member, "ZIP".into(), &AtomicBool::new(false)).unwrap(),
            password_hex: hex(pass.as_bytes()),
            destination,
        }
    }
    #[test]
    fn changed_source_and_cancel_never_export() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input.zip");
        let output = dir.path().join("output.txt");
        std::fs::copy(base().join("legacy.zip"), &source).unwrap();
        let original = request(&source, "ForenHC2026!", Some(0), Some(output.clone()));
        let cancelled = execute(
            request(&source, "ForenHC2026!", Some(0), Some(output.clone())),
            &AtomicBool::new(true),
        );
        assert!(cancelled.is_err());
        assert!(!output.exists());
        std::fs::write(&source, b"changed original").unwrap();
        assert!(
            execute(original, &AtomicBool::new(false))
                .unwrap_err()
                .contains("改变")
        );
        assert!(!output.exists());
    }
    #[test]
    fn selected_member_does_not_claim_other_passwords() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mixed.zip");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, pass) in [("first.txt", "first"), ("second.txt", "second")] {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .with_aes_encryption(zip::AesMode::Aes256, pass);
            zip.start_file(name, options).unwrap();
            zip.write_all(b"Different passwords per entry").unwrap();
        }
        zip.finish().unwrap();
        let receipt = execute(
            request(&path, "first", Some(0), None),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(receipt.scope.contains("first.txt"));
        assert!(
            execute(
                request(&path, "first", Some(1), None),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(
            execute(
                request(&path, "second", Some(1), None),
                &AtomicBool::new(false)
            )
            .is_ok()
        );
    }
    #[test]
    fn tampered_zip_content_is_not_verified() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["aes.zip", "legacy.zip"] {
            let mut bytes = std::fs::read(base().join(name)).unwrap();
            let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
            bytes[central - 1] ^= 1;
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let member = if name == "aes.zip" { Some(1) } else { Some(0) };
            assert!(
                execute(
                    request(&path, "ForenHC2026!", member, None),
                    &AtomicBool::new(false)
                )
                .is_err(),
                "{name}"
            );
        }
    }
    #[test]
    fn export_paths_and_records_are_confined() {
        for name in [
            "../outside.txt",
            "/absolute.txt",
            "C:\\escape.txt",
            "folder/../../escape",
        ] {
            assert!(safe_name(name).is_err());
        }
        assert_eq!(safe_name("folder\\file.txt").unwrap(), "folder/file.txt");
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("existing.txt");
        std::fs::write(&output, "keep me").unwrap();
        assert!(
            execute(
                request(
                    &base().join("legacy.zip"),
                    "ForenHC2026!",
                    Some(0),
                    Some(output.clone())
                ),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(output).unwrap(), "keep me");
        assert!(load_record(&base().join("passwords.txt"), dir.path()).is_err());
    }
}

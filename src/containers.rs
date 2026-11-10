//! Read-only VeraCrypt file-container headers. No devices, drivers or mounting.
use cipher::{
    BlockDecrypt, BlockEncrypt, BlockSizeUser, KeyInit, consts::U16, generic_array::GenericArray,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Spec {
    pub prf: String,
    pub bits: u32,
    pub header: String,
    pub pim_start: u32,
    pub pim_stop: u32,
    pub keyfiles: Vec<PathBuf>,
}
impl Default for Spec {
    fn default() -> Self {
        Self {
            prf: "sha512".into(),
            bits: 512,
            header: "primary".into(),
            pim_start: 0,
            pim_stop: 0,
            keyfiles: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Keyfile {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Record {
    pub spec: Spec,
    pub offset: u64,
    pub header_sha256: String,
    pub source_size: u64,
    pub keyfiles: Vec<Keyfile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mode {
    pub mode: u32,
    pub label: String,
}
impl Spec {
    pub fn validate(&self) -> Result<()> {
        if !matches!(
            self.prf.as_str(),
            "auto" | "sha512" | "sha256" | "ripemd160" | "whirlpool" | "streebog" | "blake2s"
        ) || !matches!(self.bits, 0 | 512 | 1024 | 1536)
            || !matches!(
                self.header.as_str(),
                "primary" | "hidden" | "backup" | "hidden-backup"
            )
        {
            return Err("VeraCrypt 参数无效；仅支持文件容器的 PBKDF2 卷头".into());
        }
        if (self.pim_start == 0) != (self.pim_stop == 0)
            || self.pim_start > self.pim_stop
            || self.pim_stop > 65535
        {
            return Err("PIM 请留空使用默认值，或填写 1–65535 的完整范围".into());
        }
        if self.keyfiles.len() > 20
            || self
                .keyfiles
                .iter()
                .any(|p| p.to_string_lossy().contains([',', '\0', '\n', '\r']))
        {
            return Err("最多选择 20 个密钥文件；路径不能含英文逗号或换行".into());
        }
        Ok(())
    }
    fn prfs(&self) -> Vec<&str> {
        if self.prf == "auto" {
            vec!["sha512", "sha256"]
        } else {
            vec![&self.prf]
        }
    }
    pub fn modes(&self) -> Vec<Mode> {
        self.prfs()
            .into_iter()
            .flat_map(|prf| {
                let base = match prf {
                    "ripemd160" => 29410,
                    "sha512" => 29420,
                    "whirlpool" => 29430,
                    "sha256" => 29450,
                    "streebog" => 29470,
                    "blake2s" => 29490,
                    _ => unreachable!(),
                };
                let levels = if self.bits == 0 {
                    vec![1, 2, 3]
                } else {
                    vec![self.bits / 512]
                };
                levels.into_iter().map(move |n| Mode {
                    mode: base + n,
                    label: format!("{} / XTS {}", prf.to_uppercase(), n * 512),
                })
            })
            .collect()
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn regular(path: &Path) -> Result<File> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("请选择普通容器文件，不支持物理设备或分区".into());
    }
    Ok(file)
}
pub fn header(path: &Path, spec: &Spec) -> Result<([u8; 512], u64, u64)> {
    spec.validate()?;
    let mut file = regular(path)?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let offset = match spec.header.as_str() {
        "primary" => 0,
        "hidden" => 65536,
        "backup" => size
            .checked_sub(131072)
            .filter(|_| size >= 262144)
            .ok_or("文件太短，无法定位普通卷备份卷头")?,
        "hidden-backup" => size
            .checked_sub(65536)
            .filter(|_| size >= 262144)
            .ok_or("文件太短，无法定位隐藏卷备份卷头")?,
        _ => unreachable!(),
    };
    if offset.checked_add(512).is_none_or(|end| end > size) {
        return Err("所选卷头超出文件范围".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut data = [0u8; 512];
    file.read_exact(&mut data).map_err(|e| e.to_string())?;
    Ok((data, offset, size))
}
fn keyfile_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    regular(path)?
        .take(1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("密钥文件为空，请重新选择".into());
    }
    Ok(bytes)
}
pub fn record(path: &Path, mut spec: Spec, cancel: &AtomicBool) -> Result<Record> {
    for keyfile in &mut spec.keyfiles {
        *keyfile = keyfile.canonicalize().map_err(|e| e.to_string())?;
        // Rust canonicalization adds a Win32 verbatim prefix. Hashcat's keyfile
        // path parser does not accept it, so retain the absolute conventional path.
        #[cfg(windows)]
        {
            let text = keyfile.to_str().ok_or("密钥文件路径不是有效 Unicode")?;
            if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
                *keyfile = PathBuf::from(format!("\\\\{unc}"));
            } else if let Some(local) = text.strip_prefix("\\\\?\\") {
                *keyfile = PathBuf::from(local);
            }
        }
    }
    let (bytes, offset, size) = header(path, &spec)?;
    let mut keyfiles = Vec::new();
    for path in &spec.keyfiles {
        if cancel.load(Ordering::SeqCst) {
            return Err("操作已取消".into());
        }
        let bytes = keyfile_bytes(path)?;
        keyfiles.push(Keyfile {
            path: path.clone(),
            sha256: hex(&Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
        });
    }
    Ok(Record {
        spec,
        offset,
        header_sha256: hex(&Sha256::digest(bytes)),
        source_size: size,
        keyfiles,
    })
}
pub fn extract(path: &Path, spec: &Spec) -> Result<crate::import::Extracted> {
    let (data, _, _) = header(path, spec)?;
    Ok(crate::import::Extracted {
        hash: format!("$veracrypt${}${}", hex(&data[..64]), hex(&data[64..])),
        mode: spec.modes()[0].mode,
        format: "VeraCrypt · PBKDF2 卷头".into(),
        note: "已读取所选卷头。参数需与容器创建设置一致；未挂载或修改容器。".into(),
        members: vec![],
        selected: None,
    })
}
fn effective_password(password: &[u8], spec: &Spec) -> Result<(Vec<u8>, Option<u32>)> {
    if spec.pim_start == 0 {
        return Ok((password.to_vec(), None));
    }
    if spec.pim_start == spec.pim_stop {
        // Hashcat adds a PIM annotation even for a fixed non-default PIM.
        if !password.ends_with(b")") {
            return Ok((password.to_vec(), Some(spec.pim_start)));
        }
    }
    if let Some(pos) = password.windows(8).rposition(|w| w == b"   (PIM=") {
        let n = password
            .get(pos + 8..password.len().saturating_sub(1))
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| s.parse::<u32>().ok());
        if password.ends_with(b")")
            && let Some(n) = n
            && (spec.pim_start..=spec.pim_stop).contains(&n)
        {
            return Ok((password[..pos].to_vec(), Some(n)));
        }
    }
    if spec.pim_start == spec.pim_stop {
        Ok((password.to_vec(), Some(spec.pim_start)))
    } else {
        Err("此结果缺少 PIM 值，请使用引擎报告的 PIM 或设为单一 PIM 后验证".into())
    }
}
fn mix_keyfiles(password: &[u8], spec: &Spec) -> Result<Vec<u8>> {
    if password.len() > 128 {
        return Err("VeraCrypt 密码超过 128 字节".into());
    }
    if spec.keyfiles.is_empty() {
        return Ok(password.to_vec());
    }
    let length = if password.len() > 64 { 128 } else { 64 };
    let mut pool = vec![0u8; length];
    for path in &spec.keyfiles {
        let mut crc = u32::MAX;
        let mut at = 0;
        for byte in keyfile_bytes(path)? {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320u32.wrapping_mul(crc & 1));
            }
            for value in crc.to_be_bytes() {
                pool[at] = pool[at].wrapping_add(value);
                at = (at + 1) % length;
            }
        }
    }
    for (i, byte) in password.iter().enumerate() {
        pool[i] = pool[i].wrapping_add(*byte);
    }
    Ok(pool)
}
fn derive(prf: &str, pass: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    match prf {
        "sha512" => pbkdf2::pbkdf2_hmac::<Sha512>(pass, salt, iterations, output),
        "sha256" => pbkdf2::pbkdf2_hmac::<Sha256>(pass, salt, iterations, output),
        "ripemd160" => pbkdf2::pbkdf2_hmac::<ripemd::Ripemd160>(pass, salt, iterations, output),
        "whirlpool" => pbkdf2::pbkdf2_hmac::<whirlpool::Whirlpool>(pass, salt, iterations, output),
        "streebog" => pbkdf2::pbkdf2_hmac::<streebog::Streebog512>(pass, salt, iterations, output),
        "blake2s" => {
            pbkdf2::pbkdf2::<hmac::SimpleHmac<blake2::Blake2s256>>(pass, salt, iterations, output)
                .expect("HMAC accepts any key")
        }
        _ => unreachable!(),
    }
}
fn xts<C: BlockEncrypt + BlockDecrypt + KeyInit + BlockSizeUser<BlockSize = U16>>(
    data: &mut [u8],
    primary: &[u8],
    secondary: &[u8],
) {
    let first = C::new_from_slice(primary).unwrap();
    let second = C::new_from_slice(secondary).unwrap();
    let mut tweak = GenericArray::default();
    second.encrypt_block(&mut tweak);
    for chunk in data.chunks_exact_mut(16) {
        let mut block = GenericArray::clone_from_slice(chunk);
        for (v, t) in block.iter_mut().zip(&tweak) {
            *v ^= t;
        }
        first.decrypt_block(&mut block);
        for ((v, b), t) in chunk.iter_mut().zip(block).zip(&tweak) {
            *v = b ^ t;
        }
        let mut carry = 0;
        for v in tweak.iter_mut() {
            let next = *v >> 7;
            *v = (*v << 1) | carry;
            carry = next;
        }
        if carry != 0 {
            tweak[0] ^= 0x87;
        }
    }
}
fn decrypt(name: usize, data: &mut [u8], primary: &[u8], secondary: &[u8]) {
    match name {
        0 => xts::<aes::Aes256>(data, primary, secondary),
        1 => xts::<serpent::Serpent>(data, primary, secondary),
        2 => xts::<twofish::Twofish>(data, primary, secondary),
        3 => xts::<camellia::Camellia256>(data, primary, secondary),
        4 => xts::<kuznyechik::Kuznyechik>(data, primary, secondary),
        _ => unreachable!(),
    }
}
fn valid(data: &[u8]) -> bool {
    data.len() == 448
        && &data[..4] == b"VERA"
        && crc32fast::hash(&data[192..448]) == u32::from_be_bytes(data[8..12].try_into().unwrap())
        && crc32fast::hash(&data[..188]) == u32::from_be_bytes(data[188..192].try_into().unwrap())
}
fn try_ciphers(data: &[u8], keys: &[u8], depth: usize) -> Option<String> {
    const NAMES: [&str; 5] = ["AES", "Serpent", "Twofish", "Camellia", "Kuznyechik"];
    fn visit(data: &[u8], keys: &[u8], depth: usize, names: &mut Vec<usize>) -> Option<String> {
        if names.len() == depth {
            for reverse in [true, false] {
                let mut plain = data.to_vec();
                for j in 0..depth {
                    let i = if reverse { depth - 1 - j } else { j };
                    decrypt(
                        names[i],
                        &mut plain,
                        &keys[i * 32..i * 32 + 32],
                        &keys[(depth + i) * 32..(depth + i + 1) * 32],
                    );
                }
                if valid(&plain) {
                    return Some(
                        names
                            .iter()
                            .map(|n| NAMES[*n])
                            .collect::<Vec<_>>()
                            .join(" / "),
                    );
                }
                if depth == 1 {
                    break;
                }
            }
            return None;
        }
        for n in 0..5 {
            if !names.contains(&n) {
                names.push(n);
                if let Some(value) = visit(data, keys, depth, names) {
                    return Some(value);
                }
                names.pop();
            }
        }
        None
    }
    visit(data, keys, depth, &mut Vec::new())
}
pub fn verify(
    path: &Path,
    expected: &Record,
    password: &[u8],
    cancel: &AtomicBool,
) -> Result<String> {
    if record(path, expected.spec.clone(), cancel)? != *expected {
        return Err("卷头、容器大小或密钥文件已改变，请重新导入".into());
    }
    let (data, _, _) = header(path, &expected.spec)?;
    let (pass, pim) = effective_password(password, &expected.spec)?;
    let pass = mix_keyfiles(&pass, &expected.spec)?;
    let depths = if expected.spec.bits == 0 {
        vec![1, 2, 3]
    } else {
        vec![expected.spec.bits as usize / 512]
    };
    let mut success = None;
    for prf in expected.spec.prfs() {
        if cancel.load(Ordering::SeqCst) {
            return Err("操作已取消".into());
        }
        let iterations = pim
            .map(|n| 15000 + n * 1000)
            .unwrap_or(if prf == "ripemd160" { 655331 } else { 500000 });
        let mut keys = vec![0u8; depths.iter().max().unwrap() * 64];
        derive(prf, &pass, &data[..64], iterations, &mut keys);
        for depth in &depths {
            if let Some(name) = try_ciphers(&data[64..], &keys, *depth) {
                success = Some(format!(
                    "PBKDF2-{} / {name}-XTS / VERA + 2×CRC32{}",
                    prf.to_uppercase(),
                    pim.map(|p| format!(" / PIM {p}")).unwrap_or_default()
                ));
                break;
            }
        }
        keys.fill(0);
        if success.is_some() {
            break;
        }
    }
    if cancel.load(Ordering::SeqCst) {
        return Err("操作已取消".into());
    }
    if record(path, expected.spec.clone(), cancel)? != *expected {
        return Err("验证期间卷头或密钥文件发生改变，结果已作废".into());
    }
    success
        .ok_or_else(|| "密码或容器参数不匹配。请核对卷头、PRF、级联长度、PIM 和密钥文件。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offsets_and_modes_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("volume.hc");
        std::fs::write(&p, vec![1u8; 524288]).unwrap();
        for (name, offset) in [
            ("primary", 0),
            ("hidden", 65536),
            ("backup", 393216),
            ("hidden-backup", 458752),
        ] {
            let s = Spec {
                header: name.into(),
                ..Default::default()
            };
            assert_eq!(header(&p, &s).unwrap().1, offset);
        }
        let s = Spec {
            prf: "auto".into(),
            bits: 0,
            ..Default::default()
        };
        assert_eq!(
            s.modes().iter().map(|v| v.mode).collect::<Vec<_>>(),
            [29421, 29422, 29423, 29451, 29452, 29453]
        );
        assert!(
            Spec {
                pim_start: 2,
                pim_stop: 1,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Spec {
                prf: "argon2id".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn keyfile_pool_and_pim_annotation() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("key");
        std::fs::write(&key, b"123456789").unwrap();
        let s = Spec {
            pim_start: 1,
            pim_stop: 10,
            keyfiles: vec![key],
            ..Default::default()
        };
        let (p, pim) = effective_password(b"test   (PIM=3)", &s).unwrap();
        assert_eq!(p, b"test");
        assert_eq!(pim, Some(3));
        let a = mix_keyfiles(b"test", &s).unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(&a[..4], b"test");
        assert!(effective_password(b"test   (PIM=11)", &s).is_err());
    }
}

//! Native, bounded verifier extraction. Format references: RAR technote,
//! 7-Zip 7zFormat.txt and Hashcat modules 11600/12500/13000/23700/23800.
use super::*;
use sha2::{Digest, Sha256};
struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let b = bytes(self.b, self.p, n)?;
        self.p += n;
        Ok(b)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn rar(&mut self) -> Result<u64> {
        let mut n = 0u64;
        for i in 0..10 {
            let b = self.byte()?;
            if i == 9 && b > 1 {
                return Err("RAR 整数溢出".into());
            }
            n |= u64::from(b & 127) << (7 * i);
            if b & 128 == 0 {
                return Ok(n);
            }
        }
        Err("RAR 整数过长".into())
    }
    fn seven(&mut self) -> Result<u64> {
        let first = self.byte()?;
        let mut mask = 128u8;
        let mut n = 0;
        for i in 0..8 {
            if first & mask == 0 {
                return Ok(n | (u64::from(first & (mask - 1)) << (8 * i)));
            }
            n |= u64::from(self.byte()?) << (8 * i);
            mask >>= 1;
        }
        Ok(n)
    }
    fn expect(&mut self, v: u8) -> Result<()> {
        if self.byte()? != v {
            return Err("不支持的 7z 流结构".into());
        }
        Ok(())
    }
}
pub(super) fn rar(f: &mut File, member: Option<usize>) -> Result<Extracted> {
    if read_at(f, 0, 8)? == b"Rar!\x1a\x07\x01\x00" {
        rar5(f)
    } else {
        rar3(f, member)
    }
}
fn rar3(f: &mut File, member: Option<usize>) -> Result<Extracted> {
    if read_at(f, 0, 7)? != b"Rar!\x1a\x07\x00" {
        return Err("未知 RAR 版本".into());
    }
    let main = read_at(f, 7, 13)?;
    if main[2] != 0x73 {
        return Err("RAR 主头无效".into());
    }
    let flags = u16le(&main, 3)?;
    if flags & 1 != 0 {
        return Err("暂不支持分卷 RAR".into());
    }
    let head = u16le(&main, 5)? as u64;
    if head < 13 {
        return Err("RAR 主头过短".into());
    }
    if flags & 128 != 0 {
        let len = f.metadata().map_err(fail)?.len();
        if len < 7 + head + 24 {
            return Err("RAR 加密头不完整".into());
        }
        let end = read_at(f, len - 24, 24)?;
        return Ok(extracted(
            format!("$RAR3$*0*{}*{}", hex(&end[..8]), hex(&end[8..])),
            12500,
            "RAR3 · 加密文件名",
        ));
    }
    let mut offset = 7 + head;
    let len = f.metadata().map_err(fail)?.len();
    let mut found = vec![];
    let mut index = 0;
    for _ in 0..20000 {
        if offset >= len {
            break;
        }
        let base = read_at(f, offset, 7)?;
        let flags = u16le(&base, 3)?;
        let size = u16le(&base, 5)? as usize;
        if size < 7 {
            return Err("RAR 文件头过短".into());
        }
        let h = read_at(f, offset, size)?;
        if (crc32fast::hash(&h[2..]) & 65535) as u16 != u16le(&h, 0)? {
            return Err("RAR 文件头 CRC 错误".into());
        }
        if base[2] == 0x7b {
            break;
        }
        let mut packed = if flags & 0x8000 != 0 {
            u32le(&h, 7)? as u64
        } else {
            0
        };
        if base[2] == 0x74 {
            let mut plain = u32le(&h, 11)? as u64;
            let method = bytes(&h, 25, 1)?[0];
            let mut pos = 32;
            if flags & 0x100 != 0 {
                packed |= (u32le(&h, pos)? as u64) << 32;
                plain |= (u32le(&h, pos + 4)? as u64) << 32;
                pos += 8;
            }
            let name = bytes(&h, pos, u16le(&h, 26)? as usize)?;
            pos += name.len();
            let name = String::from_utf8_lossy(name.split(|b| *b == 0).next().unwrap_or(name))
                .into_owned();
            if flags & 4 != 0
                && flags & 0x400 != 0
                && flags & 0x13 == 0
                && (0x30..=0x35).contains(&method)
                && packed > 0
                && packed <= 327680
                && plain > 0
                && plain <= 655360
                && packed % 16 == 0
            {
                let salt = hex(bytes(&h, pos, 8)?);
                let crc = hex(bytes(&h, 16, 4)?);
                let mode = if method == 0x30 { 23700 } else { 23800 };
                found.push((
                    Member {
                        index,
                        name,
                        mode,
                        size: plain,
                    },
                    offset + size as u64,
                    packed,
                    plain,
                    salt,
                    crc,
                    method,
                ));
            }
            index += 1;
        }
        offset = offset
            .checked_add(size as u64)
            .and_then(|v| v.checked_add(packed))
            .ok_or("RAR 偏移溢出")?;
        if offset > len {
            return Err("RAR 数据越界".into());
        }
    }
    let chosen = if let Some(index) = member {
        found
            .iter()
            .position(|v| v.0.index == index)
            .ok_or("所选 RAR 项目无法提取")?
    } else {
        found
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.2)
            .map(|v| v.0)
            .ok_or("未找到可提取的 RAR3 加密项；固实后续项、大文件、分卷或旧加密可能不支持")?
    };
    let (entry, offset, packed, plain, salt, crc, method) = &found[chosen];
    let payload = read_at(f, *offset, *packed as usize)?;
    let mut result = extracted(
        format!(
            "$RAR3$*1*{salt}*{crc}*{packed}*{plain}*1*{}*{method:02x}",
            hex(&payload)
        ),
        entry.mode,
        "RAR3",
    );
    result.selected = Some(entry.index);
    result.members = found.into_iter().map(|v| v.0).collect();
    Ok(result)
}
fn rar5_crypto(c: &mut Cursor<'_>, iv_present: bool) -> Result<String> {
    if c.rar()? != 0 {
        return Err("不支持的 RAR5 加密版本".into());
    }
    let flags = c.rar()?;
    if flags & 1 == 0 {
        return Err("RAR5 缺少密码校验数据，暂不能生成恢复任务".into());
    }
    let kdf = c.byte()?;
    if kdf > 24 {
        return Err("RAR5 KDF 超出当前 Hashcat 支持范围".into());
    }
    let salt = c.take(16)?;
    let iv = if iv_present {
        c.take(16)?.to_vec()
    } else {
        vec![0; 16]
    };
    let check = c.take(12)?;
    if Sha256::digest(&check[..8])[..4] != check[8..] {
        return Err("RAR5 密码校验字段损坏".into());
    }
    Ok(format!(
        "$rar5$16${}${kdf}${}$8${}",
        hex(salt),
        hex(&iv),
        hex(&check[..8])
    ))
}
fn rar5(f: &mut File) -> Result<Extracted> {
    let mut offset = 8;
    let len = f.metadata().map_err(fail)?.len();
    for _ in 0..20000 {
        if offset >= len {
            break;
        }
        let base = read_at(f, offset, 7)?;
        let mut c = Cursor::new(&base[4..]);
        let size = c.rar()?;
        let prefix = 4 + c.p;
        if !(2..=2 * 1024 * 1024).contains(&size) {
            return Err("RAR5 文件头大小无效".into());
        }
        let h = read_at(f, offset, prefix + size as usize)?;
        if crc32fast::hash(&h[4..]) != u32le(&h, 0)? {
            return Err("RAR5 文件头 CRC 错误".into());
        }
        let mut c = Cursor::new(&h[prefix..]);
        let kind = c.rar()?;
        let flags = c.rar()?;
        if flags & 0x18 != 0 {
            return Err("暂不支持分卷 RAR5".into());
        }
        let extra = if flags & 1 != 0 { c.rar()? as usize } else { 0 };
        let data = if flags & 2 != 0 { c.rar()? } else { 0 };
        if extra > size as usize - c.p {
            return Err("RAR5 附加区越界".into());
        }
        if kind == 4 {
            return Ok(extracted(
                rar5_crypto(&mut c, false)?,
                13000,
                "RAR5 · 加密文件名",
            ));
        }
        if kind == 1 && c.rar()? & 1 != 0 {
            return Err("暂不支持分卷 RAR5".into());
        }
        if kind == 2 && extra > 0 {
            let mut e = Cursor::new(&h[h.len() - extra..]);
            while e.p < e.b.len() {
                let size = e.rar()? as usize;
                let mut record = Cursor::new(e.take(size)?);
                if record.rar()? == 1 {
                    return Ok(extracted(rar5_crypto(&mut record, true)?, 13000, "RAR5"));
                }
            }
        }
        if kind == 5 {
            break;
        }
        offset = offset
            .checked_add(h.len() as u64)
            .and_then(|v| v.checked_add(data))
            .ok_or("RAR5 偏移溢出")?;
        if offset > len {
            return Err("RAR5 数据越界".into());
        }
    }
    Err("没有找到受支持的 RAR5 加密项".into())
}

#[derive(Clone)]
struct Coder {
    id: Vec<u8>,
    props: Vec<u8>,
    unpack: u64,
}
fn seven_hash(
    f: &mut File,
    offset: u64,
    packed: u64,
    coders: &[Coder],
    crc: u32,
    crc_len: u64,
) -> Result<Extracted> {
    let aes = coders.first().ok_or("7z 缺少编码器")?;
    if aes.id != [6, 0xf1, 7, 1] || coders.len() > 2 {
        return Err("暂不支持该 7z 编码链；当前支持 AES、AES+LZMA/LZMA2".into());
    }
    let props = &aes.props;
    let first = *props.first().ok_or("7z AES 参数缺失")?;
    let cycles = first & 63;
    if cycles > 24 {
        return Err("7z KDF 超出当前支持范围".into());
    }
    let (salt_len, iv_len, start) = if first & 0xc0 != 0 {
        let b = *props.get(1).ok_or("7z AES 参数过短")?;
        (
            (((first >> 7) & 1) + (b >> 4)) as usize,
            (((first >> 6) & 1) + (b & 15)) as usize,
            2,
        )
    } else {
        (0, 0, 1)
    };
    // Hashcat 11600 treats salt as a literal field and currently accepts only short salts.
    if salt_len != 0 {
        return Err("当前 Hashcat 11600 的盐字段限制不支持此带盐 7z，请使用外部提取器确认".into());
    }
    let salt = bytes(props, start, salt_len)?;
    let iv_raw = bytes(props, start + salt_len, iv_len)?;
    let mut iv = [0; 16];
    if iv_len > 16 {
        return Err("7z IV 过长".into());
    }
    iv[..iv_len].copy_from_slice(iv_raw);
    if packed == 0
        || packed > 8 * 1024 * 1024
        || !packed.is_multiple_of(16)
        || aes.unpack > packed
        || crc_len == 0
        || crc_len > 8 * 1024 * 1024
    {
        return Err("7z 验证块超过当前 8 MiB 上限或结构无效".into());
    }
    let (kind, attributes) = if let Some(coder) = coders.get(1) {
        match coder.id.as_slice() {
            [0] if coder.props.is_empty() && coder.unpack == aes.unpack => (0, String::new()),
            [3, 1, 1] if coder.props.len() == 5 => (1, hex(&coder.props)),
            [0x21] if coder.props.len() == 1 => (2, hex(&coder.props)),
            _ => return Err("暂不支持此 7z 压缩或过滤编码器".into()),
        }
    } else {
        (0, String::new())
    };
    if kind == 0 && crc_len != aes.unpack {
        return Err("7z 原始块 CRC 范围不完整".into());
    }
    let payload = read_at(f, offset, packed as usize)?;
    let mut hash = format!(
        "$7z${kind}${cycles}${salt_len}${}${iv_len}${}${crc}${packed}${}${}",
        hex(salt),
        hex(&iv),
        aes.unpack,
        hex(&payload)
    );
    if kind != 0 {
        hash.push_str(&format!("${crc_len}${attributes}"));
    }
    Ok(extracted(hash, 11600, "7z"))
}
fn digests(c: &mut Cursor<'_>, count: usize) -> Result<Vec<Option<u32>>> {
    let all = c.byte()? != 0;
    let bits = if all {
        vec![]
    } else {
        c.take(count.div_ceil(8))?.to_vec()
    };
    (0..count)
        .map(|i| {
            if all || bits[i / 8] & (128 >> (i % 8)) != 0 {
                Ok(Some(u32::from_le_bytes(c.take(4)?.try_into().unwrap())))
            } else {
                Ok(None)
            }
        })
        .collect()
}
fn encoded_seven(f: &mut File, h: &[u8]) -> Result<Extracted> {
    let mut c = Cursor::new(h);
    c.expect(0x17)?;
    c.expect(6)?;
    let pos = c.seven()?;
    if c.seven()? != 1 {
        return Err("暂不支持多流 7z 加密头".into());
    }
    c.expect(9)?;
    let packed = c.seven()?;
    let mut next = c.byte()?;
    if next == 10 {
        digests(&mut c, 1)?;
        next = c.byte()?;
    }
    if next != 0 {
        return Err("7z PackInfo 无效".into());
    }
    c.expect(7)?;
    c.expect(0xb)?;
    if c.seven()? != 1 || c.byte()? != 0 {
        return Err("暂不支持外部或多块 7z 加密头".into());
    }
    let count = c.seven()? as usize;
    if count == 0 || count > 2 {
        return Err("不支持此 7z 加密头编码链".into());
    }
    let mut coders = vec![];
    for _ in 0..count {
        let flags = c.byte()?;
        if flags & 0x90 != 0 {
            return Err("暂不支持复杂 7z 编码器".into());
        }
        let id = c.take((flags & 15) as usize)?.to_vec();
        let props = if flags & 0x20 != 0 {
            let n = c.seven()? as usize;
            if n > 64 {
                return Err("7z 编码参数过长".into());
            }
            c.take(n)?.to_vec()
        } else {
            vec![]
        };
        coders.push(Coder {
            id,
            props,
            unpack: 0,
        });
    }
    // Only the ordinary single-input linear decoding chain is accepted.
    for i in 0..count - 1 {
        if c.seven()? != i as u64 + 1 || c.seven()? != i as u64 {
            return Err("不支持此 7z 绑定关系".into());
        }
    }
    c.expect(0xc)?;
    for coder in &mut coders {
        coder.unpack = c.seven()?;
    }
    c.expect(10)?;
    let crc = digests(&mut c, 1)?[0].ok_or("7z 加密头没有 CRC")?;
    c.expect(0)?;
    c.expect(0)?;
    let crc_len = coders.last().unwrap().unpack;
    let mut result = seven_hash(
        f,
        32u64.checked_add(pos).ok_or("7z 偏移溢出")?,
        packed,
        &coders,
        crc,
        crc_len,
    )?;
    result.format = "7z · 加密文件名".into();
    Ok(result)
}
pub(super) fn seven(f: &mut File) -> Result<Extracted> {
    let start = read_at(f, 0, 32)?;
    if crc32fast::hash(&start[12..]) != u32le(&start, 8)? {
        return Err("7z 起始头 CRC 错误".into());
    }
    let offset = 32u64.checked_add(u64le(&start, 12)?).ok_or("7z 偏移溢出")?;
    let size = u64le(&start, 20)?;
    if size == 0 || size > 2 * 1024 * 1024 {
        return Err("7z 文件头超过 2 MiB 或为空".into());
    }
    let header = read_at(f, offset, size as usize)?;
    if crc32fast::hash(&header) != u32le(&start, 28)? {
        return Err("7z 文件头 CRC 错误".into());
    }
    // Encrypted encoded headers cannot be decoded without a password. Extract their
    // AES block directly; ordinary compressed headers go through the bounded reader.
    if header[0] == 0x17 {
        match encoded_seven(f, &header) {
            Ok(result) => return Ok(result),
            Err(error) if header.windows(4).any(|s| s == [6, 0xf1, 7, 1]) => return Err(error),
            Err(_) => {}
        }
    }
    f.seek(SeekFrom::Start(0)).map_err(fail)?;
    let archive = sevenz_rust2::Archive::read_with_memory_limit(
        f,
        &sevenz_rust2::Password::empty(),
        128 * 1024,
    )
    .map_err(fail)?;
    for (block_index, block) in archive.blocks.iter().enumerate() {
        let ordered = block.ordered_coder_iter().collect::<Vec<_>>();
        if ordered.len() != block.coders.len()
            || ordered
                .first()
                .is_none_or(|(_, c)| c.encoder_method_id() != [6, 0xf1, 7, 1])
        {
            continue;
        }
        let first = archive.stream_map.block_first_pack_stream_index()[block_index];
        let next = archive
            .stream_map
            .block_first_pack_stream_index()
            .get(block_index + 1)
            .copied()
            .unwrap_or(archive.pack_sizes().len());
        if next - first != 1 {
            continue;
        }
        let entry = archive.files.iter().enumerate().find(|(i, e)| {
            archive.stream_map.file_block_index[*i] == Some(block_index) && e.has_stream
        });
        let (crc, crc_len) = if block.has_crc {
            (block.crc as u32, block.get_unpack_size())
        } else if let Some((_, entry)) = entry.filter(|(_, e)| e.has_crc) {
            (entry.crc as u32, entry.size)
        } else {
            continue;
        };
        let coders = ordered
            .iter()
            .map(|(i, c)| Coder {
                id: c.encoder_method_id().to_vec(),
                props: c.properties().to_vec(),
                unpack: block.get_unpack_size_at_index(*i),
            })
            .collect::<Vec<_>>();
        let pos = 32u64
            .checked_add(archive.pack_pos())
            .and_then(|v| v.checked_add(archive.stream_map.pack_stream_offsets()[first]))
            .ok_or("7z 偏移溢出")?;
        return seven_hash(f, pos, archive.pack_sizes()[first], &coders, crc, crc_len);
    }
    Err("未找到受支持的 7z 加密块；可能没有加密或使用暂不支持的编码链".into())
}

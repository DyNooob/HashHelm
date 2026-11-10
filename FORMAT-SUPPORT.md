# 文件支持与验证范围 · 0.8

HashHelm 使用 Hashcat 进行候选搜索，独立验证器负责用找到的密码读取原文件。两者的状态分别显示；导入成功、Hashcat 匹配、原文件验证通过是不同阶段。

| 文件 | 本机提取 / Hashcat 模式 | 原文件验证 | 导出 |
| --- | --- | --- | --- |
| ZIP / ZIP64 ZipCrypto Stored / Deflate | 17210 / 17200；所选项压缩后不超过 300 KiB | 完整读取所选项并检查 CRC32 | 所选项的解密文件 |
| ZIP / ZIP64 WinZip AES | 13600；所选项不超过 8 MiB | 完整读取所选项，检查 AES 认证 | 所选项的解密文件 |
| RAR3 内容加密 | Stored 23700 / 压缩 23800；密文 ≤327680 B，原文 ≤655360 B；跳过固实后续项 | UnRAR 校验全部文件 | 无密码 ZIP 副本 |
| RAR3 文件名加密 | 12500 | UnRAR 校验全部文件 | 无密码 ZIP 副本 |
| RAR5 内容 / 文件名加密 | 13000；需密码检查字段及其有效校验和 | UnRAR 校验全部文件 | 无密码 ZIP 副本 |
| 7z AES + Copy / LZMA / LZMA2 | 11600；包括常见文件名加密；不带盐、单流线性编码链，验证块及 CRC 范围 ≤8 MiB | 解压并逐项检查 CRC32 | 无密码 ZIP 副本 |
| Office 2007/2010 标准、Agile | 9400 / 9500 / 9600 | 实际解密并完整读取 OOXML ZIP；Agile 按库默认策略检查完整性 | 解密文档 |
| 部分旧 Word / Excel RC4 | 9800 等（以导入结果为准） | 密码校验、解密及 CFB 结构检查 | 解密文档 |
| PDF Standard Security R2–R6 | 10400 / 10500 / 10510 / 10600 / 10700 | 密码验证及对象解密 | 解密 PDF |
| 外部哈希文本 | 用户确认算法 | 没有原文件记录时不自动验证 | 可复制匹配密码 |

RAR/7z 的 ZIP 副本仅保留文件内容和相对名称，不保留原始压缩格式、时间戳、权限及其他归档元数据。拒绝不安全的相对路径；不会把归档路径直接解压到磁盘。ZIP 仅验证所选项目，不能推断其他项目的密码。

文件内容校验的强度取决于格式。CRC32 不是密码学完整性认证，PDF 和部分旧 Office 没有全文件认证。原文件 SHA-256 用于检查相对于导入时是否变化，不能证明导入之前没有被修改。

## 资源和操作限制

- 不支持分卷、自解压、证书/IRM、特殊或多流 7z 编码链。对于没有密码检查字段的 RAR5、带盐 7z 等会明确拒绝，不生成猜测的验证数据。
- ZIP 中央目录 ≤16 MiB，最多 20000 项；7z 文件头 ≤2 MiB。ZIP64 解决 64 位偏移/大小读取，不解除 Hashcat 验证块限制。
- 独立验证子进程限时 120 秒，可随时取消。7z 解码器内存配置为 128 MiB，单线程读取；归档输出总量 ≤1 GiB、最多 20000 项；RAR 导出单项 ≤128 MiB；Office/PDF ≤128 MiB。
- 原文件按 SHA-256 在导入前后、验证前后核对。失败不生成“已验证”记录；导出再次验证并以不覆盖方式提交新文件。
- 旧任务需要重新导入原文件，才能创建本版的验证记录。

## 固定回归样本

`tests/fixtures/import` 仅含合成和公开许可的测试数据。覆盖 ZIP/ZIP64、RAR3/RAR5 内容/文件名加密、7z Copy/LZMA/LZMA2、Office 标准/Agile/旧 RC4、PDF R2–R6。来源见该目录上级 README 和 THIRD_PARTY.md。

```sh
cargo build --locked --bin extract-check --bin verify-check
python3 scripts/file-verification-tests.py
# 需要本机可用的 Hashcat 和计算后端；HASHHELM_ENGINE 可指定原生引擎。
python3 scripts/archive-recovery-tests.py
```

前一项检查错误密码、正确密码、独立子进程、解密导出、禁止覆盖和原文件变化；后一项实际搜索新增归档的已知测试密码。Windows 已运行，macOS/Linux 的同一验证流程纳入四个原生 CI 任务，尚待对应机器执行结果。

## VeraCrypt 文件容器（0.11）

使用 Hashcat 8.0 新文本模式 `$veracrypt$盐$加密卷头`：RIPEMD-160 29411–29413、SHA-512 29421–29423、Whirlpool 29431–29433、SHA-256 29451–29453、Streebog 29471–29473、BLAKE2s 29491–29493。后缀 1/2/3 对应 XTS 总密钥长度 512/1024/1536 bit。模式会尝试对应级联长度的受支持算法，不从随机卷头猜测 PRF 或加密算法。

支持普通、隐藏、普通备份和隐藏备份卷头；默认只读 512 字节及密钥文件前 1 MiB，不为导入而扫描整个大型容器。按用户所选参数准备输入，并按目标匹配 Hashcat 模式。密钥文件和 PIM 会明确出现在参数预览中，PIM 注记与密码分开显示和复制。

原文件验证支持这些 PBKDF2 PRF 和 AES、Serpent、Twofish、Camellia、Kuznyechik 单算法及其两/三级联，通过 VERA 和双 CRC32 验证卷头。指纹范围明确为卷头，不验证数据区，不导出解密磁盘映像。物理设备、系统/boot 模式、Argon2id、TrueCrypt 兼容卷及未知密钥文件内容不在此界面的支持范围，完整命令仍保留 Hashcat 本身的能力。

验证样本为公开 Hashcat 8.0 ST_HASH/ST_PASS 和 OpenSSL 生成的合成卷头，来源记录在 tests/fixtures/containers/manifest.json；未使用用户案件。

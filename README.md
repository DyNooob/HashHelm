# HashHelm

A lightweight password recovery workbench built with Rust and Hashcat for CTFs and digital forensics.

**Originally developed by DyNooob.**

DigiForensics: https://digiforensics.cn

Software releases / Foren DEV: https://www.foren.dev

Original Project: https://github.com/DyNooob/HashHelm

HashHelm 是 DigiForensics 的轻量 Hashcat 图形工作台。通过文件分析、线索组合、可排序的恢复计划和真实引擎进度，降低密码恢复的使用门槛，同时为高级用户保留字面参数编辑和设备控制。

## 功能

- 分析常见加密 ZIP/ZIP64、RAR3/RAR5、7z、Word、Excel、PowerPoint 和 PDF，提取密码验证信息；支持导入已有哈希。原文件保持只读。
- VeraCrypt 文件容器恢复：普通/隐藏/备份卷头、六种 PBKDF2 PRF、XTS 单算法及双/三级联、PIM 和密钥文件；使用 Hashcat 8.0 的 294xx 模式，恢复后独立校验卷头。
- 独立原文件验证、导出解密副本和不含密码的验证记录；核对 SHA-256，拒绝覆盖已有目标。
- 从姓名、生日、关键词生成有限、去重的候选，支持中文姓名拼音和首字母。
- 自定义字典、规则、字符集、掩码、长度范围及固定前后缀，组合并调整多个恢复阶段。
- 显示引擎报告的处理量、总量、拒绝数、速度、设备状态、候选窗口、基础工作位置和预计完成时间。
- 原生检查点续跑：查看保存时间与基础位置，调整续跑时长和设备，跳过已完成阶段；核对输入文件大小与修改时间。
- 原生 `--total-candidates` / `--keyspace` 计数与 `--lookup` 示例覆盖检查，不启动恢复计算。
- PCFG 支持 hints 线索词表、密码形状、长度范围与 OMEN 开关，默认保留引擎自动调优。
- 保存日志与最近进度快照，支持回看、导出和计划复用。
- 按物理设备调度任务队列：不同设备可并行，同一显卡的 CUDA/OpenCL 别名共用占用；支持排序、暂停派发、单任务取消和中断后手动继续。
- 运行前查看候选量和规则放大倍数，统计外部输入，并使用独立会话测速；未知范围与算法基准估算均明确标注。
- 设备诊断、可操作的运行时提示、诊断报告导出，以及策略模板的本地保存和 JSON 导入/导出。
- 多设备选择、内核参数、八组自定义字符集、规则链式/合并，以及完整 JSON 参数编辑器。
- 95 项掩码库、按示例逐位生成、字符速查和多行 .hcmask 导入，预览候选范围后再应用。
- 文件与路径输入框支持原生拖拽；分步/同页两种填写方式共用输入，设置搜索可直接定位并展开对应字段。
- 中文/英文、浅色/深色/系统外观；内置离线使用指南和作者署名页。
- 从 Foren DEV 检查新版本、选择稳定版/测试版并下载适配包；显示进度、支持取消，校验包大小与 SHA-256 后保存。默认在启动后检查更新，可在关于页关闭。

HashHelm 不经过 CMD、PowerShell 或 POSIX shell 执行用户参数。可复制命令仅供查看和外部使用。

## 平台与运行

Windows 0.11 提供两个便携包：offline 随带完整微软 WebView2 固定版，适合无需另装运行库的分发；lite 使用系统 WebView2。两者 GUI 都静态链接 VC/C++ 运行库。离线包体积主要来自浏览器运行时，GUI 自身仍保持轻量。完整解压后运行 HashHelm.exe。

新增「日志与诊断」、原生启动错误弹窗、不可写便携目录的用户数据回退及支持包导出。无法启动时可运行随包 Diagnose.cmd；Start-HashHelm.cmd 可保留实际 GUI 启动的退出码。详见 [启动与沙箱说明](SECURITY-NOTES.md)。

Windows UI 组件包含 Microsoft Defender SmartScreen，并按微软隐私声明收集、发送信息。使用随包微软运行时须遵守独立许可；开始页和关于页提供说明，完整文本在 assets/licenses 中。

| 平台 | 图形后端 | 当前交付方式 |
| --- | --- | --- |
| Windows x64 | 随包或系统 WebView2 Runtime | 便携目录，运行 HashHelm.exe |
| macOS 15+ Apple Silicon / Intel | 系统 WKWebView | 原生 .app 构建配置 |
| Linux x64 / ARM64 | GTK3 / WebKitGTK 4.1 | 原生便携构建配置 |

Windows 已进行本机核心和前端验证。macOS/Linux 原生编译及启动结果以仓库 Actions 为准；CI 启动检查不能代替实际 GPU 和人工视觉验收。

发布包需要保留完整的 `hashcat` 和 `assets` 资源。GPU 驱动、OpenCL/CUDA/Metal 等后端按系统准备；程序不会自动安装驱动。日常 GUI 无需 Rust 或 Node，Hashcat 的 Python 插件模式仍需要系统 Python。

## 快速开始

1. 选择 ZIP、RAR、7z、Office、PDF，或导入哈希文本并确认算法。
2. 填写已有线索，或限定字典、长度和字符范围。
3. 检查计划、候选量、阶段顺序与实际命令，必要时统计输入或测速，再开始恢复。
4. 查看当前处理量和候选窗口；找到候选密码后，在结果页验证原文件，再导出解密副本。

候选窗口是引擎的异步批次抽样，不是每个密码的完整尝试记录。处理量可能包含拒绝项、多盐验证和重复阶段；基础工作位置不等于字典行号。时间上限到达不代表该范围已全部检查。

Hashcat 本身并行计算；同一进程可通过设备编号协同使用多设备。重复启动进程争用同一显卡可能更慢。Hashcat 8 忽略 `-w`，本项目不以该参数提供负载档位。

0.7 的图形恢复任务统一经过队列。准备其他文件不必等待正在运行的任务结束；没有本次启动的设备检测结果时，队列使用独占模式。暂停派发不影响已在计算的任务。重启后不会自动恢复计算；模板会保留字典路径，应用到其他目标前请检查候选内容。

## 构建与文档

- [中文使用指南](GUIDE.zh-CN.md)
- [macOS/Linux 构建说明及平台依赖](BUILDING.md)
- [第三方组件、字典和规则许可](THIRD_PARTY.md)
- [作者署名](CREDITS.md)

```sh
# 需要 Rust stable、Python 3.11+ 和相应平台的图形开发依赖。
cargo test --no-default-features --lib --locked
cargo run --locked --bin hashhelm

# macOS/Linux：构建固定版本的完整 Hashcat 并打包。
bash scripts/build-unix.sh --with-hashcat
```

自动构建覆盖 macOS arm64/x64 与 Linux arm64/x64。原生包检查程序/引擎架构、模块、共享核心、GUI 初始化及体积增量。macOS 当前配置为 ad-hoc 开发签名，未配置 Developer ID 或 Apple 公证。

## 当前边界

- ZIP ZipCrypto（Stored/Deflate）及 WinZip AES、Office 常见标准/Agile 加密与部分旧 Word/Excel RC4、PDF Standard Security R2–R6 的常见组合。
- 已支持常见 RAR3（内容/文件名加密）、RAR5（带密码检查字段）、7z AES + Copy/LZMA/LZMA2 和 ZIP64。分卷、自解压、带盐或特殊过滤链 7z、证书及 IRM 等仍需外部工具。
- 文件解析设有大小和结构限制；具体提示以程序为准。引擎匹配与原文件验证分别显示；ZIP 只验证和导出所选项，其他项可能使用不同密码。
- 每个计划的阶段顺序执行，不同物理设备可并行。停止可能丢失最近检查点后的工作；只有实际存在的原生检查点才能继续。
- 无法保证恢复出密码。不同算法、迭代次数与候选范围可能造成数量级的耗时差异。

## 原文件验证与导出

0.8 在独立子进程中验证候选密码；ZIP 完整读取所选项，7z/RAR 校验全部内容，Office 实际解密并检查文档结构，PDF 验证标准安全处理器并解密对象。PDF 与旧 Office 的格式本身不提供现代全文件认证，验证通过不等于证明所有内容未被篡改。

单条结果会自动验证（需要本版重新导入原文件）；多条结果可分别点击验证。导出会再次验证，ZIP 输出所选项，RAR/7z 输出不带密码的 ZIP 副本，Office/PDF 输出解密文档。始终创建新文件，不覆盖原文件或其他已有文件。

验证设有 120 秒、归档总输出 1 GiB、Office/PDF 128 MiB 等限制；RAR 导出单项上限 128 MiB。导入时仍受各 Hashcat 模式验证块大小限制。完整支持范围、已测样本和复现方法见 [FORMAT-SUPPORT.md](FORMAT-SUPPORT.md)。

## 本地数据

Windows 默认使用便携目录中的 `workspace`；macOS 默认使用 `~/Library/Application Support/DigiForensics/HashHelm`；Linux 默认使用 `$XDG_DATA_HOME/digiforensics/hashhelm` 或 `~/.local/share/digiforensics/hashhelm`。Linux 可通过 `portable.flag` 启用旁边的便携工作区。

数据可能含明文候选与已恢复密码，请妥善保管。文件、任务与结果在本机处理，不上传至外部服务。程序只会在用户点击关于页链接时调用系统浏览器。

## License

Copyright (c) 2026 **DyNooob**.

HashHelm 原始代码和有权授权的相关材料按 [HashHelm Attribution License v1.0](LICENSE) 分发。允许免费使用、商用、修改、更换品牌及闭源衍生；再分发时须保留原作者署名、官网、原项目链接和许可声明，图形应用须在关于或致谢等页面提供可见署名。具体权利和条件以 LICENSE 为准。

Hashcat、第三方库、字典、规则和测试样本继续遵循各自的许可证，不因 HashHelm 的许可而改变。

## 操作快捷方式

- Ctrl/Cmd+O：选择新的恢复目标。
- Ctrl/Cmd+K：查找设置，输入名称后按 Enter 定位；支持方向键选择。
- Ctrl/Cmd+Shift+M：掩码库与生成器。
- Ctrl/Cmd+Enter：生成恢复计划，不直接运行。

参数更改会使已有计划过期，运行前需重新生成。拖入字典后会使用该字典，先前输入的候选保留在界面中。VeraCrypt 验证记录的 SHA-256 对应所选 512 字节卷头，不代表完整数据区的哈希或解密结果。详见 [格式支持](FORMAT-SUPPORT.md) 和 [设计与验收说明](DESIGN.md)。

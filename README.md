# HashHelm

A lightweight, cross-platform Hashcat GUI built with Rust for password recovery, CTFs, and digital forensics.

**Originally developed by DyNooob.**

Official Website: https://www.foren.dev

Original Project: https://github.com/DyNooob/HashHelm

HashHelm 是 DigiForensics 的轻量 Hashcat 图形工作台。通过文件分析、线索组合、可排序的恢复计划和真实引擎进度，降低密码恢复的使用门槛，同时为高级用户保留字面参数编辑和设备控制。

## 功能

- 分析常见加密 ZIP、Word、Excel、PowerPoint 和 PDF，提取密码验证信息；支持导入已有哈希。原文件保持只读。
- 从姓名、生日、关键词生成有限、去重的候选，支持中文姓名拼音和首字母。
- 自定义字典、规则、字符集、掩码、长度范围及固定前后缀，组合并调整多个恢复阶段。
- 显示引擎报告的处理量、总量、拒绝数、速度、设备状态、候选窗口、基础工作位置和预计完成时间。
- 保存任务、检查点、日志与最近进度快照，支持回看、导出和计划复用。
- 多设备选择、内核参数、八组自定义字符集、规则链式/合并，以及完整 JSON 参数编辑器。
- 中文/英文、浅色/深色/系统外观；内置离线使用指南和作者署名页。

HashHelm 不经过 CMD、PowerShell 或 POSIX shell 执行用户参数。可复制命令仅供查看和外部使用。

## 平台与运行

| 平台 | 图形后端 | 当前交付方式 |
| --- | --- | --- |
| Windows x64 | 系统 WebView2 Runtime | 便携目录，运行 HashHelm.exe |
| macOS 15+ Apple Silicon / Intel | 系统 WKWebView | 原生 .app 构建配置 |
| Linux x64 / ARM64 | GTK3 / WebKitGTK 4.1 | 原生便携构建配置 |

Windows 已进行本机核心和前端验证。macOS/Linux 原生编译及启动结果以仓库 Actions 为准；CI 启动检查不能代替实际 GPU 和人工视觉验收。

发布包需要保留完整的 `hashcat` 和 `assets` 资源。GPU 驱动、OpenCL/CUDA/Metal 等后端按系统准备；程序不会自动安装驱动。日常 GUI 无需 Rust 或 Node，Hashcat 的 Python 插件模式仍需要系统 Python。

## 快速开始

1. 选择 ZIP、Office、PDF，或导入哈希文本并确认算法。
2. 填写已有线索，或限定字典、长度和字符范围。
3. 检查计划、阶段顺序与实际命令，再开始恢复。
4. 查看当前处理量和候选窗口；找到后用原软件打开或解压验证。

候选窗口是引擎的异步批次抽样，不是每个密码的完整尝试记录。处理量可能包含拒绝项、多盐验证和重复阶段；基础工作位置不等于字典行号。时间上限到达不代表该范围已全部检查。

Hashcat 本身并行计算；同一进程可通过设备编号协同使用多设备。重复启动进程争用同一显卡可能更慢。Hashcat 8 忽略 `-w`，本项目不以该参数提供负载档位。

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
- 文档编辑限制、工作表保护与打开密码不同。RAR、7z、ZIP64、分卷、自解压、证书和 IRM 等特殊格式请用外部工具提取哈希。
- 文件解析设有大小和结构限制；具体提示以程序为准。恢复成功后仍需验证原文件，尤其是使用不同密码的 ZIP 成员。
- 一次运行一个恢复进程，阶段顺序执行。停止可能丢失最近检查点后的工作；只有实际存在的原生检查点才能继续。
- 无法保证恢复出密码。不同算法、迭代次数与候选范围可能造成数量级的耗时差异。

## 本地数据

Windows 默认使用便携目录中的 `workspace`；macOS 默认使用 `~/Library/Application Support/DigiForensics/HashHelm`；Linux 默认使用 `$XDG_DATA_HOME/digiforensics/hashhelm` 或 `~/.local/share/digiforensics/hashhelm`。Linux 可通过 `portable.flag` 启用旁边的便携工作区。

数据可能含明文候选与已恢复密码，请妥善保管。文件、任务与结果在本机处理，不上传至外部服务。程序只会在用户点击关于页链接时调用系统浏览器。

## License

Copyright (c) 2026 **DyNooob**.

HashHelm 原始代码和有权授权的相关材料按 [HashHelm Attribution License v1.0](LICENSE) 分发。允许免费使用、商用、修改、更换品牌及闭源衍生；再分发时须保留原作者署名、官网、原项目链接和许可声明，图形应用须在关于或致谢等页面提供可见署名。具体权利和条件以 LICENSE 为准。

Hashcat、第三方库、字典、规则和测试样本继续遵循各自的许可证，不因 HashHelm 的许可而改变。

# macOS / Linux 构建指南（源码 0.11.0）

正式产品名为 **HashHelm**，DigiForensics 品牌和图标保持一致。

本仓库包含跨平台源码和自动构建配置。Windows 主机已验证共用核心、前端和打包规则；
macOS/Linux 原生编译、GUI 启动、看门狗及 GPU 性能必须在对应系统继续验证。
这里没有把 Windows EXE 改名成原生程序，也没有附带未经验证的 macOS/Linux 可执行文件。

## 构建目标

Windows 本地打包仍使用 `powershell -File scripts/package.ps1`。每次在
`dist/packages/<构建编号>/HashHelm-<版本>/` 创建全新目录，经压缩完整性与体积校验后
再替换 `dist/HashHelm-<版本>-build<构建编号>-windows-x64-offline.7z`。旧展开目录及其中的工作区不作修改，
已删除的旧资源也不会混入新包；实际展开路径记录在 `dist/size-report.json`。

| 平台 | 架构 | CI Runner | 输出 |
| --- | --- | --- | --- |
| macOS 15+ | Apple Silicon arm64 | macos-15 | 含 HashHelm.app 的 tar.gz |
| macOS 15+ | Intel x64 | macos-15-intel | 含 HashHelm.app 的 tar.gz |
| Linux | x64，glibc | ubuntu-22.04 | 便携目录 tar.gz |
| Linux | ARM64，glibc | ubuntu-24.04-arm | 便携目录 tar.gz |

Linux x64 在 Ubuntu 22.04 构建，ARM64 在 Ubuntu 24.04 构建；更旧 glibc、musl/Alpine
不在当前二进制兼容范围。Linux 窗口通过 GTK3 / WebKitGTK 4.1 渲染，覆盖 X11 和
GTK 支持的 Wayland 环境。系统 WebKitGTK 与 GPU 驱动不打入包内。

## 自动构建

将源码放进自己的 GitHub 仓库后，在 Actions 中运行 **Native desktop builds**。
主分支提交、`v*` 标签和 PR 也会触发。流程只上传构建 Artifact，不创建 Release，
也不向外上传用户文件或本地 workspace。

四个任务各自完成：

1. 安装构建依赖，检查 Runner 的实际架构。
2. 安装锁定的开发用前端依赖，执行命令、工作流、跨平台路径和打包规则测试。
3. 从 Hashcat 官方仓库构建固定的 8.0.0 提交
   `6a7e3b5a96abcd6373c2e4112bc358abb24dc538`；使用 MAINTAINER_MODE=1，避免把构建机独有的 CPU 指令带入分发包。
4. 运行 Rust Clippy、核心测试、原生输出/取消/结果查询，以及 Unix 子进程组与异常退出清理检查。
5. 编译 GUI，验证 ELF/Mach-O、架构、Hashcat 版本、共享核心和完整模块集；复制内核、字典、规则、PCFG、Python/Feed/Bridge 等运行资源。
6. 检查 GUI 静态增量和同格式压缩增量都在 10,000,000 字节以内。
7. 启动原生窗口，等待前端完成 JavaScript 初始化并通过 IPC 回报就绪。

Artifact 包含 `.tar.gz`、体积/SHA256 报告及原生启动报告，保存 14 天。
CI 初始化检查不能代替人工视觉验收，也不代表已经在真实 GPU 上验证性能。

## macOS 本机构建

准备 Xcode Command Line Tools、Git、make、Rust stable（建议 1.95+）、Python 3.11+。
不需要安装 WebView2 或另带浏览器；使用 macOS 自带 WKWebView。

```sh
xcode-select --install
bash scripts/build-unix.sh --with-hashcat
```

`xcode-select --install` 只在没有 Command Line Tools 时由用户执行。脚本构建当前 Mac
的原生架构，Apple Silicon 不依赖 Rosetta，Intel 也独立构建。
最终目录为 `dist/HashHelm-0.9.0-macos-arm64` 或 `...-macos-x64`。

包内 Hashcat 8 的 Mach-O 最低系统版本为 macOS 15，应用包声明相同要求。
macOS 包生成 DigiForensics ICNS 图标，并使用 ad-hoc 开发签名。
**没有 Developer ID 发行签名，也没有 Apple 公证。** 对外正式分发需要你自己的 Apple
Developer 证书和公证流程；本轮不伪造签名、不关闭 Gatekeeper。

## Linux 本机构建

Ubuntu 22.04 / 24.04 或相应发行版准备：

```sh
sudo apt-get update
sudo apt-get install build-essential git pkg-config libclang-dev libgtk-3-dev libwebkit2gtk-4.1-dev python3
bash scripts/build-unix.sh --with-hashcat
```

还需要 Rust stable（建议 1.95+）、Python 3.11+；Ubuntu 22.04 系统默认 Python 3.10
需要另准备 3.11+ 并让 `python3` 指向它。自动构建已用 setup-python 配置 3.12。
单独运行源码 GUI：`cargo run --locked --bin hashhelm`。

包内 `launch.sh` 不依赖启动时的当前目录。解压后执行：

```sh
./launch.sh
# 可选：安装当前用户的应用菜单入口，移动文件夹后重新运行。
python3 install-desktop.py
```

启动 GUI 无需 root。恢复计算需要能被 Hashcat 检测到的 GPU / OpenCL / Metal 后端。
CPU OpenCL 或显卡驱动由系统提供，脚本不安装、更换驱动。

## 引擎和打包选项

默认源码引擎目录：`vendor/hashcat-src-8.0.0`。已存在的 Git 工作树必须位于固定提交；
脚本不会重置或删除已有文件。

```sh
export HASHCAT_SOURCE=/absolute/path/to/hashcat-source
export HASHHELM_BUILD_JOBS=4
bash scripts/build-unix.sh

# 对已有完整原生引擎单独打包；源目录已编译完成，且 GUI 已 release 构建。
python3 scripts/package_unix.py --engine-dir "$HASHCAT_SOURCE" --output dist/my-native-build
```

输出目录已存在时拒绝合并，避免旧模块掩盖缺失资源。打包不会删除 Hashcat 功能模块，
只排除 Git、obj/target、中间对象、日志、Python 缓存与其他平台的 EXE/DLL。
模式 73000 使用 Python 时仍需要系统 Python；其他官方可选后端同样需要其系统依赖。

## 资源与用户数据

| 平台 | 资源 | 默认任务、候选、日志和恢复结果 |
| --- | --- | --- |
| macOS | `HashHelm.app/Contents/Resources` | `~/Library/Application Support/DigiForensics/HashHelm` |
| Linux | 可执行文件旁的 assets/hashcat | `$XDG_DATA_HOME/digiforensics/hashhelm`，未设置则 `~/.local/share/digiforensics/hashhelm` |
| Windows | 便携目录旁的 assets/hashcat | 便携目录内 `workspace` |

macOS 不把可变数据写进 `.app`，避免资源只读和应用签名失效。
引擎查询和未指定 `--cache-path` 的命令统一使用工作区的 `cache`，不打包构建机缓存；包内引擎运行后会再次检查应用签名。
Linux 在程序目录创建空的 `portable.flag` 后，可使用旁边的 `workspace`；目录必须可写。
也可用绝对路径 `HASHHELM_DATA_DIR` 覆盖数据目录。

改名前的 `FORENHC_DATA_DIR`、`FORENHC_HASHCAT`、`FORENHC_STARTUP_REPORT` 仍兼容；
新变量优先。macOS/Linux 已有 ForenHC 数据目录且尚未建立 HashHelm 数据目录时，
继续使用已有目录，不自动覆盖、移动或删除任务和恢复结果。

引擎按打包目录、开发目录、PATH 查找；macOS 还检查 `/opt/homebrew/bin` 和
`/usr/local/bin`。可通过界面选择，或用绝对路径 `HASHHELM_HASHCAT` 指定。
外部引擎应为 8.0.0；系统包较旧时，部分新参数会不兼容。

跨平台复制历史不等于外部文件路径和原生检查点自动兼容。跨系统恢复应重新分析原文件；
同系统移动包时，内部资源与数据路径分别迁移，不改写其他外部路径。

## 本地检查

```sh
cargo clippy --all-targets --locked -- -D warnings
cargo test --no-default-features --lib --locked
npm ci --prefix tests/browser --ignore-scripts
node scripts/ui-tests.cjs
node scripts/workflow-tests.mjs
node scripts/platform-ui-tests.mjs
python3 -m unittest discover -s tests -p 'test_packaging.py'
```

`runtime-check` 是开发用的原生进程验收程序，不随发布包分发。
Unix 用独立看门狗监测 UI 的管道，即使 UI 异常退出，也能结束恢复进程组；
Linux 额外设置父进程死亡信号。Windows 继续使用 Job Object。
前端命令预览按系统使用 PowerShell 或 POSIX 引号，实际执行始终传递字面参数，不经过 shell。

官方依据：[Wry 平台依赖](https://docs.rs/crate/wry/0.57.1)、
[Hashcat 8 构建说明](https://github.com/hashcat/hashcat/blob/v8.0.0/BUILD.md)、
[GitHub Runner 列表](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。

## 原文件验证回归

新验证器和格式提取无需 Python/Perl 运行时。UnRAR C++ 核心随程序静态链接，构建需要 C++ 工具链；Windows 通过系统 advapi32 使用 CryptoAPI，并通过 .cargo/config.toml 静态链接 VC++ 运行库；GUI 不额外依赖 VC++ Redistributable DLL。

```sh
cargo build --locked --bin extract-check --bin verify-check
python3 scripts/file-verification-tests.py
```

测试只读取固定公开样本，在临时目录验证错误密码、正确密码、导出、禁止覆盖和源文件变化。四个平台的 CI 都执行它。实际 Hashcat/GPU 提取回归另见 scripts/archive-recovery-tests.py，可用 HASHHELM_ENGINE 指定本机原生引擎。

## Windows 0.9 离线包

```powershell
powershell -File scripts/package.ps1        # 完整包，内置官方 WebView2 固定版
powershell -File scripts/package.ps1 -Lite  # 轻量包，使用系统 WebView2
```

运行时由 assets/webview2-runtime.json 固定官方 URL 与哈希，prepare-webview2.ps1 校验 CAB 和微软代码签名，原样复制到 runtime/webview2。完整包单独统计浏览器体积，GUI/资源增量仍检查 10 MB 预算。不要用本机 Edge 安装目录替代 WebView2 Fixed Version。升级固定版时应从官方接口核对版本、URL、哈希，并复跑原生启动。

应用日志位于数据目录 logs，20 份轮换、每份 2 MiB。测试启动诊断：python scripts/startup-tests.py；可用 HASHHELM_GUI_PATH 指定最终包中的原生程序。

## 更新服务与构建编号

DigiForensics（https://digiforensics.cn）是品牌站点；Foren DEV（https://www.foren.dev）用于软件开发与发布。HashHelm 使用 `https://foren.dev/updates/v1/check/`，App ID 为 `hashhelm`。客户端按当前系统和架构匹配包，不需要授权 Token。

`Cargo.toml` 控制 SemVer 版本，`HASHHELM_BUILD_NUMBER` 控制同版本的构建编号，默认 1，范围 1–2147483647。重新分发同一版本的不同构建时递增编号，并在编译、打包和 Foren DEV 后台保持一致。Windows PowerShell 可在构建前设置 `$env:HASHHELM_BUILD_NUMBER='2'`；macOS/Linux 使用 `export HASHHELM_BUILD_NUMBER=2`。

在 Foren DEV 后台为 HashHelm 新建版本草稿，填写版本、构建编号、系统、架构、不可变的 HTTPS 包地址、整个归档的 SHA-256 与字节数，核对后发布。Windows 默认发布 offline 包；lite 包可留在下载页。同一构建不混用源码包与客户端二进制包。不要覆盖已发布 URL 的文件。

服务没有可用新版时，界面按产品约定显示“当前已是最新版本”，不展示下载入口；它仅表示当前渠道没有可用的更新包。更新服务故障不会影响恢复任务。开发时可显式运行 `cargo run --no-default-features --bin update-check` 验证公开接口与 ETag 缓存；CI 不依赖线上更新服务存活。前端更新状态回归：`node scripts/update-tests.mjs`。

## 0.11 验收补充

`node scripts/usability-tests.mjs` 覆盖拖拽路由、字典优先级、掩码生成、多行掩码、同页状态与过期计划；更新测试也覆盖默认启动检查与关闭设置。`scripts/container-tests.py` 使用 container-check/verify-check 验证公开卷头。CI 使用 release 工具执行 PBKDF2 样本，避免无优化构建的计算耗时。`scripts/container-recovery-tests.py` 是需要真实 Hashcat 计算后端的可选互通测试，CI 不伪造 GPU 结果。

已完成编译且确认产物对应当前源码时，可用 `scripts/package.ps1 -SkipBuild` 打包；脚本仍检查版本、构建、系统和架构。默认路径仍会编译。macOS/Linux 的拖拽使用系统逻辑坐标；原生窗口与拖拽实际体验需在对应系统验收。

会话与原生范围检查回归：`node scripts/session-ui-tests.mjs`。可选真实引擎验收：先 `cargo build --release --locked --no-default-features --bin session-check`，再 `python scripts/session-recovery-tests.py`；后者需要可用计算设备，使用公开测试哈希和短时掩码，不使用用户文件。

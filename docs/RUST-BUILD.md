# Satori 0.1.3：从源码到 Windows EXE
本说明针对本次“首页快捷入口修复版”，API 仍是可选增强层。

## 1. Rust 在这里负责什么
Rust 把 .rs 源文件编译成原生机器码。Cargo 是 Rust 的依赖、编译和测试工具，rustup 管理编译器与目标平台。普通用户运行编译好的 Satori，不需要安装 Rust 或 Node；Windows 界面需要 WebView2 Runtime。

Satori 的 Svelte/TypeScript 前端经 Vite 生成 HTML、JS、CSS；Tauri 再把这些生产资源嵌入 Rust 程序。Windows 上用 WebView2 显示界面，Rust 处理 SQLite、学习规则、托盘和受限系统操作。因此，“npm run build 成功”只证明前端构建成功，还没有生成桌面 EXE。

| 文件或目录 | 用途 |
| --- | --- |
| Cargo.toml | 工作区、Rust 依赖声明和构建配置 |
| Cargo.lock | 锁定具体 Rust 依赖版本 |
| package.json / package-lock.json | 前端命令、依赖声明和锁定版本 |
| crates/habitos-core/src/engine.rs | 助手决策、状态变化、显式操作 |
| crates/habitos-core/src/runtime.rs | 串行处理命令与事件，协调核心状态 |
| crates/habitos-core/src/store.rs | SQLite 读写 |
| src-tauri/src/main.rs | 桌面入口、Tauri 命令及窗口连接 |
| src/App.svelte | 面向用户的主界面 |
| src-tauri/tauri.conf.json | 前端路径、窗口、版本及安装包配置 |

crate 可理解为一个 Rust 代码包；workspace 是多个包组成的工程。本项目把可独立测试的核心库和桌面程序放在同一工作区。根配置默认只构建核心库，所以直接运行 cargo build 也不等于完成桌面发布。

## 2. 我采用的工程思路
**把“用户现在要做什么”和“助手猜用户想做什么”分开。** home_apps 是已保存、可手动打开的六个快捷入口；quick_apps 是受学习与建议开关影响的推荐。关闭学习不应使手动按钮失效。超过六个目标仍保留在完整列表里，敏感或停用目标不会被启动。

**状态有明确的归属。** Engine 管业务，Store 管持久化，Platform 隔离系统调用，界面通过命令获取状态。列表是否展开属于界面状态，后台刷新不能擅自覆盖用户的选择。搜索时需要展开列表，应由搜索输入事件明确触发，而不是依赖互相影响的响应式绑定。

**结构化状态比猜文字可靠。** 添加、移除、打开的结果类别由命令名称决定，不能在显示文案里搜索“打开”来判断。文件名与提示说明本身就可能包含这些词。

**学习闭环要检查到数据库。** 模型输出只是候选。用户确认、场景校验、受限执行、确认经验入库都完成后，下一次才能本地复用。本次回归发现确认应用后漏写经验，已恢复确认写入；现有测试同时检查“记住选择”和“旧确认不能再次执行”。手动快捷打开仍不会伪造推荐训练反馈。

**先用证据，再交付。** 本次执行类型检查、核心测试、界面测试、AI 本机模拟测试及生产资源检查。编译通过不能证明真实 Windows 文件选择、输入法、亮度硬件和服务商 API 已经正常工作。测试失败时先区分程序错误、测试定位过时和环境缺少工具，再做最小修复。

**保留你的产品核心。** 本地规则与 SQLite 经验优先；API 未开启时仍可使用助手。开启后，只在满足条件的新异常或需要解释的选择上唤醒；模型不能获得通用 shell 权限。优化 UI 和构建不会把它变成一直联网的通用 agent。

## 3. 在自己的 Windows 电脑上准备环境
按 Tauri 官方前置要求安装：
1. Microsoft C++ Build Tools，选“使用 C++ 的桌面开发”，保留对应 MSVC 编译工具和 Windows SDK。
2. Rust（rustup），使用 x64 Windows 默认 MSVC 工具链。这个源码标注 Rust 1.90；本次使用 1.90.0。
3. Node.js；项目 CI 使用 Node 24，本次也用 Node 24。
4. WebView2 Runtime（机器上已有可用版本则无需重复安装）。

安装后重新打开终端，检查：
```powershell
rustc --version
cargo --version
node --version
npm --version
```
解压源码，在同时包含 Cargo.toml 和 package.json 的目录里打开 PowerShell。路径中有空格时用引号，例如 cd "D:\Projects\Satori"。

## 4. 一次完整的 Windows 构建
以下每条命令成功后再执行下一条；若有报错，先处理，不要直接上传旧 EXE。
```powershell
npm ci
cargo fmt --all --check
cargo test -p habitos-core --locked
cargo clippy -p habitos-core --all-targets --locked -- -D warnings
npm run check
npm run test:browser
npx playwright install chromium
npm test
powershell -NoProfile -File tests/ai-response.Tests.ps1
powershell -NoProfile -File tests/ai-prompt.Tests.ps1
python tests/ai_http_fixture.py --powershell powershell
npm run tauri -- build --no-bundle -- --locked
node scripts/verify-release.mjs target/release/satori.exe
```
HTTP 模拟测试需要 Python 3，只访问本机测试服务；不消耗 API 额度。Playwright 首次要下载测试浏览器；以后已有浏览器可跳过安装。

默认 Windows x64 MSVC 环境下，运行文件位于 target/release/satori.exe。前端由 tauri build 的 beforeBuildCommand 自动生成，生产构建启用 custom-protocol 来嵌入前端。项目 build.rs 会拒绝未嵌入前端的 Release，防止交付一个仍依赖开发服务器的程序。

只想学习最基本的 Rust 编译，可先运行 cargo test -p habitos-core --locked 看核心库。若不用 Tauri CLI 而直接构建桌面，需要先生成前端，再指定桌面包和生产特性：
```powershell
npm run build
cargo build -p habitos-desktop --release --features custom-protocol --locked
```
新手发布时优先用前面的 Tauri CLI 完整流程。

## 5. Release、EXE 和安装包的区别
cargo build 默认是便于调试的 dev 构建；--release 使用发布配置。项目的发布配置是：
```toml
[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
```
它以缩小体积为优化目标，启用跨代码包的链接优化、减少代码生成分块并去除符号，因此编译通常更慢。Release 是编译方式，不代表软件已经成熟；Satori 仍标注 0.1.3 Alpha。

--no-bundle 生成发布 EXE，但不创建安装向导。本次交付的是完整便携 ZIP，含 EXE、WebView2Loader.dll、启动脚本、浏览器配套文件和第三方许可。请完整解压，不能只把 EXE 从 ZIP 中拖出来就运行。

在 Windows 上执行下面命令可按当前配置生成 NSIS 安装包：
```powershell
npm run tauri -- build -- --locked
```
通常到 target/release/bundle/nsis 查找 setup.exe。这里的 setup.exe 是安装器，satori.exe 是实际应用。这条安装包流程在本次 Linux 构建环境中没有执行，不应把本次便携包描述成已经验证过的安装器。

## 6. 本次实际构建方式与验收
本次在 Linux 上交叉编译 Windows x64，Rust 1.90.0，目标 x86_64-pc-windows-gnu，使用 MinGW GCC 13.2.0。它与上面更适合你日常开发的 Windows MSVC 流程工具链不同，产物仍是 Windows x64 PE 程序。

实际生产命令：
```bash
CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS=1 \
  npm run tauri -- build --target x86_64-pc-windows-gnu --no-bundle -- --locked
```
需要预先安装该 Rust target、MinGW 编译和资源工具，并把它们加入 PATH。本次构建遇到宿主侧中间对象文件为空，重试时将并行任务限制为 2，并把宿主构建依赖的 codegen-units 设为 1；最终应用的发布优化配置与依赖锁文件保持不变。交叉编译不意味着你在 Windows 开发时也要装 MinGW。

运行包 BUILD-INFO.md 记录本次最终检查和 SHA-256。exe 检查会核对当前前端、图标与内嵌辅助脚本，避免误发旧文件。没有在本次环境中启动 Windows EXE，也没有实际连接外接屏或真实 API 服务商。

首次验收可先运行 Start-Demo.cmd，再退出并运行 Start-Real.cmd：添加普通 exe，检查快捷格及完整列表；展开后暂停与恢复，确认不折叠；搜索列表；移除并确认磁盘文件还在。随后再按你的设备测试输入、亮度、音量和启动项。更新前先从托盘退出旧进程，避免单实例机制把启动请求交给旧版本。

## 7. 怎么开始学 Rust
先读 Platform trait（系统能力接口）与 Engine::open_target，再读对应测试，比从整个桌面入口开始更容易。
- struct：有固定字段的数据。
- enum：有限种状态或命令，让遗漏分支更容易被编译器发现。
- Result<T, E>：成功返回 T，失败返回 E；? 会把错误交给调用者处理。
- &T：借用数据；&mut T：独占地借用并允许修改。
- trait：约定一组能力；本项目可用 DemoPlatform 替代 Windows 操作来验证决策。

Rust 的所有权与借用规则能在编译时阻止许多内存错误，但“确认后忘记存数据库”仍然是业务 bug，要靠清晰流程与回归测试发现。先读一个函数、跑对应测试、做一个小改动，再看编译器反馈，就能逐步上手。

官方参考：
- [Tauri 前置要求](https://v2.tauri.app/zh-cn/start/prerequisites/)
- [Tauri Windows 安装包](https://v2.tauri.app/distribute/windows-installer/)
- [Cargo 构建配置](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Rust 官方书](https://doc.rust-lang.org/book/)


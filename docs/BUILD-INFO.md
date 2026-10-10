# Satori v0.1.5 Alpha · 完整修订构建记录

构建日期：2026-10-10。基础源码为用户上传的 v0.1.3 源码包，包含 v0.1.4 与 v0.1.5 列表修复，以及本次状态、保存和 CI 修订。

最终 satori.exe SHA-256：`1468c8d825ccc2ddfa689cfc1cdb34ad40e617ebf7ff8896899d7e6fc52655c2`。

这次重新编译了本版核心库、桌面程序及生产前端；不是此前 EXE 改名。此前同版本交付的 EXE 哈希以 `dfb72f4f` 开头，应换成这次完整修订包。

## 已在本次环境执行

| 检查 | 结果 |
| --- | --- |
| 完整 Playwright 面板测试，CI=true、两个 worker | 28/28 通过 |
| 列表、刷新、扩展输入时序回归，两个 worker，每项重复 20 次 | 220/220 通过 |
| Svelte / TypeScript 检查 | 0 错误、0 警告 |
| 浏览器隐私过滤测试 | 3/3 通过 |
| Rust 核心测试（含保存失败及 AI 失效返回） | 80/80 通过 |
| cargo fmt --all --check | 通过 |
| 核心 Clippy，全部目标 | -D warnings 通过 |
| Windows GNU Release 全工作区 Clippy，全部目标 | -D warnings 通过 |
| AI 响应解码，Linux PowerShell 7.4.6 | 14 项通过 |
| AI 提示词边界，Linux PowerShell 7.4.6 | 23 项通过 |
| AI HTTP 本机模拟，Linux PowerShell 7.4.6 | 7 场景通过，不调用付费服务 |
| Tauri Windows GNU 生产构建 | --locked、custom-protocol、--no-bundle，通过 |
| Windows 桌面测试程序 | 编译通过；未执行 Windows 二进制 |
| EXE 格式和版本 | Windows PE x64，GUI subsystem 2，0.1.5 |
| 生产资源实际内嵌 | 当前 HTML、JS、CSS 与 7 种图标尺寸匹配 |
| 辅助脚本实际内嵌 | ai.ps1、desktop-input.ps1 字节匹配 |
| DLL 导入 | Windows 系统 DLL 与 WebView2Loader.dll；无额外 MinGW 运行时 DLL |
| 预览图片 | 安装中文与表情字体后重新生成，4 张已人工查看 |
| CI YAML 与新增 PowerShell 脚本语法 | 解析通过；Windows 安装及启动步骤未在本机执行 |

工具链：Rust 1.90.0、Node 24.19.0、MinGW GCC 13.2.0；Linux 交叉编译，目标 x86_64-pc-windows-gnu。新增 tempfile 仅用于桌面测试，所有第三方 Cargo 锁记录及前端锁定依赖版本未升级。浏览器扩展代码仍为 0.1.3。

```bash
CI=true npm test
CI=true npm test -- --workers=2 --repeat-each=20 tests/saved-targets-toggle.spec.ts tests/assist-refresh.spec.ts tests/assist.spec.ts
cargo test -p habitos-core --locked
CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS=1 npm run tauri -- build --target x86_64-pc-windows-gnu --no-bundle -- --locked
node scripts/verify-release.mjs target/x86_64-pc-windows-gnu/release/satori.exe
```

## Windows 与远程验收边界

本次没有 Windows 原生运行环境，没有启动 EXE，也没有执行 Windows PowerShell 5.1。Linux PowerShell 结果不能代替 5.1。新增桌面测试、5.1 解码和 HTTP 回归、已打包 EXE 的原生演示启动检查均写入 Windows Actions；这些是待执行检查，不能声称远程 CI 已通过。没有直接推送远程仓库。

GUI 启动检查只确认窗口出现、消息循环响应及程序未退出，不能代替真实设备验收。Windows 文件选择、输入法、桌面输入、音量、DDC/CI、启动项和真实 API 服务商仍需真机测试。便携 EXE 未签名，系统需要 WebView2 Runtime。

## 包与兼容性

交付包名带 complete，便于区别此前同版本附件。源码保留 Rust/Tauri/Svelte/SQLite 架构、应用标识 io.habitos.desktop 和 SQLite schema 5。不要清空已有学习数据。

完整解压运行包，先从托盘退出旧实例，再双击 satori.exe；左下角应显示 v0.1.5。包根 BUILD-INFO.md 记录实际 EXE 哈希，SHA256SUMS.txt 用于核对包内文件。CI 或其他机器重新构建的 EXE 哈希可能不同，应以其包根记录为准。

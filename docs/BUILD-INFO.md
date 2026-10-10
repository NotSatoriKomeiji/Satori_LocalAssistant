# Satori v0.1.4 Alpha · Release 构建记录

构建日期：2026-10-10。
输入：Satori-v0.1.3-home-shortcuts-source(1).zip。
输入 ZIP SHA-256：2c28ab89589004c4317def0eba5e94106599d5a7bb262047c677768a5a5d57c2。
最终 satori.exe SHA-256：a65a878893792ff00a4014d6749fbd8774c19f8a04f7ad6dc94f15bca7805812。

本版实际重新构建 Windows x64 EXE，未重新命名旧版二进制。基于上传版保留搜索输入展开修复，统一两处搜索的首尾空格和大小写处理，并扩展列表展开状态回归；版本统一升级至 0.1.4。第三方 Cargo/npm 依赖锁记录未变，浏览器扩展沿用 0.1.3。应用标识 io.habitos.desktop 和 SQLite schema 5 未变。

## 本次执行结果

| 检查 | 结果 |
| --- | --- |
| 上传源码未经修改的 Playwright 测试 | 22/22 通过，含截图中的失败用例 |
| v0.1.4 Svelte 类型检查 | 0 错误、0 警告 |
| v0.1.4 Playwright 面板测试 | 22/22 通过，扩展了原展开用例的边界断言 |
| 浏览器隐私测试 | 3/3 通过 |
| Rust 核心测试 | 73/73 通过 |
| cargo fmt --all --check | 通过 |
| 核心 Clippy（全部目标） | -D warnings 通过 |
| Windows GNU Release 全工作区 Clippy（全部目标） | -D warnings 通过 |
| Tauri Windows GNU 生产构建 | 通过，--locked、custom-protocol、无安装器 |
| EXE 格式和资源版本 | Windows PE x64；0.1.4 |
| 生产资源实际内嵌 | 当前 HTML、JS、CSS 和 7 种图标尺寸通过 |
| 辅助脚本内嵌 | ai.ps1 与 desktop-input.ps1 字节匹配 |
| DLL 依赖 | Windows 系统 DLL 和 WebView2Loader.dll，无额外 MinGW 运行时 DLL |

工具链：Rust 1.90.0、Node 24.19.0、MinGW GCC 13.2.0；Linux 交叉编译，目标 x86_64-pc-windows-gnu。复用了此前兼容的构建缓存，重新编译本版核心库与桌面程序并核对最新界面嵌入。

实际构建命令（需事先配置 Rust Windows target 和 MinGW PATH）：

```bash
CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS=1 npm run tauri -- build --target x86_64-pc-windows-gnu --no-bundle -- --locked
node scripts/verify-release.mjs target/x86_64-pc-windows-gnu/release/satori.exe
```

## 验收范围

本次未在 Windows 中启动 EXE。原生文件选择、窗口焦点、输入法、实际音量、外接屏亮度、自启动和真实 API 服务商需真机验收。本次没有重新运行 AI PowerShell/HTTP 历史回归，也没有调用真实 API；这些代码未变，Windows PowerShell 回归继续由现有 GitHub Actions 执行。未直接推送用户远程仓库，因此不能宣称其新 Actions 已通过。

交付为未签名便携 Release，需 WebView2 Runtime。完整解压后运行 Start-Real.cmd 或 Start-Background.cmd；先从托盘退出旧版。包内 SHA256SUMS.txt 记录文件校验值。

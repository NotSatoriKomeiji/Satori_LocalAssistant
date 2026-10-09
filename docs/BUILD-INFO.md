# Satori 0.1.3 Alpha · 本次 Release 构建记录

源码基础：Satori-v0.1.3-source-home-shortcuts-fix(1).zip。

输入 ZIP SHA-256：b874d5cf890b7a2b4640e656d20dd092a361879225485cab6d2b6fc713c997f8

最终 satori.exe SHA-256：4232bd9d740752e5a3b47ab1f8940160961c989eaef7cb535453a8ac218f2d34

工具：Rust 1.90.0，Node 24.19.0，MinGW GCC 13.2.0；Linux 交叉编译，目标 x86_64-pc-windows-gnu。版本仍为 0.1.3，SQLite schema 5。Cargo.lock 与 package-lock.json 与上传版字节相同。

实际生产命令（需准备 Rust Windows target 与 MinGW PATH）：

```bash
CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS=1 npm run tauri -- build --target x86_64-pc-windows-gnu --no-bundle -- --locked
```

宿主构建依赖曾出现空对象文件；清理失败缓存后改为单个代码生成单元成功构建。应用发布配置仍为 opt-level="s"、lto=true、codegen-units=1、strip=true；未更改锁文件。

| 检查 | 本次结果 |
| --- | --- |
| cargo fmt --all --check | 通过 |
| Rust 核心测试 | 73 项通过 |
| Svelte 类型检查 | 0 错误、0 警告 |
| Playwright 界面回归 | 22 项通过 |
| 浏览器隐私过滤 | 3 项通过 |
| AI 响应解析 | 14 项通过 |
| AI 系统提示词 | 23 项通过 |
| AI 本机 HTTP 模拟 | 7 项通过 |
| Windows GNU Release 全工作区、全部目标 Clippy | -D warnings 通过 |
| EXE 格式 | Windows PE x64 |
| 最新生产资源实际内嵌 | HTML、当前 JS/CSS、7 个图标尺寸通过 |
| 辅助脚本实际内嵌 | ai.ps1 与 desktop-input.ps1 字节匹配 |
| WebView2Loader.dll | 配套 x64 DLL 已包含 |
| DLL 导入 | Windows 系统 DLL 及 WebView2Loader.dll，无额外 MinGW 运行时 DLL 导入 |

## 上传版之后补齐的修改

- 搜索输入时明确展开已保存列表；保留用户展开/折叠状态与手动快捷入口。
- 添加、移除、打开反馈类别按命令决定，不从提示文本猜测。
- 用户确认 AI 应用候选并成功打开后，将确认经验写入 SQLite；回归同时验证旧确认不能再次执行。
- 旧界面测试用精确按钮名区分快捷格和完整列表；Rust 标准格式化。

未新增 API 权限、未新增通用脚本执行、未修改数据标识或数据库 schema。

## 验收范围

AI 脚本与 HTTP 模拟在 Linux PowerShell 7.5.3 运行，未请求真实服务商。界面测试使用浏览器预览与模拟数据。未在本次环境中执行 Windows EXE，Windows PowerShell 5.1、原生文件选择与焦点、输入法、外接屏 DDC/CI、真实音量、自启动、真实 API 仍需真机验收。

本包未签名；是便携 Release，未制作或验证安装向导。启动方法见 START-HERE.md，构建教程见 docs/RUST-BUILD.md。

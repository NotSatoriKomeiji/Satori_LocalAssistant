# Satori v0.1.4 Alpha · 搜索展开修复版

本版以用户上传的 `Satori-v0.1.3-home-shortcuts-source(1).zip` 为基础。上传源码已经包含搜索输入展开列表的修复；本次先在未修改源码上运行全部 22 项 Playwright 测试，全部通过。因此，截图中的 GitHub 失败记录不能用于判断上传源码仍有此错误，也不能单凭截图确认当前仓库是否已更新。

## 本版修改

- 保留非空搜索输入时展开完整列表、后台刷新不覆盖手动展开/收起的行为。
- 快捷格和完整列表使用相同的搜索词处理：忽略首尾空格及大小写。
- 扩展原失败测试：清空搜索保持展开、空白搜索不强制展开、搜索期间允许手动收起、暂停/恢复保留收起状态、再次输入搜索重新展开。
- Cargo 工作区、两份本地 crate 锁记录、npm 元数据、Tauri 配置和界面统一标为 0.1.4；第三方依赖版本不变。浏览器扩展未修改，沿用 0.1.3。

## 下载与升级

- `Satori-v0.1.4-source.zip`：完整源码、测试、锁文件、GitHub Actions、文档和许可，不包含构建缓存。
- `Satori-v0.1.4-windows-x64.zip`：Windows x64 便携 Release；完整解压后进入 satori 目录，运行 `Start-Real.cmd` 或 `Start-Background.cmd`。程序需要 WebView2 Runtime。

升级前从托盘退出旧实例。界面左下角显示 v0.1.4。应用标识仍为 `io.habitos.desktop`，SQLite schema 仍为 5，升级无需删除学习数据。

## 更新 GitHub 源码

把源码包中的 `Cyber3rdEye_Satori` 目录内容复制到现有仓库根目录（保留现有 `.git`）。同时替换 `src/App.svelte`、测试、版本文件、文档及 `.github` 内容；不要把项目目录整体套进仓库再建一层，也不要把 Release ZIP 当作源码上传。

在仓库目录检查后提交：

```powershell
npm ci
npm run check
npx playwright install chromium
npm test
git status
git add src/App.svelte tests/saved-targets-toggle.spec.ts Cargo.toml Cargo.lock package.json package-lock.json src-tauri/tauri.conf.json README.md docs/RELEASE.md docs/BUILD-INFO.md docs/RUST-BUILD.md
git commit -m "fix(panel): preserve saved target toggles and normalize quick search"
git push
```

推送后查看对应**新提交**的两项 Actions 检查；历史失败不会因新提交通过而变绿。本次没有直接修改你的远程仓库。

## 构建及验收

本次检查结果、EXE 校验值和验收范围见 `BUILD-INFO.md`。交付为未签名便携程序，不是安装向导。Windows 原生窗口、文件选择、输入法、实际音量、DDC/CI 和真实 API 服务仍需在 Windows 设备上验收。

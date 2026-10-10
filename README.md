# Satori · 本地习惯助手

**熟悉的事在本地做，陌生的事才唤醒 AI。学会以后，回到本地。**

Satori 是面向普通电脑用户的 Windows 桌面助手。无需 API 就能学习常用应用、提供快速词，以及按习惯小幅调节主音量和外接屏亮度。可选 AI 层负责解释反复出现的陌生冲突、协助澄清程序选择；经过验证的经验保存在 SQLite，后续相似场景优先离线处理。

**v0.1.5 Alpha · 状态、保存与发布验收修订 · MIT**

*A local-first Windows companion that learns your habits and wakes optional AI only when needed.*

公开版整理自内部 0.3.0 原型，保留现有功能和 SQLite schema 5，不是回退旧代码。采用 Rust / Tauri / Svelte / SQLite。

本版修复列表展开与异步状态竞争、辅助设置保存一致性及失效 AI 返回，并补全完整便携包的 CI 校验。详见 [发布说明](docs/RELEASE.md)、[实际构建验收](docs/BUILD-INFO.md) 和 [CI 风险及参考实践](docs/CI-REVIEW.md)。

## 运行

完整解压 Windows 运行包，进入 `satori`，可直接双击 `satori.exe` 打开面板，或双击 `Start-Background.cmd` 后台运行。托盘左键查看常用应用，双击打开主面板，右键暂停或退出。`Start-Real.cmd` 打开真实面板；`Start-Demo.cmd` 使用独立模拟数据库，不控制真实设备、不发送 AI 请求。

需要 Windows x64 与 WebView2 Runtime；`satori.exe` 和 `WebView2Loader.dll` 放在同一目录。切换真实/演示模式前，从托盘退出旧实例。

添加应用后会显示结果，并展开「已添加的应用与网站」。这里列出全部保存目标，推荐卡片仍最多显示六个；搜索也能找到未进入推荐位的已添加目标。暂停、关闭学习或敏感前台场景不会删除已添加记录。exe 应用旁的「移除」可取消该推荐目标，不删除电脑上的程序文件，也不更改已有学习开关或偏好；需要时可重新添加。

开机启动：打开主面板 → **设置 → 开机自启动 → 开启**。默认关闭，登录 Windows 后后台运行，只修改当前用户启动项，无需管理员权限。演示模式只切换模拟状态。建议先把运行包放到固定目录，再开启；移动或删除程序前关闭启动项，换路径后重新开启。

## 现有功能

| 功能 | 本地能力 | 可选 AI 增强 |
| --- | --- | --- |
| 常用应用与网站 | 时段、日期与前序应用参与推荐，点击才打开 | 多次出现相近候选时协助询问，确认过的选择可本地复用 |
| 自动调节 | 主音量与 DDC/CI 外接屏亮度，每次最多 5 个百分点 | 对重复纠正提出适用边界候选，用户确认后才改变边界 |
| 快速词 | 一个开关，频率与时间衰减排序，默认 50 条，点击输入 | 不向 AI 上传输入文字或词库 |
| 纠正学习 | 本次手动改动优先；两个独立场景的重复纠正形成例外 | AI 返回候选解释，本地验证，不执行模型生成的代码 |
| 经验管理 | SQLite 保存带边界的经验；可查看与忘记 | API 超时、关闭或额度耗尽不影响已有本地经验 |

普通场景自动学习，敏感软件和输入跳过。系统自己的调节、插入和没有撤销的操作不作为满意证据。

亮度针对外接显示器，使用 DDC/CI，不依赖 WMI。快速词支持部分 Windows 可编辑文本框；浏览器使用配套 Chrome/Edge 扩展，安装说明见 [浏览器连接](docs/BROWSER.md)。Enter、Backspace 和输入法按键由原输入框处理。网站观察默认关闭，当前仍需要扩展；本版没有 DNS 监听或无扩展网址读取。

![首页演示](docs/home.png)

## 我们想保留的特点

助手记住的是「什么时候该让你来决定」：单次手动纠正马上优先，两个独立场景的重复纠正形成有边界、可忘记的本地经验。没有撤销不算满意，程序自己的动作不训练自己。可选 AI 只协助解释陌生边界，确认过后就回到本地复用。

## 冷处理 API

默认本地模式，不配置服务也能使用。首页的「AI 增强 · 可选」可连接兼容 Chat Completions 的公开 HTTPS 接口。密钥只在本次程序运行的内存中，退出后需要重新连接。

0.1.2 为每次 AI 请求加入正式系统提示词，明确助手的目标、按需工作方式、敏感边界及输出协议；提示词不授予执行权，本地护栏继续负责验证。详情见 [AI 系统提示词](docs/AI-PROMPT.md)。

0.1.1 修复了部分接口返回中文乱码的问题：请求、响应和脚本输出均明确使用 UTF-8。主动回答保留换行、长文本自动折行，请求失败时清除旧回答；API 仍是可选层。

没有每步推理循环。两个独立场景出现纠正或程序歧义后，才产生待解释事件；满足开关、普通场景、冷却和预算时，单独线程调用 AI。自动唤醒最多 5 次/UTC 日，相隔至少 10 分钟；主动问答与自动唤醒合计最多 50 次/UTC 日，预算持久化，不因重启或清除经验而重置。

同一个待解释事件不会循环重试。关闭 AI、暂停助手、切换场景或更新证据会取消或丢弃过期结果。改变例外范围及打开候选程序都需要明确点击；模型没有执行权。已有经验不因 AI 关闭而失效。

自动唤醒只发送软件名称、粗略时段、独立证据次数和少量候选，不发送完整路径、设备标识、输入文字、截图、文档或完整操作历史。服务商将接收这些摘要；开启前界面有明确说明。主动问答只发送用户输入的问题。详情见 [PRIVACY.md](PRIVACY.md)。

## 试试核心循环

启动演示模式，在首页展开「试试纠正学习」，点击「模拟两次纠正」。可以看到一条本地例外，AI 唤醒仍为零；展开「看看记住了什么」可删除该经验。

自动化测试还使用替代传输验证真实可选层：关闭 API 零调用 → 开启后仅陌生事件触发 → 人工确认 → SQLite 复用 → API 失败仍保留本地规则。没有用演示假数据宣称真实设备验收。

## 开发与验证

Windows 原生开发需要 Node.js 22.12+（CI 使用 24）、Rust 1.90+ / MSVC、Visual Studio C++ Build Tools、Windows SDK 与 WebView2 Runtime。先从源码仓库根目录执行；第一次安装依赖需要联网。网页 `npm run dev` 只预览模拟界面，不能操作真实设备。

```powershell
npm ci
npm run desktop:demo
# 真实模式
npm run desktop
```

```powershell
cargo fmt --all --check
cargo test -p habitos-core --locked
cargo clippy -p habitos-core --all-targets --locked -- -D warnings
npm run check
npm run test:browser
./tests/ai-response.Tests.ps1
./tests/ai-prompt.Tests.ps1
python tests/ai_http_fixture.py --powershell powershell
npm test
npm run tauri -- build --no-bundle
node scripts/verify-release.mjs target/release/satori.exe
```

新核心位于 `crates/habitos-core/src/experience.rs`；SQLite、证据去重和经验决策在本地 Engine/Runtime；`src-tauri/src/cold_ai.rs` 只承担可选调用。模型响应是严格的限定类型 JSON，不能变成 shell、脚本、路径或任意设备动作。

本版迁移 SQLite 至 schema 5，保留旧学习数据和权限。快速词与亮度的原有偏好仍在 `assist.json`，新增例外、程序选择及调用预算在 SQLite；尚未统一所有历史存储。

**Windows 原生 UIA、输入法、DDC/CI 和真实服务商请求仍待真机验收。** 已完成的自动化、交叉编译和模拟传输验证不能代替硬件及真实 API 实测。当前没有通用多步任务、任意桌面代理、会议检测或动态代码执行。

[本版架构](docs/COLD-API.md) · [API 说明](docs/API.md) · [发行说明](docs/RELEASE.md) · [参与开发](CONTRIBUTING.md) · [首次提交 GitHub](docs/PUBLISHING.md)

## 许可与开发说明

本项目使用 [MIT 许可证](LICENSE)。第三方依赖和运行包组件遵守各自许可证，清单与许可文本见 [third-party/DEPENDENCIES.md](third-party/DEPENDENCIES.md)。源码包不包含构建缓存、运行程序或个人数据库。

本项目使用 AI 辅助设计、编码、文档和测试；产品决策与发布由维护者负责。具体边界见 [AI 辅助开发说明](docs/AI-DEVELOPMENT.md)。

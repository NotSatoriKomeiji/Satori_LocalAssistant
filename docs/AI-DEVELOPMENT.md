# AI 辅助开发说明

Satori 使用 AI 辅助需求整理、架构讨论、编码、文档和测试。产品取舍与发布由维护者负责。这是首个公开 Alpha，欢迎复现问题和检查实现。

我们的核心方向是：无需远程模型也能使用；重复陌生冲突才按需唤醒可选 AI；经确认的经验保存在本地，之后优先复用。所谓「冷处理」描述产品行为，不代表已证明全球首创，也不承诺固定节省比例或资源占用。

## 发布时负责什么

- 理解真正执行的路径：自启动如何写注册表、快速词如何插入、设备动作是否有边界、远程请求发送什么。AI 写出来不等于已验证。
- 保留可重复的测试、依赖锁文件、当前实现说明和未验证清单。模拟层与真实设备测试分开说明。
- 检查 AI 引入的依赖、复制的代码和素材来源。MIT 只授权本项目拥有授权权利的内容，不覆盖第三方许可证。
- 不上传密钥、真实用户数据库、词库、浏览器队列或未经脱敏的聊天记录。没有必要为了证明使用 AI 而公开自己的完整提示词或聊天记录。
- 贡献到其他项目时遵守对方的 AI 使用及贡献规定；不伪装成人工审查，不声称没做过的测试。

## 本版已验证与待验证

自动化覆盖本地经验、预算去重、可选层取消及过期结果、网页面板和扩展输入边界。发布核验还包含前端类型检查、Windows x64 交叉编译、Clippy 和实际内嵌资源检查，具体结果见 [RELEASE.md](RELEASE.md)。

尚未完成 Windows 真机启动项/托盘、不同软件的 UI Automation、输入法、外接屏 DDC/CI、真实付费 API 和资源占用验收。持续集成配置已提供，但首次推送前没有实际的 GitHub Actions 运行结果。当前运行包没有代码签名或自动更新。

## 官方参考

- GitHub：[Review AI-generated code](https://docs.github.com/en/copilot/tutorials/review-ai-generated-code)
- GitHub：[Licensing a repository](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository)
- GitHub：[Reusing other people's code](https://docs.github.com/en/get-started/learning-to-code/reusing-other-peoples-code-in-your-projects)
- GitHub：[Removing sensitive data](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository)

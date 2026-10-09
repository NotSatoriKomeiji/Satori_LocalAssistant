# 第一次提交 GitHub

仓库名称可以使用截图中的 `Cyber3rdEye_Satori`，可见性选择 **Public**。建议简介：

> A local-first Windows companion that learns your habits and wakes optional AI only when needed.

## 建立空仓库

GitHub 创建页不用额外生成 README、.gitignore 或许可证：这些已经包含在准备好的源码中，其中 LICENSE 是 MIT。点击 Create repository 只建立空仓库，下一步再提交代码。

## 用 GitHub Desktop 提交

源码包包含较多依赖许可文件，推荐 GitHub Desktop，避免分批网页上传。

1. 在 GitHub Desktop 中登录自己的账号，选择 File → Clone repository，克隆刚建立的 `NotSatoriKomeiji/Cyber3rdEye_Satori`。
2. 解压 `Satori-v0.1.0-source.zip`，把其中 `Cyber3rdEye_Satori` 文件夹的**内容**复制进克隆目录。确保仓库根目录直接能看到 README.md、LICENSE、Cargo.toml、package.json；不要再嵌套一层同名目录。
3. 查看 Changes，确认没有真实数据库、密钥、exe、node_modules 或 target。源码包中包括 `.github`、`.gitignore` 和 `.gitattributes`，复制时保留这些文件。
4. 填写提交信息 `Initial public alpha v0.1.0`，点击 Commit，再点击 Push origin。
5. 打开仓库 Actions 页查看 Verify Satori。配置存在不等于检查已经通过，以实际运行结果为准。

不要把源码 ZIP 当成唯一的代码文件上传到仓库：解压后的代码才便于浏览、审查和接收贡献。

## 发布运行包

在 Releases 中新建 `v0.1.0-alpha` 标签的发行版，标题 `Satori 0.1.0 Alpha`，勾选预发布，上传 `Satori-v0.1.0-windows-x64.zip`。发行说明可参考 [RELEASE.md](RELEASE.md)，保留真机未验收说明。源码和运行包分开，运行程序放 Releases。

公开后建议开启私有漏洞报告、Dependabot 告警和密钥推送保护。不要在 Issue 中发布凭据；如果密钥曾进入提交历史，应先撤销/轮换密钥，再处理历史，不能只删除最新文件。

## 许可证与第三方

本项目源代码按 MIT 发布，允许使用、修改、分发和商用，要求保留版权及许可声明。第三方依赖仍按各自许可证发布，见 `third-party/`；新增借鉴代码或素材时保留来源和授权。不要直接复制无授权仓库的实现。

`package.json` 中 `private: true` 只防止误发布到 npm，不影响 GitHub 公开源代码。

版本号 `0.1.0` 是首个公开版本，整理自内部 `0.3.0` 原型，保留已有功能与 SQLite schema 5，不表示回退旧代码。程序数据标识仍沿用 `io.habitos.desktop`，避免无意丢失已有本地习惯。

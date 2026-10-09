# Assistant API v3 · Satori 0.1.3

通过 Tauri assist_api 传入 tagged op 对象，拒绝未知字段。没有通用任务、shell 或模型动作执行接口。原有快速词与亮度接口保留。

| op | 参数 | 返回/行为 |
| --- | --- | --- |
| status | 无 | 本地助手状态和 ai.enabled / awake / message，不含密钥 |
| quick_words | enabled | 统一开关 |
| limit | limit:1–200 | 默认50条 |
| add_word / delete_word | text | 安全文本；删除加入本地抑制，手动添加恢复 |
| brightness_read | 无 | DDC/CI外接屏探测 |
| brightness_set | device,before,value:10–100 | 原值核验、读回验证、手动偏好与纠正证据 |
| ai_mode | enabled,endpoint,model,key | 显式连接可选层或关闭清除凭据；演示拒绝启用 |
| ask | prompt | 已开启层的主动问答，只有回答 |
| ai | endpoint,model,key,prompt | 旧版主动问答兼容，仅已开启层可用；无执行权 |
| experience_answer | id,revision,accept | 核验场景与版本；确认边界或点击打开已有候选 |
| experience_forget | id | 删除规则及相应证据，调用预算保留 |
| demo_correction | 无 | 仅演示数据库写入两次模拟纠正；不调用 API |

核心 Status.experience 包含 rules、pending、question、local_hits、automatic_calls_today。UI 无需编辑规则或评分。内部 Runtime 串行处理认领、完成、确认与清除；AI 网络线程不占用决策 Actor。

自动调用认领前检查学习开关、暂停、当前普通场景、权限、候选与持久化预算。开始和返回时核验会话，配置变化会使结果失效。一次事件至多自动请求一次，错误缓存为 failed/interrupted；没有无限自动修复循环。

自动学习的 AI 内容响应必须是且仅是：

```json
{"interpretation":"keep_scene","app_id":null,"explanation":"在这个场景保持手动设置。"}
```

允许 interpretation：keep_scene、hold_app_device、choose_app、no_change。数值调节事件不得提供 app_id；choose_app 只接受本次候选临时编号，映射后再次检查本地已选目标。解释最多300字符，拒绝控制字符。模型不提供数值目标、路径、脚本、新能力或权限。

自主管理例外默认30天，软件/设备/6小时时段/活动匹配；扩大至整个软件和设备须确认。程序记忆仅影响建议，仍需点击。用户确认选择后才通过现有原生核验启动目标，启动失败不记录成功选择。

## 开机启动接口

开机启动通过原有主运行时命令 `set_startup` 的 `{enabled:boolean}` 参数调用，不经过可选 AI 或 assist_api。Status.startup 返回 `supported`、`enabled`、`simulated` 和说明文字；真实模式写入后重新读取状态，错误显示给用户，不伪装已启用。演示状态不影响真实注册表。

## AI 响应编码与显示

兼容 Chat Completions 非流式响应，读取首个 `choices[0].message.content` 文本。请求和响应字节按 UTF-8 处理，不依赖 HTTP 的 charset 猜测。HTTP 响应最多 1 MiB，输出最多 32768 个 UTF-16 字符且不超过 65536 个 UTF-8 字节；空内容、非文本和无效编码作为错误返回。禁止自动重定向，网络请求限时 25 秒，外层取消及预算规则保留。

主动问答显示纯文本，保留换行并折行长文本；不执行回答中的 HTML。提交新问题清除旧结果，同一次等待期间避免重复提交。

## 固定系统提示词

0.1.2 的主动问答和自动学习共享固定目标与敏感边界，并分别追加纯文本回答或限定 JSON 协议。系统消息先于用户消息，用户文本不能改变消息角色；模型请求不包含执行工具。详见 [AI-PROMPT.md](AI-PROMPT.md)。提示词不替代本地权限、候选和场景验证。

## 程序添加反馈

桌面命令 `add_target` 仍返回 Status。成功返回包含程序名的保存消息；取消时返回明确的未添加消息，目标列表不变；验证失败返回错误。`Status.targets` 是完整保存列表，`Status.quick_apps` 是最多六个推荐目标，二者不能混同。敏感规则和用户的学习关闭不会因重新选择 exe 被覆盖。

## 0.1.3 移除按钮补丁

桌面命令 `remove_target` 接收 `{id:string}`，只允许移除已添加的 exe 应用，返回更新后的 Status。删除的是 SQLite 的推荐目标登记，不是磁盘文件；观察权限、历史偏好保留，当前程序推荐卡失效。未知或已移除 id 返回错误。版本号、数据库 schema 与可选 AI 协议不变。

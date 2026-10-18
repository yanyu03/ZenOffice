# 本任务的额度报警器

用户规则：适用有效窗口的最小剩余严格低于15%时停止本任务；每30–120分钟复盘研究并执行改进，当前调度为60分钟。额度不是API账单或可相加的总token余额；从官方usedPercent换算剩余，多窗口取最小值。

## 运行

需要Python 3.11+及本机已登录的官方Codex。后台读取app-server的account/rateLimits/read，无模型推理；不自行读取、存储或输出认证文件/令牌。默认每60秒读取，不使用预计重置时间恢复。

```powershell
# 隐藏启动；重复启动会核实PID、创建时间和命令行，复用现有监测器。
& tools/quota_guard/start.ps1
# 也可显式指定PythonPath与CodexPath。

python tools/quota_guard/guard.py --check
# 0=允许新工作；75=已锁定；76=未知/过期。

# 只停止这个本地监测器；不会结束Codex或用户任务。
python tools/quota_guard/guard.py --stop

# 非推理即时刷新；路径替换为本机已安装官方codex.exe。
python tools/quota_guard/guard.py --once --codex C:/path/to/codex.exe

# 保护自己启动的直接子进程；门禁关闭时终止并核实退出。
python tools/quota_guard/guard.py --run python my_bounded_job.py

# 仅在用户明确要求恢复后使用；仍要求新的有效剩余额度>=15%。
python tools/quota_guard/guard.py --resume --codex C:/path/to/codex.exe
```

start.ps1默认查找Codex桌面内置Python及已安装Codex路径；其他环境使用-PythonPath和-CodexPath。没有设置开机自动启动；本聊天heartbeat会在必要时恢复监测器。

运行数据在target/quota_guard，均被Git忽略：state.json（权威门禁）、STATUS.md（人读状态）、ALERT.json（低额度锁定记录）、daemon.json（PID与创建时间）、command-*.json/log（自有命令的结果）。旧状态过期150秒或查询失败时拒绝新工作；旧有效值可展示，但不构成允许执行的依据。低额度锁定跨重启保留，恢复额度本身不会自动解锁。

## 停止与通知

检测到低额度后写ALERT，Windows尝试播放一次系统提示音。声音取决于系统声音设置；可靠记录是本地告警文件。本任务及60分钟heartbeat在检查点保存交接、停止新工作并暂停自己，通知用户有意义的额度停止；状态未改变时不反复通知。暂停状态保留，需用户明确恢复。

桌面外部控制socket本轮探测不可连接。该报警器不能硬中断桌面模型生成、其他聊天或远程CI；不是服务端逐请求限制。运行器只终止自身直接创建且仍有句柄的子进程，不声称所有派生进程都已停止。不得杀死Codex、其他用户进程或用模拟低额度来中断真实聊天。最长检测间隔约60秒，执行层在检查点遵守停止。

## 验证

```powershell
python -m unittest discover -s tools/quota_guard -p test_guard.py -v
```

隔离测试覆盖阈值边界、多窗口、异常/未知/过期、持久锁定、并发迟到写入、显式恢复，以及一个真实自有测试子进程停止。不触碰真实用户任务、不改变账户额度；不将这些测试说成桌面硬中断验收。

官方接口依据：[App Server](https://learn.chatgpt.com/docs/app-server)。调度限制见[Scheduled tasks](https://learn.chatgpt.com/docs/automations?surface=app)。实际周期配置由应用自动化工具维护，不手改应用调度文件。

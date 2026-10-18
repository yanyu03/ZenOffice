# 用户运维实况：关机与睡眠后的任务恢复

日期：2026-10-18（Asia/Shanghai）。用户请求：避免关机或机器睡眠导致循环任务崩溃。类型：用户直接授权的运维事项USER-OPS-CONTINUITY-001，独立于冻结产品任务；任务书v1.0及原始输入不修改，ENG-003仍为下一产品任务。

## 实施

1. tools/continuity提供本地周期begin/checkpoint/finish/inspect，记录步骤、HEAD、工作区补丁、允许的未跟踪源码副本及CI/命令句柄；JSON与源码副本刷新到磁盘，原子替换权威状态。ACTIVE不会因老旧自动解锁，重复begin拒绝，错误周期ID不能接管；不自动应用补丁、不重放命令。
2. Windows任务ZenOffice-Continuity-Recovery已注册，登录后30秒/系统恢复事件后20秒/每5分钟检查；StartWhenAvailable=true，IgnoreNew，失败每分钟重试3次，交互用户/最低权限，无密码/自动登录。WakeToRun=false，未改全局电源计划。
3. 监督器只在原zenoffice自动化ACTIVE时幂等恢复额度监测；每次开机最多请求一次实际桌面包OpenAI.Codex的激活。同次开机手动关闭应用后不周期强开；不会解除额度锁、恢复暂停的heartbeat或创建模型推理循环。
4. quota guard的受控命令增加线程范围的SetThreadExecutionState防自动睡眠请求，成功/异常后恢复原线程状态；不强制亮屏、不阻止用户手动睡眠/关机。额度JSON先flush/fsync再替换，可捕获中断记INTERRUPTED；断电残留RUNNING需核实而不是重发。
5. 原60分钟heartbeat通过应用工具增加恢复协议，身份/ACTIVE/间隔及额度停止保留，读回无编码替换字符。每步保存检查点，恢复后只推进适当的一轮，不逐个补跑所有错过周期。

## 本机证据

- 12项Python恢复专项通过：检查点与收尾、重复与并发begin、错误cycle、无年龄解锁、写失败保持旧记录、预算、未跟踪源码复制及二进制排除、旧命令需核实、睡眠请求恢复、请求失败不误报、可捕获中断持久化。
- 3项PowerShell隔离用例通过：首次写入、重复原子覆盖、boot身份JSON往返稳定；不注册系统任务或启动应用。
- 原23项额度门禁回归通过，106项迁入身份/语法/样本静态检查通过；未重跑无变化的Rust产品代码255项。
- 本机Windows电源API一次请求成功并释放（单次调用观测，不是实际自动睡眠验收）。恢复计划任务实际执行返回LastTaskResult=0，随后23:22:58（Asia/Shanghai）的自然5分钟触发亦返回0（单次周期观测）；重复启动监督器成功，app_action=not_relaunched_same_boot，未重开已经运行的应用，监测器复用既有PID身份。
- 已实际创建begin及多次checkpoint，补丁和源码副本只在被Git忽略的target/continuity。cycle COMPLETE仅代表本轮运维收尾，不表示任何产品阶段完成。

本地保存target/continuity/installed-task.xml、supervisor.json、recovery-inspection.json、startup.json、cycle.json、checkpoints、receipts及旧自动化提示备份；本机路径/标识/运行数据不上传公开仓库。远端Windows验收：恢复专项[39140441096](https://github.com/yanyu03/ZenOffice/actions/runs/39140441096)在91e47870a057aec018fce044f54741e043b3d012通过，含12项Python恢复、3项PowerShell及23项额度回归；独立额度CI[39140346610](https://github.com/yanyu03/ZenOffice/actions/runs/39140346610)在c79fb08通过。原Rust产品未变，不重跑255项。

## 发现并修复的问题

重复启动时，PowerShell把File.Replace的null备份路径转成空串，覆盖状态失败；改用当前pwsh/.NET的同目录File.Move覆盖。随后发现ISO启动时间经ConvertFrom-Json转换成DateTime造成身份比较不稳定，改为不透明boot键。两项均添加PowerShell回归，并在真实监督器重复启动/计划任务中复核。失败状态保留在本地旧supervisor-error记录中，按时间区分于后续成功状态。

首次远端恢复CI[39140346631](https://github.com/yanyu03/ZenOffice/actions/runs/39140346631)保留：Windows测试临时路径含8.3短名称，测试夹具ROOT未像生产代码规范化，路径边界将模拟源码误判为外部而未复制。修正夹具root.resolve，与生产ROOT一致；没有放松边界或删除断言。后续完整专项通过。

## 限制与复现/撤回

未进行真实关机、断电、睡眠、登录、应用缺失激活的系统实验；当前用户会话不被拿来做破坏性验证。注册触发器不是未来必然成功的证据。机器关闭期间模型与本机任务不运行，恢复依赖开机、交互登录、应用和项目可用；原模型内存/工具连接不保证无缝继续。恢复从最近完整检查点和实际命令/CI状态开始，旧进程PID不作活性证明。新二进制/大证据不在源码检查点范围，仍需独立归档。

复现：python tools/continuity/test_continuity.py；pwsh -NoProfile -File tools/continuity/test_recovery.ps1；python -m unittest discover -s tools/quota_guard -p test_guard.py。本机安装命令与具体检查点操作见tools/continuity/README.md。

撤回：先核对任务描述，再停用/移除ZenOffice-Continuity-Recovery；原先存在同名任务时按本地XML备份恢复。保留检查点，不递归清理未知目录；需要停止业务周期时另暂停原zenoffice自动化。防自动睡眠请求在受控命令退出后释放，不需恢复电源计划。

官方依据：[OpenAI项目自动化运行条件](https://learn.chatgpt.com/docs/automations?surface=app)、[Microsoft线程电源请求](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate)、[Windows任务设置](https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtasksettingsset?view=windowsserver2025-ps)。这些说明边界，不代替本机系统实验。

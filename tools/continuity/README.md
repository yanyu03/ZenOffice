# 关机、睡眠与循环任务恢复

这是一层本地恢复保护，不是在关机期间运行模型。项目自动化仍要求电脑开机、用户登录、桌面应用运行且仓库可用；重新打开后等待应用下一次调度，不承诺补跑每个错过的周期。[OpenAI运行条件](https://learn.chatgpt.com/docs/automations?surface=app)

## 已实现

- Windows计划任务ZenOffice-Continuity-Recovery：登录后30秒、系统恢复事件后20秒及每5分钟检查。StartWhenAvailable开启，IgnoreNew防重叠，失败每分钟重试最多3次；只在用户交互会话运行，不保存密码、不自动登录。
- 只在原zenoffice自动化ACTIVE时维护额度监测器。暂停或额度锁定不被自动解锁；它不创建模型任务，不替代原60分钟heartbeat，也不重放业务命令。
- 每次系统启动最多请求一次桌面应用激活；本机通过已安装OpenAI.Codex包的AppID发现应用，未误把内置codex.exe命令行当桌面程序。同次启动中用户主动关闭应用后不每5分钟强行重开。
- WakeToRun关闭，尊重手动睡眠/关机；不改全局电源计划。重启后必须先登录，随后才可能恢复桌面应用和原调度。
- guard.py --run执行受控命令时在当前线程申请ES_SYSTEM_REQUIRED，结束/异常后恢复先前线程状态，不强制亮屏，不阻止用户手动睡眠或关机。[Microsoft电源API边界](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate)
- 周期检查点保存HEAD、Git状态、已跟踪工作区补丁、允许的未跟踪源码副本、步骤与命令/CI句柄。原子替换JSON前flush/fsync，NTFS保存最近成功的完整记录；临时不完整快照不冒充成功。文件系统/硬件故障下不保证零丢失。

## 每周期调用

```powershell
# 先额度、只读任务书检查，再检查残留周期/命令。
python tools/continuity/continuity.py inspect
python tools/continuity/continuity.py begin --task ENG-003 --step "before implementation"
# 保存返回的cycle_id，不能另造ID替换一个ACTIVE周期。
python tools/continuity/continuity.py checkpoint --cycle <cycle_id> --step "CI running" --handle "CI:<run-id>"
python tools/continuity/continuity.py finish --cycle <cycle_id> --step "actual record and CI verified"
```

已有ACTIVE周期先读cycle.json、其最近checkpoint及实况记录。核对进程创建时间/路径与输出，不凭PID或时间已久认定失败；CI按记录或已推提交查真实状态。确认上轮已结束或需要续做后，在同一cycle上保存checkpoint再继续；未完成的命令不能重复启动。失败/暂停也可finish，step明确真实状态；COMPLETE仅表示周期已收尾，不表示产品任务DONE。

工作区已有改动先保持。确需恢复时在本地检查补丁及源码副本，先git apply --check，不自动覆盖或重放命令。快照不含模型内存状态，不能从断电位置无缝接续一个模型请求。原实况和下一步仍是恢复依据。

快照限16MiB，未跟踪文件仅复制rs/toml/lock/py/ps1/md/json/yml/yaml；忽略目录、点文件、外部链接及其他扩展不复制并明确列出。大型或二进制新证据需另行本地保存/研究归档，不能把这个源码检查点称为整机或全部证据备份。快照只保存在Git忽略的target/continuity，含本机路径/句柄，不上传公开仓库。

## 安装、核查、撤回

```powershell
& tools/continuity/install.ps1 -PythonPath <已安装Python3.11以上路径>
Get-ScheduledTaskInfo -TaskName ZenOffice-Continuity-Recovery
Get-Content target/continuity/supervisor.json
python tools/continuity/test_continuity.py
python -m unittest discover -s tools/quota_guard -p test_guard.py
```

安装器只更新带本工具描述的同名任务，保留之前的任务XML，幂等；启动器、任务XML和运行状态在target/continuity。成功调度一次不证明下一次登录/唤醒一定成功；依赖安装路径、操作系统及应用可用。

撤回：先核对同名任务描述，再Disable-ScheduledTask，必要时Unregister-ScheduledTask -TaskName ZenOffice-Continuity-Recovery -Confirm:$false。原先存在的同名定义按本地备份恢复；保留检查点供人工核查，不递归删除未知目录。关闭恢复计划任务不改变原60分钟heartbeat，需要停业务周期时另暂停原自动化。

本次是用户明确授权的运维请求，单独记录在docs/operations；不修改冻结任务书或把运维测试算作引擎功能验收。尚未做真实关机、睡眠、登录或桌面应用缺失时的启动实验，避免中断用户当前工作。

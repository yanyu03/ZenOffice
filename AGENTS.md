# ZenOffice 工作约定

先读README、docs/taskbooks/ZEN-ENGINE-v1.0.md、docs/ENGINE_BACKLOG.md与最新基线/循环/实况记录。2026-10-18用户最新要求为引擎与基本功能优先，UI后置；冻结任务书是当前验收标准，旧ENGINE_DEVELOPMENT_PLAN_2026-10-18.md保留为历史方向。

- 每次任务及heartbeat先检查额度，再运行python tools/taskbook/calibrate.py；任务书、tasks.json及lock.json只读，哈希或只读属性异常时停止并记录，不解除只读、不按新内容刷新哈希。依赖和未完成句柄核实后，用--init <任务ID> --output docs/task_runs/<唯一轮次>.json开独立实况；结束逐条记录PASS/FAIL/NOT_RUN、样本数、命令、证据、偏差、未测及下一步，运行--report <实况.json>并再次核验基准。未完成填PARTIAL/BLOCKED/FAILED，不能降低条件后标DONE。标准变更只提案并经用户明确要求新建版本，v1.0不回改。详见tools/taskbook/README.md。

- 当前仓库是公开Rust核心基线。按E1共用会话、E2/E3表格、E4基础Word、E5基础Slides、E6引擎可用版推进；UI排到E7。每阶段交付可保存重开的真实任务，不同时启动多套前端。
- 开发只在本仓库进行；相邻zen-office-v2为历史来源，不覆盖其证据和冻结输入。旧自动化未迁移，不能假设其正在推进本仓库。
- 保留原包与未知部件，未支持编辑明确拒绝，错误与目标锁定不得损坏原件或旧输出。
- 区分代码实现、Rust测试、原生宿主对照、桌面验收。报告必须对应提交、工具链和输入身份，不引用旧通过数充当当前结果。
- 默认检查：python tools/check_baseline.py；cargo build --locked --release -p zen-harness --bin zen-deliver；cargo test --locked --workspace --all-targets。按变更范围选择检查，稳定后的重复全量运行需有新理由。额度工具变化只运行其Python专项与基线静态检查，不重跑未改动的Rust全套。
- 外部SIU模板只可选择性下载并核验，不入Git；外部测试明确ignored，缺少必需自制样本则失败。禁止静默跳过后报告通过。
- 不导入WPS图标、资源包、反编译输出、SDK、现成可执行文件或本机凭据。新UI必须使用来源明确的资源并能在无WPS环境运行。
- BASELINE_IMPORT.json的原始迁入哈希不可重写；后续变更如影响当前校验哈希，必须记录变更原因与新验证身份，再更新对应当前指纹，不篡改原始来源记录。
- 普通源码工作不需要新增自动化；迁移既有自动化前核对其指向目录、待办与用户通知意图。
- 用户已授权本聊天每60分钟复盘研究与执行改进（允许30–120分钟内有理由调整）。当前heartbeat为zenoffice；保留同一身份，不创建重复调度。额度报警器每60秒读官方额度；任务每轮开始、实质工作和较重命令前执行tools/quota_guard/guard.py --check，并用官方额度工具核实。
- 有效适用窗口剩余严格低于15%或门禁锁定时，保存target/quota_guard/PROGRESS_AT_HALT.md，停止实现、研究、测试与推送，暂停本聊天heartbeat和active goal。用户已明确授权这一条件停止；不要再次询问。额度未知或过期不启动新实质工作。恢复需要用户明确指令及有效非低额度；不可凭倒计时解锁。
- 预计超过60秒的本机命令优先通过guard.py --run执行或采用有界步骤并在间隔检查。运行器只验证终止自己直接拥有的子进程；派生进程与外部CI需单独核实。不宣称桌面模型硬中断或全局强停已经接通。

- 用户已要求用 DSHA 的 REA 工具驱动调查；加载本机 `$dsha`，遵循 docs/REA_ENGINE_WORKFLOW.md。明确产物和问题，保留身份、证据与限制，再转为独立引擎实现及功能对照；普通可读源码仍用仓库工具。环境验收不计入引擎功能完成度。

# 引擎待办与交接

当前阶段：E1。最新产品实测：745545d，Windows CI的zen-deliver Release构建与274项Rust测试通过、6项忽略；ENG-003四条冻结条件通过（可信本地父目录/create-new范围），未做Office宿主对照。当前冻结任务书：taskbooks/ZEN-ENGINE-v1.0.md；15个任务、57条条件，逐任务实况另存task_runs，不能回改任务书。

| ID | 状态 | 下一步或证据 |
| --- | --- | --- |
| ENG-001 | DESIGN DONE / 未实现 | WORKBOOK_SESSION_CONTRACT.md：已核查现有接口、确定复用边界、状态转换与失败原子性；无新运行能力主张 |
| ENG-002 | DONE / 内存范围 | 4dd773d：zen-session数字编辑、不可变字节帧、undo/redo、分支/无变化与失败不变；9项新测试，2份独立工作簿；实况2026-10-18-004-ENG-002.json |
| ENG-003 | DONE / 限定保存范围 | 745545d：CLI、安全新目标保存、保存点与故障原子性；新增4项实际junction/大小写/点路径验收；C1–C4 PASS。仅可信父目录/create-new，不承诺目录命名空间CAS；见2026-10-19-008-ENG-003.json |
| ENG-004 | TODO | 内部端到端后，独立业务样本与同等修改的Excel对照 |
| ENG-005 | TODO | 定义类型编辑、公式与依赖扩展的准入边界 |
| W-001 | OPEN / 后续专项 | 横向字宽与兼容设置未修复，不阻塞表格会话主线 |
| RES-001 | DONE / 静态范围 | OFFICE_RESEARCH_BASELINE_2026-10-18.md：8模块身份/资源、3类型库、3小模块Ghidra；未开展Office运行对照；定稿后的回溯校准见task_runs/2026-10-18-002-RES-001.json |
| RES-002 | NEXT / M1对照依赖 | 3份独立工作簿、逐份3次Excel观测，先固定输入与预期；对真实差异再定位核心实现 |
| RES-003 / RES-004 | TODO / 后续研究 | Word范围/段落/撤销与PowerPoint身份/关系，分别按冻结任务书执行 |

循环报告位置：docs/engine_cycles/。未保存的本机额度状态、告警、暂停交接与进程记录只放target/quota_guard，不上传账户或线程身份。

周期任务只写当前ZenOffice仓库，不操作旧zen-office-v2。处理用户未提交修改时先检查，避免覆盖；工作开始前读取额度门禁与最新交接。

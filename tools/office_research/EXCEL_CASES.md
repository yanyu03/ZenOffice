# RES-002固定输入与宿主对照

首版输入位于RCD/engine-acceptance/res-002-v1，三份独立自制业务工作簿：预算、订单、考勤。plan.json在创建工作簿之前声明输入/公式、初始及两次数字修改的精确预期、错误探针、日期体系和比较规则；manifest.json固定计划及三份XLSX的SHA256。普通核验不写文件，不能重算哈希以消除差异。

```powershell
python tools/office_research/verify_excel_cases.py RCD/engine-acceptance/res-002-v1
```

核验包含ZIP完整性、业务标识/输入、公式文本、初始缓存、唯一故意设置的#DIV/0!、Excel1900日期体系及3份×3阶段的独立有理数预期；不是Excel或Zen执行。所有选定数字和结果都是整数或二进制精确半数，数值容差预设为0；公式、错误、标识精确比较。当前版本只测试基础数字编辑，文本、日期体系更改和错误写入不在编辑序列内。

宿主协议已在plan.json预先声明：每次先验哈希与当前EXCEL.EXE身份，每个输入从原始副本独立跑3次；使用可确认归属的新实例及隔离副本，不能附着用户实例。每一步重算并记录Value2、Formula、Text、错误及Date1904；另存新目标、关闭重开，读取OOXML公式/缓存。需另外固定PID、创建时间/路径、应用版本、计算模式/区域配置及逐步原始结果，输入前后哈希相同。只关闭自有工作簿和实例，不结束用户Office进程。2026-10-19已完成observe_excel.ps1驱动的9次独立实例运行，逐阶段与缓存由verify_excel_observations.py核验，独立摘要见docs/research/RES-002-EXCEL-2026-10-19.json。冻结manifest中的host_runs=0是输入封存时的历史状态，不是实时进度字段，不能回改。每次运行目录必须是新目录；已有目录先读receipt和核对实际进程，禁止自动重放。

Excel的Application.Undo只撤销上次界面动作，不能撤销VB命令，[微软官方说明](https://learn.microsoft.com/en-us/office/vba/api/excel.application.undo)。COM数字编辑不以该方法测试撤销；若对照中按逆序恢复旧数值，须标明这是显式逆向赋值，不是原生Undo。Zen撤销/重做由其独立会话验收负责，不把宿主自动化当作其替代。

本轮重新采集的EXCEL.EXE文件版本为16.0.17932.21000、SHA256 c71c7ca0faf896fd52e66924f7ec43ab8b7b110fa3b179097c1f0e3c6c036369，与2026-10-18静态基线的21056/c689…不同。旧基线保留，原因未知，不解释为已确定的升级或回滚。清单记录的是文件元数据，不是本轮Excel Application.Version/Build；宿主执行前必须再次采集，不借用旧身份。

生成器prepare_excel_cases.mjs使用桌面应用load_workspace_dependencies提供的Node与@oai/artifact-tool。先在target中创建专用工作目录，将其node_modules用junction指向加载器给出的node_modules；不在仓库安装依赖。按电子表格技能在首次创建前运行mark_artifact_operation_started.mjs（create、3个xlsx）。然后：

```text
<bundled-node> tools/office_research/prepare_excel_cases.mjs <runtime-junction-dir> <new-output-dir> <local-evidence-dir>
```

输出目录必须不存在；不能覆盖已冻结输入。渲染图及生成器inspect输出放本机证据目录，作者先核对初始数值并逐页查看，再检查包/独立预期，初次使用verify_excel_cases.py --seal-identity <已采集身份.json>生成清单；该操作拒绝已有manifest。新输入应另建版本，经记录和审查后使用，禁止对旧清单重新封存。生成器含运行库导出行为/时间戳，不承诺重生成字节完全相同；原始冻结字节为后续宿主对照身份。

测试只用自制数值和简式公式，没有厂商载荷或反编译结果。仅在真实宿主差异出现后才按DSHA/REA针对明确EXCEL.EXE身份和具体问题调查，保留未定位结果。回滚本轮工具/输入按Git提交处理，保留实况与原始证据，不修改冻结任务书。

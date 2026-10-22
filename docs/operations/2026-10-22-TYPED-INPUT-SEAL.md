# 三业务类型输入身份封存

日期：2026-10-22。设备与固件：本机Windows x86_64，固件未测。工具与版本：DSHA/Python3.12.14，Git；源码起点4fafdfb。样本数：预算、订单、考勤3份独立业务输入；7个核验器测试方法。是否实测：是，原件/复制件哈希、独立包策略、日期解释和只读属性。结论强度：输入准备与身份固定，不是引擎功能验收。

## 准入决定与身份

复核原计划construction.sealing及既有证据：实际6张全表视图已检查布局/标签/数字；3张独立日期卡已检查日期体系/序号解释；本轮将9张图像实际文件哈希与公开证据逐一核对。重新读取三候选，以独立包策略和日期解释器逐份核对初始数值、标签、公式/缓存、日期体系/样式、manual calcPr及自有保留探针，共16个初始公式/缓存。

据此固定后续操作测试输入，不修改原计划的操作、预期、容差和冻结任务书。原计划日期比较本就将Value2、体系、样式分别核对，Text是观测项而非相等预期；本轮不把第三方预览的失败当作通过。订单artifact-tool2.8.99的1904显示差异明确写入清单，独立日期卡补充原始数据的解释；它不是原生Text或完整格式兼容验收。

新目录RCD/engine-acceptance/eng-005-business-v2，用create-new复制3份原包和原计划plan.json，最后写manifest.json，没有工作簿重新导出或元数据修补：

| 文件 | SHA256 |
| --- | --- |
| budget.xlsx | c76852a1ad28b717fb9b44ea3f0afd7a846c19862776dabf8065cc7b65035531 |
| orders.xlsx | 71d0703cc63d1ab66284aadcff3d54fe0f09f47102068306726c6c0fa7cd8d96 |
| attendance.xlsx | ea4395387368189d5e05451776cca51e56a439836757325b734af7e8e8659ea2 |
| plan.json | ebd6c6dd390a4a1890eb2b1ec78f54d18d828ab8a1201d65676849bded820dba |

manifest.json SHA256为bc82f786f0db1c842188dbdb891b4b6ef3c2f0af5f4c026fe4ae19bf428aaad4。其id为ENG-005-business-v2-inputs-v1；输入计划版本仍business-v2。五个新文件本机只读；Git不保存Windows只读属性，身份以外部固定清单SHA和逐文件SHA为准。新增.gitattributes仅保护新目录精确字节；原RES-002-v1、旧失败候选及冻结任务书不变。plan.json内runtime_status仍是计划编制时的0，不回改计划以反映本轮，当前事实见本实况与清单。

## 核验器与结果

新增verify_typed_inputs.py，只读且调用方必须提供独立保留的清单SHA。先核对清单/4文件身份和三业务唯一集合，再执行包/日期初始策略，结束再次对比所有字节。没有自动重算清单、修改或修复入口。不把审阅字段当作人类审查的机械证明；可信本地父目录范围，未承诺敌对命名空间原子快照。

7项专项PASS：正常三样本/无写入、清单变化/缺少pin、逐文件变化且不修复、缺失/越界路径、重算身份但错误体系的计划、重算身份但calcPr错误的包、重复JSON键/案例。故障只操作target内自有副本。独立进程复核真实封存目录PASS（3文件、16初始公式/缓存、源字节保持）；源码基线106PASS。专项与固定清单核验加入既有CI，不新建调度。

复现：python tools/office_research/test_verify_typed_inputs.py --log <新日志>；python tools/office_research/verify_typed_inputs.py RCD/engine-acceptance/eng-005-business-v2 --manifest-sha bc82f786f0db1c842188dbdb891b4b6ef3c2f0af5f4c026fe4ae19bf428aaad4。实际执行经dsha.py envfeel --require python --run python --，见target/office_research/ENG-005-seal-032。完整[证据索引](../research/ENG-005-SEALED-INPUTS-2026-10-22.json)包含复制/核验脚本、日志、源码身份。

## 后续与限制

本轮封存输入数3，Zen/Excel/ONLYOFFICE新业务执行仍0；ENG-005 PARTIAL，C1至C4本轮NOT_RUN。下一步开发使用这套固定输入/计划的Zen API类型执行器，覆盖8步操作、独立预期、逐步undo/redo与保存重开；之后运行三份各三次独立Excel同操作观测。现有数字CLI/观察器不能直接当作类型验收。

回退按提交撤销本轮核验工具/CI/新输入与文档，不动原任务书、原计划、历史样本和失败证据；封存目录不原地重建。最可能被推翻的三点：窄策略不是完整schema验证；原生宿主可能暴露新准入问题；日期解释卡不证明builtin14本机Text/版面等价。出现差异应保留此版本另记，不改输入迎合结果。

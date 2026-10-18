# Zen引擎 Office研究依据快照

日期：2026-10-18。用途：作为Zen后续设计、实现与验收的可追溯依据。分支：research/office-baseline-2026-10-18。

本快照保存8模块输入身份及资源事实、3类型库的聚焦接口声明、3小模块的独立函数观察摘要、4项未知及44份本地原始证据的SHA256索引。样本均为原2026-10-18批次的单次静态观察，本次归档未重新进行Ghidra分析或Office运行测试。不能据此声称计算/排版核心已还原。

## 数据与计划

- input-identities.json：版本、大小、签名状态、完整输入SHA256。
- resource-observations.json：8模块资源数量/覆盖范围、TYPELIB载荷偏移与哈希；没有载荷本体。
- interface-observations.json：聚焦对象/成员/DISPID/调用种类的声明事实；没有完整类型库转储。
- native-observations.json：模块概况、证据ID、独立观察摘要和未解边界；没有原始伪代码或汇编文本。
- residual-unknowns.json：4项未闭环问题及建议探测。
- raw-evidence-index.json：44份本地原始响应的哈希索引，索引不是原始载荷的异地备份。
- archive-manifest.json：本目录文件与冻结任务书/依据报告/复现脚本的SHA256。

仓库内原始位置：docs/taskbooks/ZEN-ENGINE-v1.0.md（15任务/57条件，只读标准）；docs/OFFICE_RESEARCH_BASELINE_2026-10-18.md（研究报告）；docs/task_runs（独立实况）；tools/office_research（复现脚本）；tools/taskbook（逐任务校准）。本分支包含其完整Git快照，不随main后续推进自动变化。

## 核验与复现

从本分支的仓库根目录运行 `python research/office-baseline/2026-10-18/verify_archive.py`，核对归档和引用文件的精确哈希。原任务书字节保持不变；Git不保存Windows只读位，新检出按任务书工具说明先核验哈希再设置只读属性。

要复做调查，参照依据报告，使用已安装且明确指定的Office产物、DSHA/REA环境及全新输出目录，记录新版本身份。不得把新批次冒充本批次，也不能为配合进度回改冻结任务书。

厂商二进制、资源载荷、完整类型库转储、原始反编译输出和本机凭据不随公开分支发布；原始派生证据仍在本机target/office_research/2026-10-18-01。当前原始证据只有本机保存，未声称另有远程完整备份。

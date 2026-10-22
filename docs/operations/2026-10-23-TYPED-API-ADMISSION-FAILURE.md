# 类型业务执行首次失败与准入复盘

日期：2026-10-23（Asia/Shanghai）。设备与固件：本机Windows x86_64，固件未测。工具与版本：DSHA Python3.12.14、Rust/Cargo1.99.1、MSVC SDK子进程激活PASS。样本数：封存3业务输入；正常路径3测试失败、独立拒绝回归3测试通过。是否实测：是，本机编译和API调用。结论强度：所需业务路径被准入门禁阻断；不证明保护本身有错，不是功能验收通过。

## 预定任务与实际失败

原清单SHAbc82f786…和三输入先核验，冻结任务书通过。实现Windows Rust API业务执行器，读取封存plan.json原8操作及独立字面预期，意图验证全快照、字节级undo/redo、元数据保持、3次保存重开。没有从引擎结果计算预期。首次包装器把-- --nocapture传给quota_guard，argparse启动失败，Cargo尚未执行；该日志留存。随后仅改本地包装脚本的子进程RUST_TEST_NOCAPTURE变量，不改系统环境。

真实Cargo编译成功，3测试均在第一步文本请求拒绝：budget/review-text、orders/status-text、attendance/approval-text，错误均为unknown custom XML dependency。实际成功修改0步、完成8步案例0、保存/重开0。真实日志来自target/quota_guard/command-367699c40d47468f93bb6c8530387cdf.log，副本cargo-actual-child.log留在target/office_research/ENG-005-api-033；包装器外层空重定向不计测试证据。

## 机制与纠正

crates/zen-opc/src/preserve.rs的现有保护扫描customXml/*.xml，仅接纳已知bibliography Sources及受限datastoreItem。三输入按原计划包含独立自有customXml保留探针，其命名空间未被接纳，写入口因此拒绝。包结构/初始值策略PASS不能证明写入口接纳该包。

补充纠正实况032：封存3份只完成输入身份和准备证据，不能解释为已经通过Zen可编辑准入。本轮实际调用暴露了这层缺口。清单及输入身份仍有效，保留作为失败样本；没有删除探针、修改日期体系、重算清单或添加只针对测试命名空间的白名单。也不把负向通过数算进原8步验收。

失败执行器源码保存在target/office_research/ENG-005-api-033/typed_business.failed.rs，临时serde_json测试依赖及Cargo.lock单一依赖边变更的差异同时保存。它们仅为本轮新增，已从待提交工作区移出；失败执行器不推送main。没有回滚用户修改或生产引擎代码。

## 独立负向回归

新增crates/zen-session/tests/typed_business_admission.rs，使用原3业务字节分别提出首个文本意图，明确预期unknown custom XML dependency，并核对源字节、revision、帧数、保留字节、dirty、undo/redo不变。3项本机测试PASS；这是当前拒绝边界的回归保护，不是业务完成。被拒后的完整8步历史/保存路径尚未测。

复现：先python tools/office_research/verify_typed_inputs.py RCD/engine-acceptance/eng-005-business-v2 --manifest-sha bc82f786f0db1c842188dbdb891b4b6ef3c2f0af5f4c026fe4ae19bf428aaad4；经DSHA envfeel声明python/cargo后，在SDK子环境中运行cargo test --locked --offline -p zen-session --test typed_business_admission，长命令用quota_guard --run。Cargo日志由guard的command-*.log保留，不能只读取外层stdout。完整[证据索引](../research/ENG-005-API-ADMISSION-FAILURE-2026-10-23.json)记录源码、失败与通过日志身份。

本轮实况FAILED，ENG-005整体仍PARTIAL/BLOCKED于custom XML写准入。C2本轮FAIL记录3首次文本路径未达预定行为；C1/C3/C4全条件NOT_RUN，负向原子性另记。新Excel/ONLYOFFICE运行0。源码基线106PASS，封存输入/计划保持。

下一步先明确自有/不透明custom XML保留的依赖风险与准入契约，提出可独立验证的通用边界；证据不足则继续拒绝，不能为样本特判。经该实现与故障回归后，在同一封存输入重跑原8操作。此为实现准入缺口，不要求回改冻结任务书。回退本轮仅撤销新增负向测试和文档，保留封存及失败日志。最可能被推翻的三点：拒绝字符串依赖既有实现；本次未证明所有custom XML都不可安全保留；正常路径尚未测到后续类型/公式/日期差异。

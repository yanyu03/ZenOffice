# 原生类型对照身份门禁停止

日期：2026-10-23。设备：Windows x86_64运行时。工具：Python3.12.14、Cargo/Rustc1.99.1，DSHA声明工具/ownedhealth/SDK PASS；Windows PowerShell执行器只运行到产物哈希前置检查。样本数：3Zen业务重跑、5比较器方法；原生样本0/实例0。是否实测：以下日志实际记录。结论强度：前置身份拒绝，不是Office功能失败。

周期038／实况040；开始代码310c35a。新增observe_typed_excel.ps1、verify_typed_excel.py及5项独立比较器方法。固定原清单/计划，比较器单测PASS，当前Zen原三业务重跑PASS。

首次预算pilot在创建run目录及Excel COM之前，检测指定EXCEL.EXE与既有批准SHA c71c7ca0faf896fd52e66924f7ec43ab8b7b110fa3b179097c1f0e3c6c036369不一致，退出1。DSHA context/python/health PASS、task TASK_FAILED。原生修改/保存/重开均0。新哈希、版本、变化原因未继续探查，不推断为更新或安装故障。

原日志：target/quota_guard/command-52d86965ac154d87b5dc5d22c6f0fdee.log，SHA256 c369c221301163ea3ce1b1a79cb8fc9513fafb1d0dd6d404620521bfe912ff2e。Zen重跑日志：target/quota_guard/command-46a4c197219d4e50a6097941a0ebfb59.log，SHA256 749272fb3b408c0d12aa638ead881e56957af394d630d1ffbc0016b3817a8928。EnvFeel原始回执及新工具上下文留target/office_research/ENG-005-native-040。taskbook/原输入清单均未修改。

DSHA SKILL.md规定“known condition already requires refusal, end the current task immediately”。因此停止，没有重算/替换批准哈希、降条件、替代工具或重试。只完成用户明确要求的实况/恢复检查点；本轮未提交或推送，未验证observer不得上main。此前已同步310c35a及其CI342PASS6ignored不是新observer验收。

人工去处/配置：由Office安装负责人核对本机EXCEL.EXE来源、版本和文件身份；可手工Get-FileHash -LiteralPath <已核实EXCEL.EXE路径> -Algorithm SHA256。明确新请求后，审查独立的新宿主身份批准记录，再显式提供-ExpectedExcelExecutable、-ExpectedExcelSha256及新的-NewRunDirectory。不要回改冻结任务书/锁或旧res-002 manifest来消除差异。

撤销：本轮工具/文档未提交，可在审查待同步清单后按文件撤回自有改动；不删除失败证据或覆盖他人修改。当前ENG005仍PARTIAL，本轮BLOCKED，C1–C4均NOT_RUN（完整条件）。未知边界：身份变化原因；未运行的COM/保存/清理；比较器单测不能证明真实宿主等价。

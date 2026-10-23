# ENG-006 原公式与稳定表身份绑定

2026-10-23，实况045，基于ad5e84d。新增reference_plan::binding，只读组合既有词法跨度与Workbook模型。bind_points显式接收公式所属SheetId；隐式本表引用绑定到该ID，显式表名按模型相同String::to_lowercase规则查找实际Worksheet身份，不使用数组位置。保留explicit_sheet区别，后续复制不能忽略隐式/显式语义。

BoundFormula私有字段、只读getter，借用原公式与原Workbook；reference_text/address_text从其自身保存的原文解释跨度，调用者不能把另一个文本传给这些方法。它不持有包帧、revision或可序列化授权，不是结构写入许可，也不证明完整语法或函数合法。A1 B2与UNKNOWN(A1)仍可形成词法绑定，测试明确保留这条边界。

复用Workbook构造时已有的唯一ID/小写名称保证；未知owner/目标、chartsheet或opaque目标、所有定义名称、词法失败明确拒绝。最多1024表且受调用方更小预算限制，表名128 UTF-8字节，公式8192字节和显式引用数预算；不是Excel完整表名规则。向量预留用try_reserve_exact；小写String分配和全局分配失败没有注入或全面无OOM保证，预算不是总堆字节上限。

7项手写独立预期全部PASS：显式/隐式/带转义表名、表重排、owner变化、未知和非Worksheet拒绝、名称/词法后项失败、预算及点计划组合不改原文/模型。全工作区363PASS/0FAIL/6ignored，Release zen-deliver PASS，106迁入身份基线PASS。DSHA Python3.12.14、Cargo/Rustc1.99.1、owned health/MSVC SDK均PASS。本轮仅格式化新增两个文件，无无关源码改动；没有新失败。

ENG005实况042有界DONE前置不变。结构写入0、原生Excel0，ENG006四个完整冻结条件仍NOT_RUN，任务PARTIAL。下一步窄公式语法，再处理范围与全部拥有者、原帧/revision核验和原子候选。标准、封存业务输入与原始迁入指纹均不改。最终提交/远端一致性、CI实际句柄及日志见本轮continuity检查点；证据research/ENG-006-REFERENCE-BINDING-2026-10-23.json。

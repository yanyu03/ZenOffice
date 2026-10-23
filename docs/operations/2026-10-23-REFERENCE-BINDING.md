# ENG-006 原公式与稳定表身份绑定

2026-10-23，实况045，基于ad5e84d。新增reference_plan::binding，只读组合既有词法跨度与Workbook模型。bind_points显式接收公式所属SheetId；隐式本表引用绑定到该ID，显式表名按模型相同String::to_lowercase规则查找实际Worksheet身份，不使用数组位置。保留explicit_sheet区别，后续复制不能忽略隐式/显式语义。

BoundFormula私有字段、只读getter，借用原公式与原Workbook；reference_text/address_text从其自身保存的原文解释跨度，调用者不能把另一个文本传给这些方法。它不持有包帧、revision或可序列化授权，不是结构写入许可，也不证明完整语法或函数合法。A1 B2与UNKNOWN(A1)仍可形成词法绑定，测试明确保留这条边界。

复用Workbook构造时已有的唯一ID/小写名称保证；未知owner/目标、chartsheet或opaque目标、所有定义名称、词法失败明确拒绝。最多1024表且受调用方更小预算限制，表名128 UTF-8字节，公式8192字节和显式引用数预算；不是Excel完整表名规则。向量预留用try_reserve_exact；小写String分配和全局分配失败没有注入或全面无OOM保证，预算不是总堆字节上限。

7项手写独立预期全部PASS：显式/隐式/带转义表名、表重排、owner变化、未知和非Worksheet拒绝、名称/词法后项失败、预算及点计划组合不改原文/模型。全工作区363PASS/0FAIL/6ignored，Release zen-deliver PASS，106迁入身份基线PASS。DSHA Python3.12.14、Cargo/Rustc1.99.1、owned health/MSVC SDK均PASS。本轮仅格式化新增两个文件，无无关源码改动；本机未见失败；后续CI首次超时另见下节。

ENG005实况042有界DONE前置不变。结构写入0、原生Excel0，ENG006四个完整冻结条件仍NOT_RUN，任务PARTIAL。下一步窄公式语法，再处理范围与全部拥有者、原帧/revision核验和原子候选。标准、封存业务输入与原始迁入指纹均不改。最终提交/远端一致性、CI实际句柄及日志见本轮continuity检查点；证据research/ENG-006-REFERENCE-BINDING-2026-10-23.json。

## CI跟进：首次超时与原样复跑

代码提交44658a227989921a58a91aa2038bdf9f4a8749d4的冻结CI39940574567首次job120370705257失败：既有test_typed_preflight的Node --assess子进程超过原10秒限额。实际日志保留；审查路径仅限本地计划读取/哈希/前置判断，没有在assess路径加载外部生成器或写文件。具体宿主调度/启动原因未证实，不能标成已经修复。未更改源码、测试、超时、预期或冻结标准；只执行一次有理由的原样job复跑，job120371320087成功，同5项用例耗时2.520秒。首次失败不抹除。核心CI与最终文档同步状态仍以continuity实核为准；文档后续提交不重复未变Rust全套。

代码44658a2核心CI39940574560/job120370705705已实际成功，日志合计363PASS/0FAIL/6ignored，Release与既有数字业务历史对照成功。无新增原生Office运行。后续提交仅补充CI实况，Rust源码与测试身份不变。

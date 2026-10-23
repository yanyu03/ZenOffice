# ENG-006 有界算术语法

2026-10-23，实况047，基于94f3c2c。reference_plan::arithmetic::validate_arithmetic对BoundFormula原文及绑定跨度检查操作数/操作符顺序；支持单点引用、十进制/科学计数、括号、一元+-与二元+-*/^。可选前导等号，拒绝空表达式、缺操作数、隐式相乘、相邻表达式和不配对括号。函数/字符串/比较/%/&/范围仍明确拒绝；没有自动扩大计算器或写入准入。

这是迭代语法检查，不生成优先级AST、不计算结果；1/0和1e999可语法通过而不表示可计算或有限结果。最多4096 token（调用方可更小）、64层括号，上游原文8192字节；原文、跨度、固定标记、稳定SheetId/显式限定及Workbook不变。完整结构计划仍需范围/全拥有者、函数准入、原帧/revision、重算和原子候选。

7项独立预期PASS，全Rust370PASS/0FAIL/6ignored、Release zen-deliver PASS，106身份基准PASS。首轮6PASS1FAIL：测试辅助函数对1.2.3的已词法拒绝结果unwrap；已改为单列词法拒绝断言，语法实现和接受范围未改，原日志保留。DSHA Python3.12.14、Cargo/Rustc1.99.1及health/MSVC SDK均PASS。

ENG006仍PARTIAL，整体C1–C4 NOT_RUN，XLSX结构写入0、原生Office0。ENG005已验收前置不变。仅新增模块/测试与可变实况，无迁入源指纹、任务书或封存业务修改。下一步范围规则及窄函数参数语法按独立计划推进，全拥有者计划前不得写入。同期CI继续检查Node启动观测和原5项10秒测试；上轮超时历史不抹除。最终同步/CI真实句柄与日志在本轮continuity。证据research/ENG-006-ARITHMETIC-2026-10-23.json。

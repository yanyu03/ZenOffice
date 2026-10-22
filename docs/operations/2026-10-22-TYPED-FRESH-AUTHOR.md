# 首份独立创作候选的实测

2026-10-22（Asia/Shanghai），按已审路线B从源码实现新的author_typed_package.py。它不导入测试夹具或artifact-tool，不读取旧导出包，不修改旧生成器门禁；使用既有自有metadata组件，按未改动的ENG-005-business-v2计划直接构造全新工作表/包。冻结任务书和原计划保持。

## 实现与验证

创作器先用独立有理数审计核对原计划的字面预期与操作，再写原始数字、inline文本、普通公式及预声明初始缓存，行列按坐标排序，日期格显式s=1。复用1900/1904、manual calcPr、builtin14及customXml构造；ZIP时间戳固定，重复创作字节确定。原始文本经过XML字符验证、转义并保留前后空格。

八项本机测试PASS：三计划案例的内存包通过独立checker；预算初始字面缓存/行顺序/日期；确定性与不修改spec；create-new和已有目标保持；非法计划/案例/缓存预期不产生输出；父目录缺失与格重叠拒绝；Unicode/空白/XML非法字符；注入fsync失败只写一次、保留失败产物且不覆盖重试。输入计划字节每项前后核对。

这些拒绝是测试前声明的隔离样本，不是把真实失败任务自动重放。作者只支持已存在的可信父目录、仅向新文件创建，不承诺原子发布、对抗目录重解析竞争、断电恢复或全盘事务。若写入/fsync失败可留下部分或完整但未确认持久性的候选，明确拒绝并保留，不能当成功结果。

## 一份实际文件

源码测试通过后，通过DSHA Python3.12.14环境门禁，在新target/office_research/ENG-005-fresh-028目录只创建budget.xlsx：3374字节，SHA256 47d5bc24b376e9f5c61f85055f59ab35b6d6fca65e53340daf0687eb57a45664。计划SHA256 ebd6c6dd390a4a1890eb2b1ec78f54d18d828ab8a1201d65676849bded820dba。

另一个只读进程使用独立check_typed_package.py核对实际文件，policy PASS、6个初始公式缓存及对应输入/日期/样式/关系符合当前窄策略，检查前后文件字节相同。其值与公式来自原计划，不从Zen或Excel结果反推。原3份失败候选哈希全部不变。

这是首份真实候选的包策略通过，不是完整OOXML schema或Excel有效性保证，更不是三业务引擎验收。其余两案例仅在内存单测中构造，没有创建真实业务文件；新候选未渲染、未视觉审阅、未封存manifest、未进入正式RCD输入目录，也未运行Zen/Excel/ONLYOFFICE。

公开身份及计数见research/ENG-005-FRESH-AUTHOR-2026-10-22.json；实际候选、作者/检查器receipt与DSHA日志只在本机target保留，二进制运行产物不入Git。

## 入口与后续

新创作入口为python tools/office_research/author_typed_package.py <plan.json> <case-id> <new-output.xlsx>；调用前仍需额度与DSHA环境门禁，禁止覆盖或自动重试。检查器为python tools/office_research/check_typed_package.py <plan.json> <case-id> <candidate.xlsx>，只读结果不等于封存许可。

下一周期在这份候选身份上先做读取型渲染/实际视觉检查及独立结构复核，检查前后哈希保持；成立后才扩展另外两业务实际候选并逐份准入。三份全部通过初始策略与视觉条件后才可封存新manifest，之后另建Zen API业务runner和原生宿主观测。失败保留，不能补修旧包或降低日期/calc/cache条件。

本轮ENG-005实况028为PARTIAL，产品C1—C4均NOT_RUN；仅创作工具与首份候选阶段通过。源码基线106及记录校准通过后同步main，CI按实际提交跟进，本机continuity记录结果。没有Rust源码变化，不重复Rust全套。

# 独立只读日期解释卡实测

日期：2026-10-22。设备与固件：本机Windows x86_64，固件未调查。工具与版本：DSHA环境检查Python3.12.14/Node24.14.1，Python标准库，bundled sharp0.35.5。样本数：3份既有候选、3张日期卡，7项独立测试方法。是否实测：是，环境门禁、读取、生成、哈希与三图查看。结论强度：仅有界日期诊断工具，不是Zen或原生Excel/ONLYOFFICE验收。

## 实现与边界

新增tools/office_research/preview_typed_date.py，先调用独立窄包策略验证初始文件，再独立解析workbookPr、关系目标、目标单元格原始v和s。日期体系取包内date1904，不能取标签猜测；体系不符原计划时拒绝。只输出新SVG及回执，不导出或改写XLSX、不计算公式、不实现通用Excel排版。

日期解释使用Decimal和标准库日历：1904序号0为1904-01-01，59.25显示1904-02-29 06:00:00。1900序号59为1900-02-28；60明确标为Excel虚构1900-02-29，61为1900-03-01，0标为calendar unspecified，不冒充原生显示。负数/非有限/超出日历范围/不足微秒精度的分数拒绝。输出ISO时间是诊断形式，原builtin14日期格式和整数/小数序号均未改变。

公开依据：[微软日期体系说明](https://support.microsoft.com/en-gb/excel/date-systems-in-excel)、[1900闰年兼容行为](https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/wrongly-assumes-1900-is-leap-year)、[1904序号0定义](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xls/4a5e900a-0eb0-4355-8fc1-81aab8f46e8b)。这些依据与本机运行结果分别保留，不算原生宿主对照。

## 实际结果

7方法PASS：1900边界、1904闰日/小数时间、两体系9999-12-31上界、坏输入/精度拒绝、3原计划日期字面预期、日期体系与标签/规格错配拒绝、create-new和源/旧输出保护。纳入既有Windows任务书CI。源码基线106身份PASS。未改Rust源码，不重复全套Rust回归。

三份固定身份候选输出并实际查看三张日期卡：预算c76852a1…为2026-10-19 00:00:00；订单71d0703c…为1904-02-29 06:00:00；考勤ea439538…为1900-02-28 00:00:00。三图日期/体系/原始序号/样式说明可读，无观察到截断。输入字节在读取和输出前后保持，计划哈希仍ebd6c6dd…；SVG由sharp仅栅格化成PNG，未生成另一份工作簿。

上轮artifact-tool的1904预览差异仍保留，不撤回其失败记录，不宣称已修复该库。独立卡只补充可核对的日期解释，不替代既有六张全表布局检查或未来Excel Text观测。本轮封存数仍0；下一周期逐项复核包/布局/独立日期证据，决定三输入身份封存，再做Zen API/原生业务执行器。ENG-005 PARTIAL，C1至C4 NOT_RUN。

## 复现、回退与可推翻点

专项：python tools/office_research/test_preview_typed_date.py --log <新日志>。候选：python tools/office_research/preview_typed_date.py docs/engine_specs/ENG-005-business-v2.json orders <固定输入> <新SVG>，通过dsha.py envfeel --require python --run python --执行；回执需自行保存且不覆盖。三样本实际路径/回执/栅格化脚本保存在target/office_research/ENG-005-date-preview-031；[证据索引](../research/ENG-005-DATE-INTERPRETATION-2026-10-22.json)记录完整输入/源码/日志/图像身份。

回退只撤销本轮工具、测试、CI与可变文档变更，保留原计划、任务书、候选和前轮失败证据。可能被推翻的三点：日期边界尚未在本机Excel运行；只支持已准入的初始日期单元格和微秒内分数；诊断卡不是原始格式排版或本机区域Text。正式业务验收仍须实测。

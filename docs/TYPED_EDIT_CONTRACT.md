# ENG-005类型与公式编辑实施契约

日期：2026-10-19。性质：可变实现设计，落实ZEN-ENGINE-v1.0的ENG-005-C1，不取代或修改冻结验收标准。首份设计依据28f2db7（E1实测08bc566）；类型首切片实测a83fe4d，普通公式替换切片实测d991431。C2/C3/C4整体仍未验收。日期序号新切片见末节；实测4f82fd7：本机6项、Windows CI总300项通过，6忽略；新日期宿主仍未运行。

## 数据模型和调用意图

复用zen_engine::workbook::TypedValue::{Number,Text,Boolean,Error}及DateSystem::{Excel1900,Excel1904}。空白使用Option<TypedValue>::None，不新增与现有模型冲突的Blank读取变体；空字符串是Some(Text(""))，不等同于None、0或false。原数值lexical用于保真；新数字应按value生成合法词法，不信任调用者提供的矛盾lexical。新请求不能直接修改共享Workbook，也不能暴露候选ZIP的无验证提交接口。

已实现WorkbookSession::set_value、受限set_formula及clear_formula；已新增set_date_serial(sheet,address,serial,system)，限定既有显式日期样式；固定4f82fd7的本机/Windows CI合成测试通过，新宿主对照未运行。公式替换/清除限已有格及受支持的可选链失效规则，详见末节；下述各切片按提交保留历史，当前能力以准入表和最后一节为准。SetValue(Text("=A1"))是字面文本，永不自动转为公式；SetFormula才进入公式解析。读取模型继续把带日期格式的n单元格投影为Number，日期编辑意图必须另外携带体系，不能由样式索引猜测已经完成日期语义。

| 意图 | 表示与区别 | 当前运行能力 | ENG-005目标准入及写回规则 |
| --- | --- | --- | --- |
| 清空 | None，移除输入和公式，保留单元格位置/样式 | set_value清字面输入；clear_formula清普通公式及缓存，受支持链可失效；未知链/合并/错误格拒绝 | 已存在显式c节点清空内容；保留r/s，移除f/v/is及过期类型；空白已空且无公式时不改变字节/redo |
| 文本 | Some(Text)，含空字符串及以=开头文本 | 已有输入格可写，Rust合成样本通过；尚无新宿主对照 | 新文本使用inlineStr/is/t并做XML转义及必要空白保持，不重排sharedStrings；空字符串仍是文本 |
| 布尔 | Some(Boolean)，不是数字0/1或文本TRUE | 已有输入格可写，Rust合成样本通过；尚无新宿主对照 | 输入和缓存分别写合法b/0或1；计算准入不得通过把布尔伪装成数字来通过 |
| 数字 | Some(Number)，仅有限f64 | set_value可切换已有输入类型；set_number保持E1入口限制 | 两入口复用候选提交；NaN/无穷拒绝；相同数字保持原词法和字节 |
| 日期1900 | 显式serial+Excel1900 | set_date_serial已有有界实现；本机合成通过，新宿主待办 | 体系必须与目标Workbook一致，拒绝隐式转换；先核对/解析目标日期格式，再写序列；边界及序列60单列宿主样本 |
| 日期1904 | 显式serial+Excel1904 | set_date_serial已有有界实现；本机合成通过，1904业务宿主未验收 | 同上；体系不符拒绝，不因当前机器区域或默认体系重写workbookPr；1904样本必须独立冻结 |
| 普通公式 | SetFormula，区别于输入/缓存/显示 | 已有格可替换普通公式并失效受支持可选链；合成Rust样本通过，新宿主对照待办 | 复用WorkbookCalculator解析/名称解析/循环检测，替换后重建动态依赖并重算；输入允许一个前导=，保存f不含前导=；仅严格相同公式意图可判无变化，不自行改内部空白 |
| 错误值 | Some(Error)与计算产生Error分开 | 未变错误缓存可保持；直接写错误和变化错误缓存仍拒绝 | 第一切片直接错误输入明确拒绝；在类型化缓存写回及宿主对照通过前，不放宽错误变化/类型转换门禁 |

表中“目标规则”是实施要求，不是已通过用例。第一实现切片限已存在显式c节点；缺失节点、合并覆盖格及未知元数据先拒绝，不通过索引插入顺序等隐含假设写坏行结构。单元格内容清空不删除行/列或改变工作表结构。该限制及后续范围在能力报告中列出，不隐藏为成功。

## 单一事务路径

1. 请求语法/数值/XML字符和预算检查；按当前原包查sheet/地址/目标元数据。清空、替换公式、替换类型是不同意图，不能沿用“target must already be numeric input”作为全部新类型规则。
2. 从当前有效帧建立候选计算器，SetValue复用set_input（含移除原公式），SetFormula复用set_formula；已知可解析静态名称按既有有界解析保留，未知名称/数组/共享/data-table/外链/动态引用拒绝。公式准入由解析器和依赖图决定，不靠字符串扫描，也不能把文本中的方括号误判成外链。
3. 在临时候选内检查跨表引用和循环并重算。先产出全局编辑计划，再改包；不得先提交目标格再发现依赖缓存无法写回。未知包依赖沿用当前保包准入，抽出共用检查，不复制数字写回器的整段保护逻辑。
4. 每个变化缓存按实际CalculationValue类型处理：Number用n/v，Boolean用b/v，Text公式缓存用str/v，Error用e/v。既有未变结果保留字节。空结果与空文本的表示须独立实测；未支持的结果类型在原包修改之前拒绝。共享字符串索引不能当作字面文本缓存。
5. 只补丁写目标单元格及实际变化缓存，保留合格r/s和命名空间限定名，已知边界外属性/扩展拒绝；保持关系、样式、未知部件和原源。不能再出现负向测试以无命名空间f替代x:f而失去公式的构造错误。
6. 候选按原limits重读，核对类型/公式/缓存/日期体系和预算，再复用zen-session::commit_candidate提交一帧。失败保持frames/cursor/revision/redo/dirty/save_point及原文件/旧输出；从撤销点分支只在成功后清redo。临时解析/候选和脱离历史的保存点仍计入既有预算。

持久化继续复用已验收save_as_new，Windows可信本地父目录、create-new、来源复查及锁定/重解析拒绝不因E2绕过。日期/格式意图不能触发覆盖保存、隐式工作簿日期体系迁移或用户Office控制。

## 日期和错误专门边界

微软确认Excel有1900/1904两种日期体系；同一日期跨体系的序列值有差异，不能把序列当作不带体系的绝对日期。[日期体系说明](https://support.microsoft.com/zh-cn/excel/date-systems-in-excel)。1900兼容闰年行为需保留专门用例，不能交给普通公历库后静默“修正”。[1900兼容问题](https://learn.microsoft.com/en-us/office/troubleshoot/excel/wrongly-assumes-1900-is-leap-year)。首轮边界包含1900序列59/60/61、1904序列0/1、半日小数、体系不符和非有限值；先写独立预期再看结果。

目前没有样式格式解析/日期显示验收，SetDateSerial仍是待实现能力。首切片不接收无体系的ISO日期字符串，也不把t=d导入支持或显示语义作为现有事实。日期格式创建/选择必须显式受控；未知格式与未测范围单列，不借“保留样式”冒充支持新日期编辑。

错误写入、公式新生成错误、原错误保持、错误转数字、数字转错误分别列用例。Error字符串和值类型同时比较，不能把COM错误HRESULT当成普通整数写入；未知错误标签先拒绝。旧E1针对相同错误缓存的保留不授权直接写错或修改错误类型。

OOXML的b/n/e/s/str/inlineStr语义与读取模型及意图映射要分开，不能把s索引、str公式缓存和inlineStr输入混为一类。[微软CellValues说明](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.cellvalues?view=openxml-3.0.1)。这些是公开格式/接口依据，不是Office内部实现还原。

## 实现顺序和验收材料

第一代码切片：抽取共同包准入/候选计算及写回计划，接入已存在单元格的清空、文字和布尔输入，以及变化数字/布尔/文本缓存；接入同一会话的undo/redo和失败预算。保留set_number行为和原业务CI，尚不宣布日期或公式替换可用。

第二切片：普通公式替换/清除与跨表动态依赖，验证旧依赖解除、新依赖重算和循环/未知名称拒绝；补Error缓存类型转换的显式政策和独立负向用例。第三切片：明确样式/日期序列写入、两个体系和边界；三个业务新版本分别覆盖类型切换、清空、公式替换，Zen和Excel同等操作保存重开及未知部件保持。

新类型样本另建版本，不改RCD/engine-acceptance/res-002-v1。每份在运行前冻结输入SHA256、请求序列、逐步TypedValue/公式/缓存/格式/日期预期及数值容差，包含字面=文本、空白与空字符串、false与0、拒绝分支/保存失败、两步撤销/重做和分支预算。原9次纯数字Excel观测只作旧范围回归，不能充当类型/公式新宿主结果。

首切片a83fe4d的set_value实现已有非公式/非错误输入格的清空、文本、布尔、有限数字及支持的变化文本/布尔/数字缓存；set_number保持旧入口限制。原包准入共用，源限定名/局部命名空间/r/s和原公式片段保持；ST_Xstring字面转义及CR不丢失；相同值保持字节和redo。公式目标含清除、日期、直接/变化错误、空公式结果缓存、缺失/合并格仍拒绝。n=6新测试函数、1份主要手写双表类型夹具及既有小算术夹具，不是6份业务文件。见docs/research/ENG-005-TYPED-2026-10-19.json和CI 39179693476。

C1设计范围完成；C2/C3/C4要求完整类型/公式、三份业务与宿主/回归证据，未执行不得标PASS。复现本设计的代码核查位置：zen-engine/src/workbook.rs、recalc.rs；zen-formats/src/spreadsheet/{cells.rs,edit.rs}；zen-session/src/{lib.rs,persistence.rs}及08bc566的保留/拒绝测试。回滚该可变设计按提交撤回，保留实况；不修改冻结任务书。

首切片图表边界：数值图表缓存仍按原路径维护；存在strRef或multiLvlStrRef的工作簿拒绝新的set_value请求，避免文字标签缓存陈旧。原set_number范围不变。该拒绝用既有多场景文件实际回归，不宣称已支持文字图表缓存刷新。

## 普通公式首切片（2026-10-19）

实测d9914312ec3f1ded2d18a97a3768a4c0662efb41 / CI 39190167786：WorkbookSession::set_formula已实现已有格的普通公式替换/从输入变公式，允许一个首字符=，不改写内部空白；数字/文本/布尔结果可写。复用临时计算器解析/名称解析/拓扑检查和全量依赖重建；随后的输入编辑按新的公式求值。相同公式保留字节/redo。公式XML文本和ST_Xstring缓存分别编码，限定名和r/s保留。

新请求有calcChain时拒绝；不删除链，不宣称链维护完成。原calcPr逐字节保留，手写manual/fullCalcOnLoad=0/forceFullCalc=0有独立不变预期，不推断Excel执行行为。变化错误/空结果、公式ca/ref等元数据、数组/外链/未知名/循环、缺失或合并格仍拒绝；公式清除和日期API仍未实现。4项新增合成测试、1份主要双表夹具及其变体通过；三份原数字业务仅回归，新的类型宿主运行0次。完整证据见docs/research/ENG-005-FORMULA-2026-10-19.json，ENG-005 C2/C3/C4整体仍未通过。

## 公式清除切片（2026-10-19）

实测053811dd7806de098cd714d616a5b3fdf53da7dc / CI 39191763684：WorkbookSession::clear_formula移除普通f及缓存为None，保留c/r/s，不把旧计算结果固化为输入。此意图与set_value(None)清输入分开：非空字面输入拒绝，已有空白返回Unchanged并保持字节/redo。候选用set_input(None)解除公式和依赖后全量重算；支持的下游算术重新写缓存。原calcPr及无关部件保持，undo/redo恢复完整归档。

仍拒绝calcChain、未知公式元数据/数组、原错误缓存及不支持的空下游缓存；不删除/重建链，不主张日期或宿主兼容完成。n=4新增测试函数，1份手写双表夹具及其变体，安全保存/重开、旧输出/原源和预算/失败状态保持均在最终CI验证。三份数字业务只是旧范围回归，新Office运行0次。完整证据：docs/research/ENG-005-CLEAR-2026-10-19.json；ENG-005仍PARTIAL。

## 可选calcChain失效切片（2026-10-19）

实测d81da828fe93493e36c2f22e6432d83580f4d060 / CI 39196374265。根据[微软格式说明](https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-the-calculation-chain)，链是既有计算顺序元数据，并非依赖树，使用链不是必须条件。本引擎采用失效而非重建：仅接纳/xl/workbook.xml拥有的单一/xl/calcChain.xml标准部件、标准关系与显式类型Override；唯一incoming、无outgoing/自有关系部件，根无扩展，c仅r/i且引用已投影的不重复普通公式。s等其他有效但尚未验证的链标记也拒绝，不宣称通用链兼容。

普通公式结构变更后，候选一起移除链、其关系和类型Override；原calcPr逐字节保留，支持的值与缓存全量重算。不请求宿主强制重算，不声称还原或生成Excel内部链。文字输入改动保留原链；无变化请求保持归档/redo。未知扩展、节点文字、复数归属/引用及预算/保存失败都不提交。

n=4新Rust测试函数，1份双表链夹具及7种负向变体；验证替换/清除、无悬挂关系、无关部件保持、manual calcPr不变、预算/旧输出/原源、undo还原原链、redo移除和保存重开。3份旧数字业务仅回归，新增Office运行0次；原生宿主链重建、日期和完整业务对照仍待验收。证据docs/research/ENG-005-CHAIN-2026-10-19.json，ENG-005仍PARTIAL。


## 显式日期序号切片（2026-10-19）

预期先记于engine_specs/ENG-005-date-serial-v1.json。格式层validate_date_serial核对显式体系、有限非负范围与既有日期样式；write_date_serial_edit复用已验收类型候选写回；会话set_date_serial先校验再判无变化，其余复用重算、原子候选、预算、历史与安全保存。读取仍投影Number，不新增与现有模型冲突的日期值变体；CLI仍为数字命令，本轮未增加日期CLI。

1900准入[0,2958466)，1904准入[0,2957004)，含末日小数；范围作为本切片明示策略。序号60原样作为兼容数据保持，不解释成真实公历日期。无时区、区域字符串解析或体系转换；不自动分配、改变样式或workbookPr。算术依赖仍按数字重算，计算结果不会自动获得日期意图或受输入日期界限约束。

样式准入：唯一标准styles媒体类型/拥有关系、显式单元格s、唯一cellXfs、选中xf的applyNumberFormat明确true，xfId限无/0，属性限numFmtId/fontId/fillId/borderId/xfId/applyNumberFormat且无子节点。builtin仅14/15/16/17/22，自定义只接受精确yyyy-mm-dd或yyyy-mm-dd hh:mm:ss、ID>=164且不重复；拒绝builtin覆盖、未知格式、未知根/扩展/命名空间及目标表条件格式/tableParts。此为保守准入，不是完整样式解析或渲染。字体、边框等其他样式语义未新增，原部件字节保持。

新的样式与目标表元数据读取另限2MiB/16384节点；超出拒绝，不扩张会话原资源预算。无变化也必须通过日期体系/样式准入，之后保持原词法、ZIP、revision与redo；修改仍受完整保包门禁、合并目标拒绝和候选重读约束。原数值/通用类型入口保持原意图边界，不从数字猜日期。

实现依据：[微软numFmt列表](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.numberingformat)、[xf/applyNumberFormat](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.cellformat)与research/DEVELOPMENT-REFERENCES.md固定ONLYOFFICE源码线索；来源不能替代本机Excel同操作观测。一个手写家族的两个体系变体及拒绝变体，已本机6项回归通过；新日期Excel和ONLYOFFICE运行0次。ENG-005整体仍PARTIAL。

固定提交4f82fd711ea703227f4926fcd54f6659715e3e06，[CI39206789714](https://github.com/yanyu03/ZenOffice/actions/runs/39206789714)实际300通过/0失败/6忽略，37测试二进制，六个日期回归函数逐名通过；证据见research/ENG-005-DATE-2026-10-19.json。样本实际为一个手写微型家族的两个体系/样式/命名空间变体，不把它们计作两份独立业务样本。

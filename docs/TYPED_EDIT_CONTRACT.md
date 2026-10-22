# ENG-005类型与公式编辑实施契约

日期：2026-10-23。性质：可变实现设计，落实冻结ZEN-ENGINE-v1.0的ENG-005-C1；不替代冻结标准。当前源码复核基线c5c6dbce62d7e020a91c7d943b2c086e087429fe。历史切片及当时未实现范围见Git历史与各自research凭据；本表描述当前接口，不把设计当作宿主运行结果。

## 模型与八项准入

复用zen_engine::workbook::TypedValue::{Number,Text,Boolean,Error}和DateSystem::{Excel1900,Excel1904}。空白为Option<TypedValue>::None；日期不是新的TypedValue变体；公式定义、缓存、显示值分开。

| 意图 | 调用与模型 | 当前准入/写回 | 明确拒绝或限制 |
| --- | --- | --- | --- |
| 空白 | set_value(None)清字面输入；clear_formula清普通公式 | 已有显式c；保留位置/样式，移除内容/过期类型；已有空白为不改字节的no-op | set_value不能清公式；clear_formula不清非空字面输入；错误目标、缺失/合并格、未支持空下游缓存拒绝 |
| 文本 | set_value(Some(Text))，含空文本、以=开头文本 | inlineStr/is/t，XML及ST_Xstring字面转义，保留必要空白；不解析成公式 | 非法XML字符、读取/历史预算、未知依赖或不支持缓存拒绝；不重排sharedStrings |
| 布尔 | set_value(Some(Boolean)) | b/v=0或1；false与Number(0)、Text("FALSE")分开 | 不以强制转数字绕过类型门禁；变化缓存必须可写 |
| 数字 | set_value(Some(Number{value,lexical}))；旧set_number | 有限f64；新词法由value生成，不信任矛盾lexical；相同数值no-op保留原词法/字节 | NaN/正负无穷、公式/错误目标及其他保包门禁拒绝；旧数字入口不成为通用类型入口 |
| 日期1900 | set_date_serial(...,serial,Excel1900)，读取仍Number | 匹配Workbook体系，范围[0,2958466)；序号60保留为兼容数据；既有显式支持日期样式 | 不转换日历/时区/区域字符串，不把序号60归一为真实公历日期；体系不符即使数值相同也拒绝 |
| 日期1904 | set_date_serial(...,serial,Excel1904)，读取仍Number | 匹配Workbook体系，范围[0,2957004)，含末日小数 | 不按本机默认体系转换，不修改workbookPr/格式；负值、非有限、越界拒绝 |
| 普通公式 | set_formula；clear_formula独立意图 | 已有格创建/替换普通公式；可去一个首字符=，保留内部空白；支持数字/文本/布尔结果；重建跨表依赖及缓存 | 未知名称/函数、数组/共享/data-table、外链/动态引用、循环、未知公式属性、新错误/空结果拒绝；已支持可选calcChain仅失效，不重建 |
| 错误值 | Error(String)读取模型；直接写入拒绝 | 读取t=e原始标签；计算后与原缓存同类型且同标签的既有错误可保持原字节 | 所有直接Error输入（含相同或未知标签）、字面错误目标清除/替换，以及变化错误或错误与非错误缓存转换均拒绝；读取未知标签不等于支持其计算语义 |

空白None、Text("")、Boolean(false)、Number(0)四者不同；文本"=A1"不成为公式。共享字符串索引s、公式文本缓存str和字面inlineStr不能互换。上述写入限既有显式c节点，不删除行列；不支持的元数据和未知依赖拒绝。普通输入/公式相等可提前返回Unchanged：它仅表示没有产生新帧，不是包可编辑证明。日期意图先检查体系/样式再判no-op。

## 日期样式与错误值专门边界

日期样式要求唯一标准styles部件/拥有关系、显式单元格s、唯一cellXfs，选中xf的applyNumberFormat明确true，xfId限无/0，属性限numFmtId/fontId/fillId/borderId/xfId/applyNumberFormat且无子节点。builtin仅14/15/16/17/22；自定义仅精确yyyy-mm-dd或yyyy-mm-dd hh:mm:ss、ID>=164且不重复。拒绝builtin覆盖、未知根/扩展/命名空间、目标表条件格式/tableParts、time-only/任意自定义格式。额外元数据上限2MiB/16384节点。该范围是保守编辑准入，不是日期显示或完整样式引擎；普通数值/公式结果不会自动获得日期意图。

错误分六类：直接输入、原错误标签读取、未变化计算错误保持、变化错误、错误转非错误、非错误转错误。前三者分别是拒绝、读取、限定保持；后三者写回拒绝。未知标签可读作原始Error字符串，不能据此声称标签有效或算法兼容。COM错误HRESULT不能当作普通数值写入。错误/空结果门禁在候选提交前执行，不把失败降为成功或改为数值。

## 事务与保存契约

1. 意图/数值/文字预算检查；按原包定位工作表、地址及显式目标。
2. 复用保包门禁和临时候选计算器，解析支持的公式、静态名称及依赖图；不靠文本扫描替代公式解析。
3. 候选重算并核对全部变化缓存。支持的目标/依赖Number、Text、Boolean按n、str、b写回；未变化缓存保留原字节，unsupported错误/空缓存先拒绝。
4. 补丁仅作用于合格目标、变化缓存及支持的可选链失效。保留源限定名、r/s、calcPr、关系、样式及无关部件。calcChain只接受工作簿唯一拥有的标准位置/关系/显式类型、无outgoing，c仅r/i并指向不重复已投影普通公式；结构公式编辑一起移除链/关系/Override，未知扩展拒绝。
5. 候选用原limits重读，历史/保存点/字节预算和revision检查通过后提交一个不可变帧；失败保持当前帧、模型、诊断、历史、redo、revision、dirty及源/旧输出。成功分支才清redo。
6. Windows可信本地父目录/create-new保存；源身份、锁和重解析检查不绕过，不承诺不可信目录命名空间CAS。显式受控自有独立叶子XML可按不可变OpaqueXmlPolicy贯穿候选/历史/发布前实际临时字节核验；普通未知customXML路径仍拒绝。语义独立是调用方前提，不由命名空间或保留字节自动证明。
7. save_as_new_with_receipt仅在发布成功后返回内存凭据，绑定路径、完整SHA/长度、原policy及limits/预算；reopen建立干净新历史，可继续编辑和再保存，不重算初始源SHA。修改、移动或锁定输出拒绝。凭据不能序列化/导入，跨进程恢复及CLI类型/日期入口尚未实现。

## 核查与验收层次

源码：crates/zen-engine/src/workbook.rs、recalc.rs；crates/zen-formats/src/spreadsheet/{cells,dates,edit,frame}.rs；crates/zen-session/src/{lib,persistence}.rs。对应八行可变机器表在engine_specs/ENG-005-admission.json。

本轮预先约定engine_specs/ENG-005-admission-review-v1.json；新增3项错误方法使用一个独立手写微型家族及其缓存/标签变体，不是3份业务文件。当前定向46方法PASS：workbook_session28、date_serial6、opaque_policy_session9、typed_business_admission3。其中三封存业务与字面预期、保存帧续编辑重跑通过；具体日志和哈希见research/ENG-005-ADMISSION-REVIEW-2026-10-23.json。本机Windows、Python3.12.14/Cargo及Rustc1.99.1，MSVC SDK子环境自检通过。未在本地重复无关全套/Release，提交CI结局另记continuity。

撤回：本文件旧版“SetDateSerial仍是待实现能力”及“目前没有样式格式解析”作为当前现况的描述（2026-10-19设计段）。实际4f82fd7已实现有界日期样式/序号准入；它仍没有日期显示/完整样式验收。旧切片当时的记录保留在Git及research，不重写冻结任务书或原失败实况。

C1的模型/八项准入表复核已完成；这只关闭设计条件，不等于整个ENG-005完成。C3原生Excel三业务同操作对照尚未执行，本轮新Office运行0。既有9次纯数字宿主观察和CI中的历史比对不是新类型/日期宿主结果。

最可能推翻本结论的三点：有限表格不能穷尽未知包依赖；读取未知错误标签不能证明计算语义；合成及Zen业务通过不能推出原生日期显示/格式等价。以上分别按严格门禁、原始标签保持和未测宿主边界处理，不削弱原验收条件。

复现：通过DSHA envfeel声明python/cargo后运行cargo test --locked --offline -p zen-session --test workbook_session、date_serial、opaque_policy_session、typed_business_admission（每个目标单独--test）。运行命令经本项目quota_guard，原输入清单验证保持；具体执行日志在本轮证据中。回滚：仅按本轮提交revert测试/可变设计更新，先复查后续提交；不回改封存输入、冻结任务书或历史结果。

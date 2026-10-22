# XLSX显式不透明custom XML保留契约（实现设计v0.1）

日期2026-10-23，基于实测失败47d30ee。原034轮完成设计及3份只读结构审查。035轮已实现独立只读OpaqueXmlDeclaration/validate_opaque_xml_descriptor/回执（15新增测试PASS）；036轮实现不可变OpaqueXmlPolicy、初始/候选检查区分及元数据未知属性拒绝（9新方法PASS），037轮已显式接入XLSX候选/Session历史/保存（原三业务八步PASS），默认入口仍严格；默认unknown custom XML dependency拒绝保持。会话/历史/create-new保存已实现；保存内容的普通只读重开已验证，保留policy继续编辑的保存帧重开凭据仍待实现。原生三业务类型对照尚未运行，完整设计目标并未全部完成。落实ENG-005的包保持/明确拒绝要求，不替代ZEN-ENGINE-v1.0、不改变业务v2输入和原预期。下文保留完整设计目标，描述核验不代表全部目标已实现；当前范围见operations/2026-10-23-OPAQUE-XML-VALIDATOR.md。

## 已知问题与证据

现有zen-opc::preserve::validate_text_edit_package被Sheets写入口复用；customXml/*.xml只接纳已知bibliography Sources和受限datastoreItem族。封存3输入在首个文本写入均实际拒绝；负向3回归应继续保留，不能改为默认成功。

本轮只读核对三包（各8部件）：每包仅customXml/zen-probe.xml一个191/191/195字节叶子XML，application/xml，xl/workbook.xml以单一标准customXml关系拥有，无出向关系；按声明列表未观察到已知绑定标记。原包/清单哈希不变。namespace和payload文字均不作为可信白名单。详见[审查记录](research/ENG-005-CUSTOMXML-CONTRACT-2026-10-23.json)。

微软资料显示[XML Map可关联XML元素与工作表单元格](https://support.microsoft.com/en-us/excel/map-xml-elements-to-cells-in-an-xml-map)，[datastoreItem可携带自定义部件ID和schema引用](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.customxmldataproperties.datastoreitem?view=openxml-3.0.1)。这些资料支持需要依赖审查，不证明当前探针或任意XML存在/不存在该语义。缺少已知标记、单一入向关系、哈希一致都不能单独证明无应用层依赖。

## 选定边界：显式、受控来源、不透明保留

保留默认严格入口。新增XLSX专用、显式带policy的会话构造/写回路径，供已审核的受控自有输入；不改变Word/Slides行为，不提供环境变量、全局开关、自动发现后放行或测试命名空间白名单。

拟议OpaqueCustomXmlGrant包含：首次输入SHA256、规范绝对OPC part名、预期payload SHA256/字节数/媒体类型、预期拥有者及关系类型/身份、关系拓扑摘要、调用方对其自有数据语义独立的明确声明及可追溯依据。声明是调用方承担的信任前提，不能由引擎从XML内容推断或当作数学证明。未知来源文件、未声明部件或无法判断语义的输入继续拒绝。本轮由自制样本作者/主助手可审查自己的探针，未来调用方不得把任意用户文件自动归类为自有独立部件。

首次构造必须将grant绑定实际输入；只有对应的custom XML拒绝分支可考虑被显式policy满足，其余签名/宏/嵌入/关系/标签/保护/公式/预算准入照常执行。不得先删custom XML再调用原validator，也不得在遇到错误后自动回退为宽松重试。

## 运行不变量与拒绝条件

1. grant字段/集合完整、不可变，拒绝重复、别名或未知字段；部件名按现有OPC规范化规则处理，拒绝路径穿越/大小写歧义/percent别名/重复ZIP entry。输入和XML受既有限额控制，不扩大资源预算。
2. payload身份和媒体类型精确符合声明；XML须可解析，拒绝DTD/entity和超限，不能读取外部资源。首切片只接受叶子文本XML（无子元素/混合内容），但此形状限制本身不授予信任；不硬编码zen-probe、namespace、case或payload文本。
3. 单一标准workbook拥有关系、内部Target；目标精确映射同一部件，无其它入向或出向关系、customXmlProps/itemProps/schema/数据存储引用。关系类型许可不等于payload许可；不因customXml字样在关系白名单就跳过依赖验证。
4. 已知XML映射、dataBinding、singleXmlCells、xmlCellPr、connections等绑定/依赖途径单独分诊；未被当前解析器或包门禁理解的扩展/关系/依赖拒绝。扫描词表不宣称覆盖任意自定义协议，调用方受控来源声明仍为额外前提。
5. policy只能授权原样保留该自有部件，不能授权编辑其文字/属性/关系/类型。改变工作表内容不会触碰payload；每个候选提交前重核payload/拥有关系/媒体类型，不满足即拒绝整个事务。
6. 首次输入SHA只绑定会话创建。后续有效帧会因正常编辑改变整包哈希，不能每步要求整包等于初始SHA；不透明部件及其准入图不变量则逐帧保持。不可把新整包哈希回写grant以追随任意变化。
7. 候选重读、undo/redo、分支、保存重开必须携带相同policy身份；不允许restore/reopen丢失policy或把其传播到不同来源。普通无policy重开仍只读可解析，后续写入保持严格拒绝。
8. 失败保持当前帧、history、revision、dirty、savepoint、retained bytes和旧输出。保存仍仅可信Windows父目录/create-new，原源检查及锁定/重解析保护不变。policy描述/解析内存须纳入明确预算，不以历史字节计数冒充全部堆内存。
9. 保存后的包再验payload字节/关系/类型；源、旧输出、grant和三输入封存身份不变。未知依赖、外链、宏等不能借policy获得编辑许可。

## 预先声明的实现验收

| 独立组 | 必须观察的结果 |
| --- | --- |
| 默认严格3原业务 | 现有3项unknown customXML拒绝回归继续通过 |
| 显式grant正向 | 三原封存文件各原8操作完整执行；所有独立预期、类型、公式/缓存、体系样式、原包保持、逐步undo/redo和保存重开分别留证；不新增或替换样本 |
| 身份负向 | 错输入/部件/媒体/payload hash/字节数、重复grant、漏grant、第二个未知部件均拒绝且不改状态 |
| 拓扑负向 | 外部Target、错误/多个拥有者、出向关系、itemProps/schema依赖、别名/重复entry等分别拒绝 |
| 绑定及既有门禁 | XML map/dataBinding、未知扩展、签名/宏/嵌入、未知名/数组/外链等不能被policy放行 |
| 历史与持久化 | grant不丢失不扩大；候选解析/预算/写入/锁/旧输出失败保持状态，源变化及重开不同来源拒绝；每帧原样保留 |
| 原生宿主 | 后续3文件各3次Excel同操作仍必需；显式grant不是Office行为证据 |

该表是满足原冻结条件的实现测试分解，不把原验收降低为负向通过或policy构造成功。完整ENG-005仍PARTIAL，C2上轮FAIL不回改。本轮未执行引擎编辑/原生宿主，业务8步仍未走通。

## 下一实现与回退

先实现受限grant数据模型和独立准入验证（不接入写路径），以通用自有叶子XML和拓扑/绑定故障夹具检验，证明默认拒绝与既有保护不变；再将policy显式贯穿Sheets事务/重读/历史/保存路径，最后恢复target中原失败业务执行器在同一封存输入重跑。不自动应用旧执行器补丁，不直接把它接回默认测试。

回退只撤销该新增实现设计/审查记录，保留任务书、封存输入、原失败/负向回归及CI结果。尚可能推翻的三点：具体调用方声明不足以满足所需支持级别；未知应用层依赖超出当前解析；grant与历史/持久化集成可能需要更严格资源边界。实现遇到这些证据时继续明确拒绝，而不是重写标准。

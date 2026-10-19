# Zen开发依据：ONLYOFFICE与本机Office

日期：2026-10-19。设备：本机Windows x64；不涉及固件。工具：PowerShell、GitHub REST/raw、Python 3.12 hashlib；本轮没有启动REA/Ghidra或Office。既有Office证据由REA 6.4.0、Ghidra 12.1.4及独立Excel COM观测产生。

样本：ONLYOFFICE两个固定提交的9份源码/许可证文件；复核首轮Office证据清单44个文件哈希；复用既有3份工作簿×3次Excel运行。本轮新增宿主运行0次、ONLYOFFICE运行0次、Rust运行0次。结论强度：源码局部静态阅读、既有证据完整性复核与设计建议，不是新功能验收。

## 三条依据链

| 来源 | 能支持什么 | 不能据此声称什么 |
| --- | --- | --- |
| ONLYOFFICE固定源码 | 看可读实现怎样划分日期、公式依赖、事务历史和OOXML字段 | 这段源码已在本机运行、结果等同Excel、可直接移植进Zen |
| Office LTSC静态/函数证据 | 确认指定版本的对象声明及少量函数边界 | 掌握Excel计算或Word排版算法 |
| Office LTSC实际文件对照 | 指定输入、版本、步骤下的值/公式/缓存/错误/日期结果 | 覆盖新类型编辑、所有日期边界、Word/PPT或视觉兼容 |

统一索引：[机器可读来源清单](REFERENCE-SOURCES-2026-10-19.json)。包含固定提交、逐文件SHA256、Git blob身份和永久链接。源码缓存仅在target/reference_sources；公开仓库保存自行撰写的分析和指向上游的引用，不导入上游实现或UI资源。

## ONLYOFFICE首批实际阅读

sdkjs固定于`9d2138866d929be32f1d107e8b5bfae36156e097`，core固定于`ed63e94de814ff1d4ea28cddfaba6939bc9df9ed`。这是本次观测的master提交，不是已安装发行版身份。两个递归树均未截断；下载的9份文件均与树中的Git blob SHA1一致，并单独计算SHA256。

| 读到的具体位置 | 静态事实 | 对Zen的独立决策 |
| --- | --- | --- |
| [sdkjs日期函数357–381、825–843行](https://github.com/ONLYOFFICE/sdkjs/blob/9d2138866d929be32f1d107e8b5bfae36156e097/cell/model/FormulaObjects/dateandtimeFunctions.js#L357) | 可见1900/1904分支及序号60/1900-02-29的特殊处理；不同函数路径处理不同 | 日期必须带工作簿日期体系；序号与显示/日历意图分开；不能直接交给操作系统日期归一化。此处只发现分支，未证明其全部行为 |
| [core WorkbookPr.cpp 80、194行](https://github.com/ONLYOFFICE/core/blob/ed63e94de814ff1d4ea28cddfaba6939bc9df9ed/OOXML/XlsxFormat/Workbook/WorkbookPr.cpp#L80) | workbookPr的date1904有读写字段 | Zen读取并保留日期体系；单格日期编辑不能顺带转换整个工作簿 |
| [sdkjs History.js 1140–1179、1474–1525行](https://github.com/ONLYOFFICE/sdkjs/blob/9d2138866d929be32f1d107e8b5bfae36156e097/cell/model/History.js#L1474) | 历史点、保存索引、截断后续点、事务计数及重算锁互有关联 | 保持Zen已有不可变候选帧；验证和重算成功后一次提交历史。未来批量编辑需一条逻辑事务，拒绝不截断redo。不能照搬其UI/协作状态 |
| [sdkjs parserFormula.js 11598–11636行](https://github.com/ONLYOFFICE/sdkjs/blob/9d2138866d929be32f1d107e8b5bfae36156e097/cell/model/FormulaObjects/parserFormula.js#L11598) | renameSheetCopy路径在变更表达式栈前移除依赖，之后重建依赖 | E3结构操作统一建立引用变更计划；旧依赖不能留在新公式图中。当前Zen仍用候选图全量重建，不因此承诺增量计算 |
| [core CalcChain.cpp 70–80行](https://github.com/ONLYOFFICE/core/blob/ed63e94de814ff1d4ea28cddfaba6939bc9df9ed/OOXML/XlsxFormat/CalcChain/CalcChain.cpp#L70) | 可见a/i/l/r/s/t属性读取代码；其中s/t映射到m_oRef，需另查上下文与运行结果 | 上游代码也只能作线索；不据此放开Zen当前r/i以外的链编辑。需要独立格式预期和宿主样本后再扩展 |

另下载common/NumFormat.js和cell/model/Workbook.js供后续定位；本轮没有完成这两份大文件的语义调查，不以下载数量计功能覆盖。Word和Slides源码尚未阅读，后续跟随RES-003/004的问题选定文件和版本。

许可证身份来自两个固定提交的LICENSE与文件头：AGPL v3及上游附加条款，不能只记录一个宽泛的“开源”标签。永久来源：[sdkjs LICENSE](https://github.com/ONLYOFFICE/sdkjs/blob/9d2138866d929be32f1d107e8b5bfae36156e097/LICENSE)、[core LICENSE](https://github.com/ONLYOFFICE/core/blob/ed63e94de814ff1d4ea28cddfaba6939bc9df9ed/LICENSE)。本轮用途为阅读和独立设计，未引入构建依赖、子模块、产品代码或部署服务；若后续决定复用代码，再单独记录具体文件、条款与集成方案。

## Office LTSC已有结果接入

- [首轮静态报告](../OFFICE_RESEARCH_BASELINE_2026-10-18.md)：ProPlus2024Volume，主程序16.0.17932.21056；8模块身份/资源、3类型库、3小模块Ghidra。原始证据清单44文件本轮逐个哈希一致。XLCALL32回调槽、Word启动层、PowerPoint PPMain连接不等于核心算法。
- [Excel运行摘要](RES-002-EXCEL-2026-10-19.json)：16.0.17932.21000，EXCEL.EXE SHA256 `c71c7ca0faf896fd52e66924f7ec43ab8b7b110fa3b179097c1f0e3c6c036369`；3输入×3次、36阶段。算法仍NOT_LOCATED。首个budget运行缺少Calculation/country回读的原始缺口保留。
- 本轮只读注册表确认ProductReleaseIds=ProPlus2024Volume、Platform=x64、VersionToReport=16.0.17932.21000；实际EXCEL.EXE为69737792字节，同上述运行摘要哈希一致。没有读取密钥/账号。首轮21056与当前21000是不同身份，版本变化原因未调查，不能混成同一批二进制证据。
- E1的Zen与Excel比较由[ENG-004实况](../task_runs/2026-10-19-011-ENG-004.json)另行证明；RES-002本身仅是Excel与预定期望。新增类型/公式/链失效的原生同操作对照尚未执行。

## 开发顺序与可执行验证

| 顺序/冻结任务 | 本轮依据如何使用 | 下一步产物与验收边界 |
| --- | --- | --- |
| 1 / ENG-005日期意图 | 日期分支+workbookPr+既有Value2/Formula/Text分层 | 明确序号/日期意图、1900/1904、样式准入；新建独立日期样本，不修改res-002-v1 |
| 2 / ENG-005三业务对照 | Excel已有运行器及ONLYOFFICE依赖/历史线索 | 新版本预算/订单/考勤，各含类型切换、清空、公式替换、日期、保存重开；先冻结预期，再各3次Excel同操作；另外标ONLYOFFICE未运行 |
| 3 / ENG-006结构变更 | renameSheetCopy依赖先撤后建的局部静态观察 | 统一引用变更计划、图一致性与拒绝原子性；按冻结条件验证复制/插删/重命名，不能由源码阅读直接判PASS |
| 4 / RES-003→ENG-W01 | 已有Word Range/UndoRecord声明 | 先3份DOCX的范围/段落/组合撤销宿主观测；按真实差异再选ONLYOFFICE Word源码或WWLIB函数，排版另验 |
| 5 / RES-004→ENG-P01 | 已有SlideID/MoveTo声明 | 先3份PPTX的稳定ID/顺序/关系观测；按差异再定位ONLYOFFICE Slides/PPCORE；母版与视觉单列 |

下一次ENG-005日期切片先冻结新样本的0/1/59/60/61序号、两日期体系、闰日、日内小数、边界值、格式未支持/未知部件拒绝预期；各项目支持还是拒绝须预先写明。当前阶段不加入新的日期函数全集，不把ONLYOFFICE取代为唯一正确性裁判。保存重开同时核对值、类型、公式缓存、style引用、date1904、无关部件与undo/redo。

每轮从当前待办选一个有限切片，开始/结束按冻结任务书校准；实况记录所用来源提交、样本哈希、运行命令与逐条件结果。冻结任务书不变；此依据表可随证据演进，变化保留旧记录。静态设计完成不使ENG-005 DONE。

## 复现、撤回和待证伪点

1. 先额度、任务书、continuity检查；从清单URL固定提交取得指定文件到全新target/reference_sources子目录，用原始字节计算SHA256；Git blob身份按SHA1("blob "+字节数+NUL+原始字节)核对。不得以master最新文件替代固定版本。
2. 现有Office静态复现按首轮报告命令；本轮完整性复核读取target/office_research/2026-10-18-01/evidence-manifest.json逐项核对SHA256。运行复现按tools/office_research的隔离副本流程，不附着用户Office会话。已有宿主证据保留，本轮未重复运行。
3. 回滚为撤回本次文档提交或追加纠正记录；没有安装ONLYOFFICE、修改Office或改变源工作簿，也没有持有新REA/COM会话。缓存和原始证据保留，不能用公开摘要代替丢失的原始证据。

最可能被推翻的三点：日期函数某条分支不代表所有导入/格式化行为；历史事务与异步重算可能存在本轮未读路径；calcChain可疑字段映射可能有上下文或缺陷，必须先复现才能定性。以上均为待验证线索，不能作为放宽Zen准入的理由。

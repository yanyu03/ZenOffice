2026-10-23／实况049／ENG006 PARTIAL：预声明窄裸范围文本/绑定合同，5正向10拒绝独立样例；左限定向右继承SheetId，初始统一拒绝右限定/反向/3D等。未实现/未新测Rust或Office，人工坐标核对不算测试PASS。下一步按engine_specs/ENG-006-range-lexeme-v1.json实施测试与只读解析绑定，检查额度优先。见operations/2026-10-23-RANGE-LEXEME-SPEC.md。

2026-10-23／实况048／ENG006 PARTIAL：纯归一矩形插入扩张/删除裁剪/全失效政策已实现，7方法/24双轴字面矩阵PASS，全Rust377PASS6ignored/ReleasePASS。范围文本/限定继承/绑定和错误序列化未接，无结构写入/原生Office。下一步窄范围文本与绑定，再做函数/全拥有者和原子准入。见operations/2026-10-23-RANGE-PLAN.md。

2026-10-23／实况047／ENG006 PARTIAL：新增有界点引用算术语法顺序检查，7方法PASS、全Rust370PASS6ignored/ReleasePASS。无计算/优先级AST或结构写入权限，函数/范围/字符串等暂拒绝。首次负样本已被词法拒绝却unwrap的测试失败保留，分层断言修正。下一步范围与窄函数参数/全拥有者，CI同时复核启动观测。见operations/2026-10-23-ARITHMETIC-SYNTAX.md。

实况046 CI实测补充：e846bcf首次CI39947923681成功，最小Node启动10.755382秒>旧10秒整进程预算；预热后原5项1.528秒PASS、6新增PASS，未重试。支持启动耗时假设但具体原因/长期稳定未证实，ENG-FIX仍PARTIAL；下一正常任务CI继续核对。

2026-10-23／实况046／ENG-FIX PARTIAL：上轮最终文档CI39941046014也重复Node assess10秒超时，保留未复跑。本轮新增独立30秒最小Node启动观测/no-retry及6项故障样本PASS；原5项/10秒预算源码未改、本机PASS，未本机复现根因。新CI应记录启动耗时并保持原验收；不宣称稳定修复。Rust/Office未重跑，ENG006仍PARTIAL，下一步核对新CI独立宿主证据。见operations/2026-10-23-CI-NODE-STARTUP.md。

实况045 CI补记：44658a2冻结前置检查首次Node assess10秒超时，原日志保留；一次原样复跑成功（5项2.520秒），未改超时/预期，具体宿主原因未证实。详见本轮operations/循环，不把复跑成功说成未曾失败。

2026-10-23／实况045／ENG006 PARTIAL：只读公式/Workbook借用绑定已实现，保留显式/隐式表区别，按SheetId而非表序定位；未知/非Worksheet/定义名称/超限拒绝。7方法PASS，全Rust363PASS6ignored、ReleasePASS。无完整语法、包帧/revision许可、结构写入或新原生对照；下一步窄语法和范围/拥有者计划。见operations/2026-10-23-REFERENCE-BINDING.md。

2026-10-23／实况044／ENG006 PARTIAL：新增保留原文跨度/$/引号转义的窄单点引用词法扫描，7方法PASS、全Rust356PASS6ignored、ReleasePASS。词法成功不等于公式语法/函数/SheetId绑定或结构编辑准入；范围等明确拒绝，XLSX结构写入/原生对照0。下一步语法与稳定表绑定、范围/所有者计划。见operations/2026-10-23-REFERENCE-SYNTAX.md。

2026-10-23／实况043／ENG006 PARTIAL：计算AST不保留$固定标记/源跨度，已建立STRUCTURAL_EDIT_CONTRACT。新增只读单点坐标计划器和7独立例子，全Rust349PASS6ignored、ReleasePASS；0XLSX结构改写/0新原生对照，冻结C1–C4均NOT_RUN。下一步lossless引用span解析与范围/全拥有者准入，不能把坐标映射当作结构编辑已支持。见operations/2026-10-23-STRUCTURAL-REFERENCE-PLAN.md。

2026-10-23／实况042：ENG005冻结C1–C4有界PASS/DONE。三原业务各3次独立Excel成功运行，108阶段/27保存重开，原字面预期/Zen帧/类型/公式/缓存/日期体系格式一致；55Rust专项与5比较器PASS。原生丢弃customXML探针的包规范化单列，Zen保留门禁不放宽。2观测器pilot失败保留、实例均已退出。下一步ENG006结构设计，M2/E2–E3尚未整体放行。见operations/2026-10-23-NATIVE-TYPED-ACCEPTANCE.md。

2026-10-23／用户“先打开门禁”／实况041：同一Excel当前16.0.17932.21056、微软签名Valid、SHAc689d889…复核通过；独立新宿主参照research/EXCEL-HOST-2026-10-23.json已批准，原历史manifest/冻结锁不改。身份阻断解除，后续隔离对照仍须逐次精确复查；本轮原生0。三本地observer/comparator源文件待运行验收，尚未同步。下一周期可按新profile显式参数执行新隔离目录，不重放旧失败命令。见operations/2026-10-23-EXCEL-IDENTITY-REOPEN.md。

2026-10-23／实况040：原生类型observer/comparator已准备，5比较器方法及3Zen业务重跑PASS；预算pilot于COM启动前因既有Excel批准SHA不一致拒绝，原生0。按DSHA停止，不更新身份/重试。本轮工具与记录未提交推送，待人工核对Office身份及明确新授权；ENG005PARTIAL、本轮BLOCKED。见operations/2026-10-23-NATIVE-TYPED-IDENTITY-STOP.md。

2026-10-23／实况039：八项当前类型准入表已按c5c6dbc源码复核，C1设计PASS。修正可变契约日期待实现/未知错误标签歧义；3新增错误方法、46专项PASS（28会话/6日期/9policy/3默认）。未知标签仅原始读取、直接写错/变化错误拒绝，算法未放宽。C3新Excel0，ENG005仍PARTIAL；下一步三原业务×3次原生同操作对照。见operations/2026-10-23-ADMISSION-REVIEW.md。

2026-10-23／实况038：内存保存帧凭据已实现，成功发布后绑定完整字节/路径、原policy/limits/预算。三原业务24步骤继续通过，18凭据重开、9继续编辑及第二代保存、2516快照核对，篡改/锁/移动/预算与IO故障PASS。Rust339PASS6ignored、ReleasePASS；首轮路径/负样本与构建目标错误已记录修正。无新原生Excel，ENG005仍PARTIAL；下一步C1准入表与三业务原生对照。见operations/2026-10-23-SAVED-FRAME-RECEIPT.md。

# 引擎待办与交接

2026-10-23／实况037：显式受控自有policy已贯穿XLSX原门禁、候选重读、Session编辑/undo/redo及安全保存，实际临时字节发布前核验；普通入口仍拒绝unknowncustomXML。三原封存业务各8步首次PASS：24前向、33撤销重做对、1904逐格快照核对、9保存与普通只读重开；故障原子性及默认3拒绝PASS。完整Rust336PASS6ignored/ReleasePASS，最终测试内清单自校验6集成重跑PASS。C2/C4有界新PASS，C1/C3未关闭、ENG005PARTIAL；旧033FAIL保留。无新Excel，保存文件继续编辑仍拒绝、需保存帧重开凭据，不自动重算grant。下一步凭据与原三文件×3次原生对照，见operations/2026-10-23-EXPLICIT-POLICY-SESSION.md。

2026-10-23／实况036：新增不可变OpaqueXmlPolicy，初始源/稳定身份、拥有关系与有效媒体行固定；候选每次重验但不重算初始SHA。拒绝xml:base等未知元数据属性/内容；9新方法，22OPC+24描述/policy+3默认拒绝PASS。仍是只读组件，未接编辑/撤销/保存，无候选合法编辑/祖先关系证明或已保存重开凭据，三封存输入不变；原033C2FAIL保留、ENG005PARTIAL/BLOCKED。下一步XLSX显式组合原门禁并贯穿ReadWorkbook/Session，先确定保存帧重开身份，不宽松重试。

2026-10-23／实况035：新增独立只读opaque_xml声明核验器，单一显式自有叶子XML；输入/payload SHA、媒体、唯一workbook关系、已知绑定/路径/预算拒绝。15新增+22OPC+3原输入默认原子拒绝PASS，三封存输入仅描述核验通过。初轮预算夹具先被ZIP比例门禁拦截的失败保留，store-only后验证XML限额。未接入写路径、无新业务8步/保存/Office验收；ENG005仍PARTIAL/BLOCKED，原033C2FAIL不变。下一步先跟进本提交CI，再审查policy贯穿候选/历史/保存；禁止宽松回退或探针白名单。

2026-10-23／实况034：原47d30ee两CI实际成功、旧失败周期已真实收尾；正常业务0PASS3FAIL不变。只读审查3封存包：各1叶子customXML191/191/195字节、工作簿单入向/无出向、声明的绑定标记未见，不证明任意语义独立。新增CUSTOM_XML_PRESERVATION_CONTRACT.md：默认严格不变，受控自有来源显式grant绑定输入/部件/媒体/关系，语义独立是调用方前提，禁止namespace白名单；尚未实现/放行/重跑业务。ENG005PARTIAL/BLOCKED，本轮4条件NOT_RUN。下一步无写入grant验证器及通用/负向夹具，再做事务集成。

2026-10-23／实况033：首次真实三业务类型API执行器编译成功，但首步文本均被unknown custom XML dependency拒绝（0通过3失败、成功修改/保存0）；已有zen-opc保护不接纳自有探针。新增三真实输入原子拒绝回归3PASS，不能替代原8步。失败执行器/依赖差异留target，未推失败代码；原输入和清单不变。输入封存不等于写入口准入，ENG-005仍PARTIAL且现BLOCKED；本轮FAILED，C2FAIL，其他全条件NOT_RUN。下一步自有/不透明customXML通用准入和依赖契约，不为样本白名单、不删探针或改标准。见operations/2026-10-23-TYPED-API-ADMISSION-FAILURE.md。

2026-10-22／实况032：原字节封存三业务类型输入和原计划至RCD/engine-acceptance/eng-005-business-v2，清单SHAbc82f786…；7项核验器测试PASS，独立进程验证3文件16初始公式/缓存及日期元数据PASS。复核6布局+3日期图片身份，原预览1904差异保留，不主张原生Text等价。输入准备阻断已推进为固定输入就绪；Zen/Excel新业务执行0，ENG-005仍PARTIAL。下一步真正的Zen API8步类型业务执行器及历史/保存验收，再做Excel同操作对照。见operations/2026-10-22-TYPED-INPUT-SEAL.md。

2026-10-22／实况031：新增独立只读日期解释卡，7项边界/拒绝/源保护测试PASS，三固定候选实际生成并查看日期卡；订单1904的59.25解释为1904-02-29 06:00，三原包不变。不是原生Text/通用排版，前轮artifact预览差异保留。未封存、无新Zen/Office运行，ENG-005 PARTIAL。下一步综合既有包/六全表布局/三日期卡证据逐项准入封存，再开发业务执行器。见operations/2026-10-22-TYPED-DATE-INTERPRETATION.md。

2026-10-22／实况030：订单71d0703c…、考勤ea439538…从原计划新建并独立包准入PASS，共10初始公式；四业务图已实际查看。发现订单1904体系59.25被预览为2/28/00，两种日期体系独立控制图字节相同；导入/渲染内部原因未知，不归罪Zen/Office。布局PASS而日期解释FAIL，三输入暂不封存；无新Zen/Excel执行，ENG-005 PARTIAL。下一步日期体系可验证的独立只读预览/复核，再封存输入。见operations/2026-10-22-TYPED-CANDIDATE-DATE-PREVIEW.md。

2026-10-22／实况029：原47d5…预算候选被导入器拒绝于内容类型解析，窄包检查漏掉限定名称兼容。修正新创作器默认命名空间及独立门禁，26专项PASS。另建c76852a1…预算候选，包检查及两表实际渲染/视觉检查通过，原件不动；未封存、无新Zen/Office执行，ENG-005 PARTIAL。下一步另外两真实业务候选及三份准入/封存。详见operations/2026-10-22-TYPED-RENDER-INTEROP.md。

2026-10-22／实况028：独立新创作器8项专项通过；从原计划create-new写出1份预算候选3374字节（47d5bc24…），另进程只读包策略通过6项初始公式及元数据，字节未变。原3失败包不变；另外两案例仅内存单测。候选尚未渲染/视觉检查或封存，Zen/Excel新运行0，ENG-005仍PARTIAL。下一步先复核/渲染首份身份，再扩展另两真实候选；见operations/2026-10-22-TYPED-FRESH-AUTHOR.md。

2026-10-22／实况027：固定artifact-tool2.8.99公开文档和一次help查询未找到所需metadata写入口（不证明库无此能力）。主助手已在既有开发授权内选定独立从零创作新包的路线B，下一有限任务实施源码与测试；A保留为待公开接口的备选。不是修补旧输出或回改标准，旧生成器保持拒绝。ENG-005业务输入仍BLOCKED，本轮PARTIAL且C1—C4 NOT_RUN；无新证据不重复同一帮助查询。见operations/2026-10-22-TYPED-AUTHORING-ROUTE-PROPOSAL.md。

2026-10-22／实况026：新增独立只读包策略检查器，7项方法通过（3个内存正向包、30个独立拒绝样本）；核对日期体系、calcPr、日期xf、初始值/公式缓存及customXml关系。两次测试夹具语法启动失败已保留并修正；三旧失败候选不变。旧生成器未集成，未新建封存业务输入或启动Zen/Office，ENG-005仍PARTIAL。见operations/2026-10-22-TYPED-PACKAGE-POLICY.md。

2026-10-22／实况025：上轮额度阻断保留的独立元数据构造器已通过9项专项，原5项拒绝回归通过；显式日期体系、manual calcPr、builtin14样式及自有customXml关系可在新XML部件中构造。旧生成入口仍拒绝，三业务输入/整包集成未完成，ENG-005仍PARTIAL/BLOCKED于业务验收；本轮所有产品条件NOT_RUN，未进行Zen/Office新运行。详见operations/2026-10-22-TYPED-METADATA-COMPONENT.md。

第022轮／实况023：一次性第二heartbeat副手审阅已消耗，852 tokens，回答length截断；完整一条锁证据提醒已对照27份历史日志核实，不能算完整审阅或新增产品验收。ENG-004本轮所有条件NOT_RUN/PARTIAL，既有DONE不回改；ENG-005阻断不变。此后定时只登记真实事件，不读CMD_ENV、不请求副手；普通手动分工授权不扩大自动化权限。见operations/2026-10-19-WORKER-DEEPSEEK-ONCE-RESULT.md。

用户专项实测（cycle021／实况022）：仅通过本机Zen引擎编辑三份既有数字业务文件，当前003990c源码重编译CLI后，390个值、撤销重做、另存重开、原包/部件保持及拒绝保护全部通过；Office启动0次。此轮无新引擎实现，不代替ENG-005类型/日期验收，也不解除以下v2样本阻断。新实况因未执行外部宿主按C4保留PARTIAL，既有ENG-004限定DONE不回改。

最新复盘（cycle020／实况021）：用户明确要求检查事故后，确认三业务v2生成器漏写1904、calcPr及customXml，且没有确保日期样式符合计划；先生成渲染、后检查准入的顺序有误。不能据此认定导出库缺陷。新增生成前的实现覆盖检查，缺项即停止，5项负反馈测试通过；未重生成或修补候选。三份旧候选哈希不变、ZIP完整，但0份输入封存，Zen／Excel新业务运行仍0次，ENG-005仍BLOCKED。详见[事故复盘](operations/2026-10-19-TYPED-AUTHORING-INCIDENT.md)。原cycle019当时未同步的工具、失败实况及本次修正一并纳入审核；同步状态以cycle020的实际提交与远端核验为准。原计划与冻结任务书不变。

开发依据（2026-10-19用户新增要求）：[ONLYOFFICE与Office LTSC证据索引](research/DEVELOPMENT-REFERENCES.md)。ONLYOFFICE sdkjs/core固定提交的局部源码阅读与本机Office静态/实际宿主证据分层使用；不导入产品实现。日期序号有界切片已实测；验收准备686c431预先声明[三业务v2计划](engine_specs/ENG-005-business-v2.json)，27状态／144项独立算术核对通过。该准备周期当时生成0份；后续cycle019生成3份未通过准入的候选，不能算合格样本。下一步须先实现并独立验证计划所需元数据写入，保留失败原件；不能仅把能力检查改为READY。输入封存后才做Zen及原生宿主同操作对照，现有冻结样本与任务书不改。

当前阶段：E2日期序号切片（ENG-005 PARTIAL）。最新4f82fd7：显式体系与既有日期样式准入、序号写回/依赖重算/历史/安全保存，6项新日期函数、本机定向及Windows CI300通过/6忽略；3旧数字业务回归通过，新日期宿主0次。日期CLI、日历转换/显示、广泛样式和三新类型业务原生对照未完成。历史链切片：最新实测d81da82：294项Rust通过、6忽略，4项新增链合成回归；新Office对照0次。历史清除053811d为290项通过。公式替换前轮d991431为286项通过。历史类型首切片：a83fe4d已提供程序接口级类型输入，282项Rust通过、6项忽略；新增6项合成测试，3份既有纯数字业务回归通过，新增类型Excel对照0次。E1/M1数字会话闭环已在Windows可信父目录/create-new范围按冻结条件完成。E1验收基线08bc566：276项Rust通过、6项忽略，3份业务CLI闭环、390个逐步值及保存缓存与9次实际Excel观测比对通过。当前冻结任务书：taskbooks/ZEN-ENGINE-v1.0.md；实况单独记录，标准不回改。不是通用Office兼容或桌面产品验收。

| ID | 状态 | 下一步或证据 |
| --- | --- | --- |
| ENG-001 | DESIGN DONE / 未实现 | WORKBOOK_SESSION_CONTRACT.md：已核查现有接口、确定复用边界、状态转换与失败原子性；无新运行能力主张 |
| ENG-002 | DONE / 内存范围 | 4dd773d：zen-session数字编辑、不可变字节帧、undo/redo、分支/无变化与失败不变；9项新测试，2份独立工作簿；实况2026-10-18-004-ENG-002.json |
| ENG-003 | DONE / 限定保存范围 | 745545d：CLI、安全新目标保存、保存点与故障原子性；新增4项实际junction/大小写/点路径验收；C1–C4 PASS。仅可信父目录/create-new，不承诺目录命名空间CAS；见2026-10-19-008-ENG-003.json |
| ENG-004 | DONE / E1限定范围 | 08bc566：3份CLI业务、390值/缓存/部件保持/锁定拒绝通过；相同错误缓存门禁遗漏已修正，失败记录保留。见2026-10-19-011-ENG-004.json |
| ENG-005 | DONE / 既有格类型、公式与有界日期 | 实况042：3封存业务×3独立Excel成功对照，108阶段/27保存重开及55Rust专项PASS；显式owned-policy、严格默认、Windows可信create-new。见operations/2026-10-23-NATIVE-TYPED-ACCEPTANCE.md；ENG006结构编辑待开始，非M2整体DONE |
| W-001 | OPEN / 后续专项 | 横向字宽与兼容设置未修复，不阻塞表格会话主线 |
| RES-001 | DONE / 静态范围 | OFFICE_RESEARCH_BASELINE_2026-10-18.md：8模块身份/资源、3类型库、3小模块Ghidra；未开展Office运行对照；定稿后的回溯校准见task_runs/2026-10-18-002-RES-001.json |
| RES-002 | DONE / 限定宿主用例 | 97806b8：3份输入×3次独立Excel运行、36阶段及保存缓存通过；未见预设范围差异。C4按条件性差异分诊PASS，算法未定位/0函数会话；后续Zen差异再触发专项。见2026-10-19-010-RES-002.json |
| RES-003 / RES-004 | TODO / 后续研究 | Word范围/段落/撤销与PowerPoint身份/关系，分别按冻结任务书执行 |

循环报告位置：docs/engine_cycles/。未保存的本机额度状态、告警、暂停交接与进程记录只放target/quota_guard，不上传账户或线程身份。

周期任务只写当前ZenOffice仓库，不操作旧zen-office-v2。处理用户未提交修改时先检查，避免覆盖；工作开始前读取额度门禁与最新交接。

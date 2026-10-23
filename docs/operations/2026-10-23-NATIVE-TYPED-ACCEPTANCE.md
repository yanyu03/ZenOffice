# ENG-005三业务原生闭环验收

日期：2026-10-23。设备与固件：Windows x86_64运行时，未核验固件。工具：Python3.12.14、Cargo/Rustc1.99.1、DSHAownedhealth/SDK PASS；Excel16.0.17932.21056，已批准完整SHA见research/EXCEL-HOST-2026-10-23.json。样本数：3独立业务文件，每份3独立成功实例，另2失败观测器pilot保留。是否实测：原生COM/保存重开与55Rust方法。结论强度：冻结ENG005既有格/显式policy/日期序号有界范围通过，不是通用Office兼容。

预算03/04/05、订单01/02/03、考勤01/02/03共9次成功，各initial+8前向+3保存重开，共108阶段/72操作/27重开；2448COM格及612保存包缓存格按原封存字面预期核对，日期体系与builtin14/nativeformat不变，公式/类型/空白精确匹配。2244Zen比较是同3帧集按9次宿主重复比较（748唯一格检查），不算新独立样本。原生Undo0；Zen撤销另由Rust验收。

两次失败保留：预算01快照FinalRelease使外层worksheet别名失效，修正为每操作释放再读快照；预算02对仍打开的SaveAs文件Get-FileHash遭自锁，移至关闭后核验。预算02最初700ms退出检查尚在，后独立按原PID身份核对进程已消失；未kill任何进程。9成功实例均完整记录创建时间/路径/精确SHA、空工作簿所有权和实际退出。工具缺陷与产品差异不混算，不删除失败日志。

原计划允许单列宿主规范化：Excel输出27包均不再包含自有customXML探针，新增/删除/变化部件列表已记录；不是Zen保留放宽。Zen原字节/探针/关系/styles/workbook.xml/calcPr仍经原业务Rust逐步检查。日期Text只是观测，不作为等价oracle；本机观察NumberFormat均yyyy/m/d。

Related Rust：9lib+28session+6date+9policy+3default=55PASS，比较器5PASS，原输入清单SHA及字节不变。生产Rust未改，不重复无关全套/Release；既有342PASS6ignored/Release属于310c35a历史。新增observer/comparator/tests与CI方法检查现可按实际证据提交，全部原始回执/文件留target/office_research/ENG-005-native-042，脱敏哈希摘要见research/ENG-005-NATIVE-TYPED-2026-10-23.json。

实况042 C1–C4逐条PASS、ENG005有界DONE；原033FAIL/040BLOCKED不回改。下一步ENG006结构编辑设计，M2/E2–E3整体未放行。最可能推翻点：显式语义独立前提不能扩展任意customXML；有限日期格式/序号不能推出区域排版；9次宿主内容一致不能推出任意包兼容。

复现：先quota与冻结校准；明确新hostprofile参数，经DSHA envfeel/python调用observe_typed_excel.ps1的新隔离目录，独立verify_typed_excel.py对固定plan/实际回执/Zen帧比较；test_verify_typed_excel.py为无Office单元负向检查。回滚：审查后续提交后revert本轮工具/CI/摘要；保留旧冻结输入/任务书/失败，不自动重放原生操作。最终提交/CI与远端同一性见continuity周期22a2b2fdd24f4407b41dbac5b26f43fa。

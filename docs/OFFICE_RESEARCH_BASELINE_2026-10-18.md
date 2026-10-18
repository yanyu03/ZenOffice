# Office首轮逆向依据报告 — 2026-10-18

日期：2026-10-18。设备与固件：本机Windows x64（本调查不涉及固件）。目标：Microsoft Office LTSC 2024 专业增强版，ClickToRun产品ID ProPlus2024Volume，主程序版本16.0.17932.21056。

工具与版本：REA 6.4.0、Ghidra 12.1.4、Node24.14.1、JDK21.0.12.1、Python3.12、pefile2024.8.26；Windows oleaut32 LoadTypeLibEx(REGKIND_NONE)。

样本数：8个模块的身份/资源、3份类型库、3个小型模块的Ghidra会话；每个对象单次静态观测。是否实测：静态实测，未启动Office、未执行厂商程序、未运行原生宿主对照。结论强度：对象声明和所选函数的静态观察；设计建议为独立实现方案，不是Office内部算法还原。

## 输入身份

路径共同前缀：本机 Microsoft Office/root/Office16。8个文件的Authenticode状态均为Valid（安装时本机观测）；前后SHA256逐个相等。OLB与主程序的文件版本不完全相同，不能抹平版本差异。

| 文件 | 字节数 | 文件版本 | SHA256 |
| --- | --- | --- | --- |
| WINWORD.EXE | 1639144 | 16.0.17932.21056 | `bdb93fddb7267ad3c8e8cea01739c4888554f52f1bd6d2f92fc92782e90f4b73` |
| EXCEL.EXE | 69756656 | 16.0.17932.21056 | `c689d889d65140b5d3fe549f2d6bfae22f7084202f749418f1739b5cb6e31ba5` |
| POWERPNT.EXE | 1879816 | 16.0.17932.21056 | `de81916302381d2f6b44dd0569426a5b1d1142508ac859c6d20ebb659767105c` |
| MSWORD.OLB | 954792 | 16.0.17425.20008 | `b4f5393bf3f1b3cbbaa00d322a901c042f8572eb668511275ac32e75c5f53bb3` |
| MSPPT.OLB | 444392 | 16.0.17928.20018 | `1e4fc5cf02e18753d242406b1546a9ef55c11e9bf3f008fbf0bd3868a11a3ad7` |
| WWLIB.DLL | 54524648 | 16.0.17932.21056 | `92cd39b6d3e20a6ba3b72dd6c548197f9ddb61523d54cde6446193fbfe2a901e` |
| PPCORE.DLL | 33790208 | 16.0.17932.21056 | `ea57c6e1b3c4717a2cc210ce926e16fca324514984177ad9a00c83a34a77eedc` |
| XLCALL32.DLL | 33952 | 16.0.17425.20008 | `46b393eaeff873ee88e714dbd67efca24445d50e79208ed104072bc95e28920d` |

## 声明面：三件套对象边界

下表来自本机类型库原始JSON的types[name]及members[name,dispid,invocation]，只说明静态声明存在。没有由成员数量推导功能完成率。

| 产品/类型库 | 本次类型数/选读类型数 | 观察到的接口 | 对Zen的独立设计启发 |
| --- | --- | --- | --- |
| Excel/EXCEL.EXE，GUID00020813-0000-0000-c000-000000000046，1.9 | 1035/12 | Workbooks.Open；_Workbook.SaveAs、Saved；Range.Value2读写(DISPID1388)、Formula读写(261)、Text只读(138)；CalculateFullRebuild | 把输入值、公式和显示文本区分；会话持有工作帧、计算状态和保存点，格式层处理写回与包保持 |
| Word/MSWORD.OLB，GUID00020905-0000-0000-c000-000000000046，8.7 | 767/14 | Documents.Open；_Document.Range/Undo/Redo/SaveAs2；Range.Text读写、InsertAfter；UndoRecord.StartCustomRecord(1)/EndCustomRecord(2) | 用范围/段落/运行映射编辑，不依赖当前光标；一个逻辑编辑可对应一个事务撤销单元，复用共同会话机制 |
| PowerPoint/MSPPT.OLB，GUID91493440-5a91-11cf-8700-00aa0060263b，2.12 | 348/13 | _Presentation.SaveAs(2036)；_Slide.SlideID只读(2009)、MoveTo(2030)；Shapes.AddTextbox；TextRange.Text读写 | 把稳定ID、展示顺序、包关系分开；对象命令通过会话事务提交，避免重排后引用指错对象 |

依据位置：target/office_research/2026-10-18-01/typelibs/{EXCEL.EXE,MSWORD.OLB,MSPPT.OLB}.typelib.json；这些派生原始数据只留本地，不加入公开仓库。

## REA资源与函数面

- 三个EXE资源分别为Excel1227、Word242、PowerPoint258项，REA coverage均为complete；该complete仅限定本次资源枚举，不指引擎功能。证据：
  - Excel：`ev_5f1faa3762b660fae432e5fe17610bee2f1a602e6ab86d91cb154b7319206372`
  - Word：`ev_4066b891a7d4d28b7a0dfa60a588a1c9f004c6505afc4fddf19602cbeabf2d34`
  - PowerPoint：`ev_ae98b603fe94cda6a52f5d5d6737a7d41570f271dfca5791b263d5beef2e4645`
- XLCALL32.DLL（33952字节）单次Ghidra观察38个procedure、48个string。Excel4v位于0x1800011b0；反编译表现为检查一个全局回调槽，未设置时返回0x20，否则进行间接调用。槽的初始化者、真实callee和返回码语义未解，不能据此声称掌握Excel计算算法。函数证据 `ev_5b8a4548b136eb8262989af75c9bc285db418c45621671dee40483d506c28b10`，callee证据 `ev_3a48c3942ea1ef8caa4a0fd2de27829c2ea95c82492529fec514ba0de6eb406e`。函数字节范围未专门测量。
- WINWORD.EXE单次观察250个procedure、186个string。入口0x140001610仅显示安全cookie初始化及调用0x14000148c；该层是启动线索，未定位文档排版实现。入口证据 `ev_3e541b8188dcc20afac79b1e693b72b4c84e9b3dab63a2fa06076d4aab8e1305`。PE延迟导入可见Mso20Win32Client.dll的3个ordinal；本轮未分析该库。
- POWERPNT.EXE单次观察147个procedure、109个string。入口0x140001a10同样是启动层；PE延迟导入指向ppcore.dll!PPMain，说明静态模块连接关系，尚未证明实际对象编辑过程。入口证据 `ev_4d797d34826f9e112cc5cd96398f173238bd7ba4c81a8689917db5c3857a5c54`。延迟导入依据为本地pefile输出native/delay-imports.json；不是运行调用跟踪。
- 原生批次总耗时87.078秒，3个provider会话关闭操作完成。完整8模块资源证据、原始工具响应、4项record_unknown及导出包保留于本机native目录。没有复制厂商图标、SDK、可执行文件或反编译伪代码进入公开源码。

## 文档面与实现形式建议

1. Excel的自动化Undo不能直接当作Zen的程序编辑撤销方案：微软文档限定它撤销上次界面动作且不能撤销VB命令。因此Zen应保持独立的帧/命令事务，原生对照关注内容和重算，把UI撤销与自动化行为分开测。[Microsoft Application.Undo](https://learn.microsoft.com/en-us/office/vba/api/excel.application.undo)
2. Word公开支持把若干动作组合成一个UndoRecord；本机声明与这一接口形式吻合。Zen可以独立实现逻辑事务分组，但不能声称与Word撤销细节等价。[Microsoft UndoRecord](https://learn.microsoft.com/en-us/office/vba/word/concepts/working-with-word/working-with-the-undorecord-object)
3. PowerPoint的SlideID适合作为稳定身份的参照，顺序另行表达；具体文件ID映射须用自制演示稿重排/重开对照。[Microsoft SlideID](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.slide.slideid)
4. CalculateFullRebuild提供对照接口线索，不能由声明推导重算正确性；对照程序应冻结期望、数据类型、日期体系和数值容差后再执行。[Microsoft CalculateFullRebuild](https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatefullrebuild)

建议的Zen调用链：CLI/未来UI → zen-session事务/诊断/能力门禁 → zen-formats格式编辑计划 → zen-engine计算或zen-word/zen-slide内容模型 → zen-opc保包/安全输出。现有代码复用优先；模块位置是设计选择，不是从Office私有代码推导的架构事实。差异驱动逆向优于无问题地扫描大DLL：先用原生文件对照定位行为差异，再选择核心函数和引用链调查。

## 未知、可被推翻的点

REA已record_unknown：Excel真实计算/依赖边界；Word范围/段落/排版语义；PowerPoint稳定ID与对象编辑语义；3个大核心模块的具体实现。具体unknown_id见本地unknown-0..3.json，未以口头推测填空。

最可能改变设计的3点：①Value2/Formula声明不能覆盖数组、日期、外链、动态依赖语义；②Word范围/撤销声明不能解释字体塑形、断行、兼容模式；③SlideID及PPMain静态连接不能证明重排后的包关系和未知对象保持。这些必须由RES-002/003/004的独立样本对照及按需核心调查校准。

## 复现命令与回滚方式

在仓库根目录，使用DSHA host.json里的Python，先运行tools/quota_guard/guard.py --check。首次用pwsh -NoProfile -File tools/office_research/inspect_typelibs.ps1 -OutputDirectory <全新目录>/typelibs；再用Python运行tools/office_research/inspect_native.py --inventory <目录>/typelibs/inventory.json --output <全新目录>/native --dsha-scripts <本机技能>/scripts。依赖pefile==2024.8.26；较重步骤通过额度guard --run。不要覆盖旧证据或反复导入同一大目标。

本轮pe-summary.json的最初导入项未包含延迟导入，另存delay-imports.json补充读取；复现脚本已加入delay_imports，未重跑已完成的Ghidra会话。回滚：关闭自己拥有的MCP/provider会话，保留原始证据；没有Office文件内容修改需要恢复。调查目录可以移到本地归档，不执行跨目录递归删除。任务书冻结后不因调查差异回改；另写实况偏差和后续任务。

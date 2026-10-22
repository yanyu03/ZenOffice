# 订单、考勤候选与日期预览差异

实况030，源码58f3b89806a91d9662c9df992ef915cee89a89b4；范围为输入准备和只读渲染，不是Zen/Excel产品运行。沿用原计划SHAebd6c6dd390a4a1890eb2b1ec78f54d18d828ab8a1201d65676849bded820dba，未改任务书、预期或原始失败包。

通过DSHA Python3.12.14，从原计划create-new生成orders.xlsx（3307字节，SHA71d0703cc63d1ab66284aadcff3d54fe0f09f47102068306726c6c0fa7cd8d96）与attendance.xlsx（3342字节，SHAea4395387368189d5e05451776cca51e56a439836757325b734af7e8e8659ea2）。另进程独立包检查均PASS，各核对5个初始公式和缓存，以及数值、标签、date1904、builtin14日期样式、manual calcPr和保留探针。计划独立算术核对27状态/144断言PASS，不算引擎执行。

DSHA Node24.14.1及artifact-tool2.8.99导入并渲染四视图：两个Summary A1:G8、Orders/Attendance A1:G12。四PNG均实际查看。订单375+160=535、考勤164+135=299和标签可见，无观察到截断/####。全部源字节保持，未导出任何XLSX。

## 真实差异与独立控制

Orders B9的原始XML为数值59.25、s=1，workbookPr date1904=1；预览显示2/28/00。微软[日期体系说明](https://support.microsoft.com/en-gb/excel/date-systems-in-excel)区分1900/1904基准；标准库独立日历算术1904-01-01加59.25天为1904-02-29 06:00。此算术不是Excel实测，也不把显示字符串用作原计划的数值相等预期。

在执行前登记两份独立自有控制：从同一订单规格分别新建1900/1904包，订单worksheet字节完全一致，仅workbook.xml日期位和Summary体系说明不同。未修改业务候选或原计划。两包经相同公开import/render路径渲染Orders A9:D9，PNG字节相同（SHAb58f56c41b2938de728c59a479cfe1e500a2246ae2747e79ea1a26ee5a2cffc6），两图实际查看均2/28/00。1904控制恰与业务订单候选字节相同，不计为新增独立业务样本。

可复核结论：该版本公开导入/预览路径在此固定案例没有呈现日期体系差异。尚未定位导入层或渲染层内部原因，不推导Office或Zen日期错误，也不宣称库普遍不支持1904。包准入PASS不等于显示语义PASS。

## 处置与复现

本轮记录布局PASS、订单日期解释FAIL；三输入正式封存数仍0，未新建RCD输入清单。保持原59.25、1904体系和原计划不变，不靠加1462或改1900掩盖预览差异，不重放旧失败生成器。

本地证据位于target/office_research/ENG-005-candidates-030：create.py调用write_new_candidate原计划两案例；check.py独立check_package；render.mjs只读importXlsx和render；date-controls.py从同规格分别build_candidate；render-controls.mjs固定身份渲染对照。每步经dsha.py envfeel --require python或node --run对应工具执行；脚本和实际回执哈希、四业务图/两控制图身份见[证据索引](../research/ENG-005-CANDIDATE-DATE-PREVIEW-2026-10-22.json)。复现须另用新目录，不覆盖现有输出。

下一步为日期体系可验证的独立只读预览/复核方式，明确1900边界与1904日历解释后再决定三文件输入封存；不重复既有未命中的接口帮助查询、不改验收条件。随后才进行Zen API类型/日期执行器及Excel同操作对照。ENG-005本轮PARTIAL，C1至C4 NOT_RUN，原任务DONE不回改。未改Rust源码，不重复全套Rust回归。

# ENG-005 元数据构造组件验证

2026-10-22（Asia/Shanghai），起点cae1de00c332b5b65785a361adba2bb93cb057b5。上轮实况024在额度UNKNOWN时保存的源码草稿本轮经审查验证，旧记录保留BLOCKED且不回写。本轮独立实况025为PARTIAL，产品冻结条件全部NOT_RUN。

## 新增能力与实际结果

新增tools/office_research/typed_metadata.py是无文件I/O的纯构造器，仅返回六个新XML部件：workbook、styles、workbook关系、包根关系、内容类型表与自有customXml探针。它显式输出1900/1904、manual calcPr及关闭的fullCalcOnLoad/forceFullCalc、builtin14的无子节点日期xf。调用者须另行构造两张工作表并显式引用样式1；本组件不写工作表、公式缓存、日历转换、XLSX文件或封存manifest。

本机DSHA EnvFeel验证Python3.12.14及owned health fixture后实际运行9项unittest全部通过：原计划3案例日期体系、manual配置与节点顺序、builtin14样式、probe拥有关系/内部路径、内容类型覆盖、Unicode/XML转义、非法参数拒绝、内存ZIP往返及确定性/独立返回值。内存微型包由测试自造工作表，只证明选定XML/ZIP断言，不是三业务候选或Office有效性证书。

使用Node24.19.0运行原5项preflight回归全部通过；旧生成器仍AUTHORING_NOT_READY，没有将implemented标志改true。源码基线106项通过；工作流增加本组件专项及触发路径。没有Rust源码变化，不重复无关Rust全量。原三份失败候选SHA256逐项与事故记录一致。没有渲染、封存合格输入或新Zen/Excel/ONLYOFFICE业务执行。

## 恢复与证据边界

本轮官方额度86%、本地起初UNKNOWN。只读定位发现上轮提供的bin子目录仅有rg.exe，不含codex.exe；本轮使用实际存在的官方客户端执行guard --once，恢复READY。不改变门禁阈值、不伪造账户数据、不解锁低额度；未改变全局环境或监测器配置。统一exec启动器不可用，使用本地Node execFile以参数数组运行明确程序。票据真实事件登记ordinal4，已attempted；未读取CMD_ENV或调用外部模型。

本地日志target/typed-metadata-025-tests.log与target/typed-metadata-025-preflight.log保留。身份：

| 产物 | SHA256 |
|---|---|
| typed_metadata.py | 3f42c31c0949b54990e1e151aa2327c37e94bec3eced6722db6555ccf58973e7 |
| test_typed_metadata.py | 59b9ac4b12bcc7ddad88c606a9a396bb7789e4160f42b89a7ad514b72e25c2e8 |
| 九项测试日志 | 055f74a20ca84067f91574efb965e461d51c2221dd8dc80de82f7d2ac467cf75 |
| 五项拒绝回归日志 | 5e9e6fe1bb928e1e130803f667c5cff65aa812d8bd2777cec82b2a16f877fc29 |

## 尚未完成与下一步

旧生成器未集成此组件，三业务输入仍BLOCKED；不补写旧导出包，不用更换生成器自动重试。下一有限任务须先确定符合既定构造政策的整包创作接口与独立准入，分别检查真实日期格样式、工作表/缓存、1904及自有部件，再考虑输入封存与正式引擎/宿主对照。若路径需要调整已有标准，另提版本变更，不回改任务书或v2计划。

同步：本轮相关源码、CI、上轮失败实况及本轮证据一起审核提交；实际提交/远端一致与CI句柄保存在continuity，不预报CI成功。回退限代码/文档提交，不删除失败输入、原日志或票据used状态。

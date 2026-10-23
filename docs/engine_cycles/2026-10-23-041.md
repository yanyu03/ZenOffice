# ENG-006单点坐标计划首切片

日期2026-10-23；Windowsx86_64运行时，固件未查；Python3.12.14/CargoRustc1.99.1/DSHAownedhealth+SDK PASS。样本：手写坐标向量，7测试方法，0新XLSX。是否实测：Rust本机；结论强度：纯坐标数学，不是公式/包结构编辑。

前置ENG005042有界DONE及9cfa54eCI已核对。源码审查确认计算CellReference/AST不保留$标记和源跨度，不能拿来保真改写。新增reference_plan：显式SheetId/CellAddress/固定标记，单轴插删点映射及复制偏移；区间/越界/数量预算失败不给部分结果且输入不改。Deleted不授权#REF!写回。

新增7方法PASS，全workspace349PASS6ignored、ReleasePASS。没有词法AST、范围、名称、结构元数据/包事务/会话接入，没有新原生Office；C1–C4全NOT_RUN，ENG006PARTIAL。证据research/ENG-006-REFERENCE-PLAN-2026-10-23.json、预先规格engine_specs/ENG-006-reference-plan-v1.json、完整契约STRUCTURAL_EDIT_CONTRACT.md。

最可能推翻点：点不能代表范围/隐式绑定；计算依赖图不覆盖所有包拥有者；坐标政策无新Excel结构对照。未来先lossless引用span解析/范围与拥有者准入，再一项结构写入；不放宽当前门禁或把子模块通过标整任务DONE。

复现：DSHA声明python/cargo，通过guard跑cargo test --locked --offline -p zen-engine --test reference_plan，workspace/all-targets及Release。本轮新模块导出影响当前导入指纹，已记before/after及原因，原始迁入哈希不变。回滚审查后revert本轮新模块/导出/测试与可变指纹，原任务书/历史输入不改。最终同步/CI见continuity周期6bc8037eadc347dcbb39d8b4752ded64。

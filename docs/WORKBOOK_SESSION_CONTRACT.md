# ENG-001：共用工作簿会话契约

2026-10-18，设计完成，尚未实现。依据当前源码的ReadWorkbook、WorkbookCalculator、write_number_edit和save_atomic；不重新实现XML解析、依赖图或包保持器。后续实现为ENG-002/003，不能将此文档称为会话功能通过。

## 现状与确切缺口

- ReadWorkbook::read/open提供有界只读工作簿、包和诊断，公开workbook/package访问器；模型读取能力不等于可写能力。
- ReadWorkbook::write_number_edit检查准入，在原包上修改已有数字，创建计算器，重算并维护已支持的缓存，返回候选字节；没有改变原ReadWorkbook，也没有维护用户编辑历史。
- WorkbookCalculator::set_input/set_formula与脏重算是计算层能力，不能单独证明所有输入类型都能安全写回OOXML。
- save_atomic只处理给定字节的文件替换，不负责源状态、工作副本、撤销或未保存标记。当前CLI一次操作一次退出。

## 放置与复用

新增zen-session crate，依赖zen-formats和zen-engine，首先提供WorkbookSession。会话负责操作事务与文件状态，格式层继续负责支持范围与包编辑，计算层继续负责值与依赖；CLI调用同一会话接口。通用DocumentSession的元信息和诊断可以随后提取，但不先造三种格式都未实现的万能编辑枚举。

## 会话数据

源路径与原始有界字节身份、读入限制、不可变工作帧、undo/redo游标、保存点、单调递增revision、诊断、历史内存预算。字节帧可用Arc共享，避免撤销时深拷贝。预算和文档限制公开可配置；不足时拒绝新操作并保留会话，不能悄悄丢弃旧撤销记录来宣称成功。

dirty由当前工作内容与最近成功保存点决定，不直接等同于“调用过编辑”。源文档身份与另存目标身份独立；Save As不能在读取旧源状态前悄悄把源身份切换成输出。初次范围是已有数值输入，E2逐步扩展类型与公式，Word/Slides后续复用共同事务约定。

## 状态转换

| 操作 | 提交条件与结果 | 失败要求 |
| --- | --- | --- |
| open | 有界读取和ReadWorkbook成功后创建首帧，undo/redo空，dirty=false | 不创建半初始化会话，不写原件 |
| set_number | 验证finite、地址、现有输入、历史预算；调用write_number_edit；用同限制重读候选；全部成功后一次替换帧、增加revision并清除redo | 不改变当前帧、revision、历史、dirty或文件 |
| 相同值输入 | 依据有效输入数值判断无变化，保留既有字节和历史 | 不因仅重新打包就制造修改 |
| undo/redo | 移动到完整有效帧，恢复数据与缓存；revision递增，保存点不移动 | 没有可用历史返回明确无变化，不改文件 |
| save_as | 检查源变化与目标策略，候选内容完整后调用save_atomic；成功后才更新保存点和dirty | 锁文件、权限、磁盘错误、源变化保持当前可编辑状态和旧输出；允许用户处理后重试 |
| close/discard | 由调用方明确选择，不自动保存未确认内容 | 不把关闭视为保存成功 |

源状态初次可用有界原字节精确比较，不使用不稳定的std::hash作为持久身份。要另行定义路径别名、符号链接和同路径替换行为；检测时刻的比较不能冒充与外部写入原子的compare-and-swap。同路径保存必须完成平台相关的并发/原子性验证，未完成时明确拒绝，不绕过检查。

## 对外接口草案

open(path, limits, history_budget)、from_bytes(bytes, limits, history_budget)、workbook()、diagnostics()、revision()、is_dirty()、can_undo()/can_redo()、set_number(sheet,address,value)、undo()/redo()、save_as(path, policy)。返回类型区分Changed、Unchanged与明确错误；调用方不能直接修改内部工作帧。

接口名称可以随实现调整，必须保留状态转换及失败原子性；初次不把格式层拒绝边界放宽。后续若单次重读造成性能问题，先测量再用有验证的编辑计划与增量路径替换，不先造第二套计算状态。

## ENG-002/003验收清单

1. 同工作簿两个独立输入连续修改，下游公式每步符合独立预期，重开候选后再次核对。
2. undo/redo跨两次修改正确；撤销后新编辑清除redo；相同值编辑不增加历史。
3. 非有限数字、公式格/受保护或未知依赖拒绝、预算耗尽、解析失败均保持所有会话状态及源字节。
4. 保存成功后dirty=false，撤销偏离保存点则true，回到保存内容后false；保存失败不移动保存点。
5. 源外部变化、目标已存在、独占锁、同路径/别名与输出错误有独立检查；未测的平台路径不宣称可用。
6. 命令行多步编辑使用同一服务；为未来UI预留诊断，但不先实现界面。

本契约不证明代码已通过测试。下一周期优先实现内存工作帧及set_number/undo/redo，不同时扩展文本、公式、结构与三个文档引擎。

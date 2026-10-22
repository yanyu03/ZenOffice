# 不可变custom XML policy与候选核验（实况036）

新增OpaqueXmlPolicy。bind_initial核对初始SHA后私有复制声明；域分离、长度前缀的policy身份覆盖初始包、调用方声明和拥有关系/有效媒体行。候选包可随正常工作表变化或ZIP重压缩改变整包SHA，但逐帧重新核对payload、关系、媒体、绑定、活动内容与预算，不回写初始SHA。validate_initial始终要求原始包身份，不能将候选或另一个来源自动纳入。回执的source_sha256表示当次实际检查字节。

精确固定拥有关系和有效媒体XML行；关系文件中的其它关系可变化，未来仍须普通编辑门禁同意。TargetMode=Internal的等义追加、default变override等变化也明确拒绝。字符串/行载荷保留预算16KiB，不冒称全部堆内存。调用方改动原声明不改变已绑定policy。

本轮还审查出元数据解析可能忽略未知属性/内容的边界：描述核验新增拒绝xml:base、未知根/行属性、嵌套元素和非空元数据文字。6故障样本在初始描述、绑定与候选检查分别拒绝。此改动不触碰生产默认写入口。

本机真实结果：9新增方法；OPC22项、描述/policy24项、原3输入默认原子拒绝全部PASS，共49项。DSHA环境/健康/SDK前提PASS；三封存输入字节与原清单不变。证据与源/日志哈希见[独立记录](../research/ENG-005-OPAQUE-XML-POLICY-2026-10-23.json)。

这些是只读不变量检查，不能证明候选由允许编辑产生，也没有真正session/undo/save接入；测试中的合成候选不是新业务成功。policy不是编辑授权或任意语义独立证明。原033 C2FAIL保留，ENG-005仍PARTIAL/BLOCKED，全冻结条件本轮NOT_RUN。无新Excel观测。

下一轮：只在XLSX显式路径中复用全部原包安全/关系/标签/保护/公式/资源门禁，仅让已经通过policy的对应customXML分支被满足；不得删除部件再验或捕获任意错误后宽松重试。ReadWorkbook需在候选重读时保留policy，Session在提交/restore/保存前重验。修改后的保存文件不能用原validate_initial重开；需要与实际已保存帧和policy身份绑定的重开凭据/会话API，先完成该设计，不自动重算grant源哈希。随后才恢复原三业务8步执行与原生对照。

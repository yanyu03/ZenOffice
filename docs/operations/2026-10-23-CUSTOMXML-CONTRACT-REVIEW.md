# custom XML写准入契约审查

日期：2026-10-23（Asia/Shanghai）。设备与固件：本机Windows x86_64，固件未测。工具与版本：DSHA Python3.12.14、标准库只读ZIP/XML；源码47d30ee。样本数：3份原封存输入、各1个custom XML部件。是否实测：只读结构/字节审查；引擎编辑和原生宿主均未运行。结论强度：实现设计，不是写准入放行。

先恢复原实况033周期：CI39815580821、39815580826均在47d30ee实际success，Git本地/远端一致且干净；只说明负向回归/既有基线通过，原业务0PASS3FAIL保留。结束旧周期后，本轮仅推进一个有限契约审查，不重放失败执行器。

DSHA环境门禁通过后，先核验清单SHAbc82f786…，只读审查三包payload、媒体类型、关系及已声明绑定标记：各8部件，customXml/zen-probe.xml分别191/191/195字节；叶子文本，application/xml，xl/workbook.xml的一条内部customXml入向关系，无出向；本轮声明标记集合未发现匹配。源和清单不变。标记为空不是任意语义独立证明。

[实现设计](../CUSTOM_XML_PRESERVATION_CONTRACT.md)选择默认严格不变、受控自有来源的显式policy路径。grant绑定首次输入、部件字节/媒体/图以及调用方语义独立声明；声明是额外信任前提，不能从XML文字/namespace推断。禁止测试探针白名单和删部件后验证。只允许原样保留，已知绑定/未知依赖/宏/签名等其余门禁照常；候选/历史/重开/保存维持同一policy及失败原子性。先做无写入的准入模型及正负故障验证，再谈接入执行器。

复现本轮只读调查：先运行verify_typed_inputs.py固定清单，再使用target/office_research/ENG-005-customxml-034/audit.py（DSHA envfeel --require python --run python --）；脚本/日志/JSON身份及完整探针拓扑见[证据索引](../research/ENG-005-CUSTOMXML-CONTRACT-2026-10-23.json)。源码基线与任务书校准按收尾记录。回退仅撤销本轮文档，不动任何引擎/输入/任务书/历史失败记录。

没有生产代码修改、没有新增门禁许可、没有产品运行；ENG-005保持PARTIAL/BLOCKED。本轮4原条件NOT_RUN，原033C2FAIL不回改。下一步无写入的grant验证器，优先证明默认路径不变和身份/拓扑/绑定负向，然后事务集成。可推翻点见契约末节；尤其不能由一次受控探针拓扑推广为任意custom XML可编辑。

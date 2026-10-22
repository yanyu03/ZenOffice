# 新预算候选导入失败与修复实测

实况029，起点6068332，周期4db2adf8039442d183d13d8860b65f7a。范围是候选准备工具，冻结ENG-005条件不变。

原预算候选SHA47d5bc24b376e9f5c61f85055f59ab35b6d6fca65e53340daf0687eb57a45664在DSHA环境检查PASS后，artifact-tool2.8.99导入失败，栈指向System.IO.Packaging内容类型解析。首次未捕获任务内部异常，第二次仅加入诊断并以同一身份输入复现，均未生成图片。两次日志留target/office_research/ENG-005-render-029。属于TASK_FAILED，不是环境拒绝。

新创作器用ElementTree输出带ns0前缀的Types/Default/Override。现有独立检查器只比较展开名称，遗漏了读取器兼容约束。微软[公开Packaging源码](https://source.dot.net/System.IO.Packaging/System/IO/Packaging/ZipPackage.cs.html)在ParseContentTypesFile比较reader.Name与Types、Default等字面名称。该公开源码支持原因假设，未宣称它就是本机WASM组件的固定源码版本。

修正仅从零创作的内容类型部件：使用局部默认命名空间，无进程全局命名空间注册。独立检查器增加限定名称兼容门禁；元数据新增1项回归、包检查新增1项前缀负样本。原有内容类型负样本保持默认命名空间，确保仍测各自缺陷。26方法专项PASS（10元数据、8包检查、8作者），源码基线106PASS。不是完整OPC/XML schema校验，不宣称带前缀XML一般无效。

从未改动的原计划另建budget.xlsx（3369字节、SHAc76852a1ad28b717fb9b44ea3f0afd7a846c19862776dabf8065cc7b65035531）。独立只读包策略PASS，6个初始公式及缓存符合原计划。相同DSHA Node24.14.1、相同artifact工具成功导入新文件并渲染Summary A1:E8、Budget A1:E14。逐张实际查看两张PNG：标题/标签/金额可读，无观察到截断或####；总差额1300，Summary1300/46315，日期显示10/19/26，Review state7。原候选和新候选前后字节保持，无XLSX导出或修补旧包。

证据、源码/日志/图片哈希见[独立实测索引](../research/ENG-005-RENDER-INTEROP-2026-10-22.json)。原47d5…候选保留为失败证据，不能封存为合格输入。旧三失败包未处理。

本轮未封存业务输入，未执行Zen/Excel/ONLYOFFICE，未重跑未改动Rust，ENG-005 PARTIAL，C1至C4全部NOT_RUN。下一步为另两业务生成新候选并独立检查/渲染，三份全部准入后才能封存，再实现类型/日期引擎及原生对照执行器。不能由本次渲染成功推导引擎编辑完成。

提交推送后核对本地HEAD、远端main、工作区及该提交CI；实际句柄/结局存连续性检查点。

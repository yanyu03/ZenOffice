# 只读任务书与执行实况

冻结标准位于docs/taskbooks/ZEN-ENGINE-v1.0.md及tasks.json，lock.json保存两者SHA256。三份文件在本机设为只读；Git不保存Windows只读属性。新检出先按lock.json核验哈希，再设置文件只读属性，不能重建锁。只读和哈希防误改，不抵抗有本机权限的主动篡改。

.gitattributes对docs/taskbooks禁用文本换行转换，保留冻结时的精确字节；提交前也核对Git blob与本机锁定哈希一致，不通过重写任务书来消除换行差异。

每次任务或既有heartbeat开始，先遵守额度门禁，再运行：

```powershell
python tools/taskbook/calibrate.py
python tools/taskbook/calibrate.py --init ENG-002 --output docs/task_runs/2026-10-18-003-ENG-002.json
```

核对依赖及前次活动句柄后实施。一个任务可跨周期，每次保存真实状态，不能重复启动已有分析/测试。不把创建实况模板当作产品进展。

实况必须逐条保留冻结条件，并填写提交、范围、命令、PASS/FAIL/NOT_RUN、样本数、证据、偏差、未测与下一步。研究原始响应和厂商派生输出放target/office_research，不加入公开Git；实况引用本地证据文件及公开独立摘要即可。输入身份与具体结果不能只写“通过”。

结束时运行：

```powershell
python tools/taskbook/calibrate.py --report docs/task_runs/<本轮记录>.json
python tools/taskbook/calibrate.py
```

DONE要求该任务的冻结条件均PASS；跨周期未完为PARTIAL，外部阻断为BLOCKED，真实失败为FAILED。脚本校验哈希、Windows属性、条件集合、原文一致和必要字段；证据真实性、依赖是否已完成、阶段放行仍须读取真实日志和文件判断，不把结构检查冒称引擎验收。

哈希或只读属性异常立即停止新工作，单独写偏差，不能解除只读并重算锁。需要改变范围时新建变更提案，保留v1.0，只在用户明确要求修订后建立新版本。普通实现细节在满足原条件时可记为偏差，不需为每一步重复申请。

本轮首份RES-001实况是定稿后的回溯校准；没有伪称此前运行已受本门禁控制。工具专项验证：`python tools/taskbook/test_calibrate.py`，覆盖基准漂移、失去只读、改条件、漏证据、NOT_RUN冒充DONE、错版本和覆盖已有实况等情况。

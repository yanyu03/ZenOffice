# ZenOffice 工作约定

先读README、docs/DEVELOPMENT_DIRECTION_2026-10-18.md与最新基线验收记录。

- 当前仓库是公开的Rust核心基线，目标是Sheets Alpha完整编辑保存流程。先完成M0/M1，不同时扩展三套UI或三个完整Office应用。
- 开发只在本仓库进行；相邻zen-office-v2为历史来源，不覆盖其证据和冻结输入。旧自动化未迁移，不能假设其正在推进本仓库。
- 保留原包与未知部件，未支持编辑明确拒绝，错误与目标锁定不得损坏原件或旧输出。
- 区分代码实现、Rust测试、原生宿主对照、桌面验收。报告必须对应提交、工具链和输入身份，不引用旧通过数充当当前结果。
- 默认检查：python tools/check_baseline.py；cargo build --release -p zen-harness --bin zen-deliver；cargo test --workspace --all-targets。按变更范围选择检查，稳定后的重复全量运行需有新理由。
- 外部SIU模板只可选择性下载并核验，不入Git；外部测试明确ignored，缺少必需自制样本则失败。禁止静默跳过后报告通过。
- 不导入WPS图标、资源包、反编译输出、SDK、现成可执行文件或本机凭据。新UI必须使用来源明确的资源并能在无WPS环境运行。
- BASELINE_IMPORT.json的原始迁入哈希不可重写；后续变更如影响当前校验哈希，必须记录变更原因与新验证身份，再更新对应当前指纹，不篡改原始来源记录。
- 普通源码工作不需要新增自动化；迁移既有自动化前核对其指向目录、待办与用户通知意图。

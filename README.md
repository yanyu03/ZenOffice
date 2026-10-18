# ZenOffice

本地优先的 Rust 办公文件引擎。当前公开基线包含受限 DOCX/PPTX 文字修改、XLSX 数字修改与重算，以及包保持和原子保存接口。

**阶段：实验性引擎基线，尚未发布桌面 Sheets Alpha。** 2026-10-18 的 GitHub Windows 干净环境验收已通过：Release 构建成功，246 项 Rust 测试通过、0 失败、6 项显式忽略。实测提交为 `dffb11a`，范围见[本轮验收记录](docs/BASELINE_ACCEPTANCE_2026-10-18.md)。历史 Office 宿主验收仍只适用于各自冻结版本。

当前按[引擎优先计划](docs/ENGINE_DEVELOPMENT_PLAN_2026-10-18.md)推进：共用会话与安全保存 → 表格基本输入和计算 → Word基础编辑 → Slides基础编辑 → 引擎可用版，再接UI。首个闭环为打开XLSX、修改、重算、撤销/重做、另存并重开核对。完整论文排版与任意Office文件兼容不属于当前承诺。

## 构建与使用

需要支持 Rust 2024 edition 的 Rust 工具链；本轮实测为 Rust/Cargo 1.99.1、Windows MSVC。静态检查需要 Python 3.11+。当前 Cargo.lock 已收敛并通过锁定构建。

```powershell
cargo build --locked --release -p zen-harness --bin zen-deliver
cargo test --locked --workspace --all-targets

# 修改项目自制样本中的一个已有数字，并安全另存
.\target\release\zen-deliver.exe sheet-number --input .\RCD\red-team-corpus\office-targets-2026-10\xlsx\zen_spreadsheet_multiscenario.xlsx --output .\target\edited.xlsx --sheet Transactions --cell D4 --value 42
```

其他命令：`word-structure`、`word-text`、`word-paper-a4`、`slide-text`。不支持的编辑会拒绝；可读或能保留的对象不一定能编辑或由 Zen 正确显示。请使用副本试验。

## 当前边界

- Word：保留原包的运行内文字替换；横向预览仍估宽，已知存在与原生 Word 不同的断行。
- Sheets：受支持普通公式的依赖重算、已有数字修改及支持的数值/图表缓存更新；不能推广为任意文本、公式与结构编辑。
- Slides：受控已有文字替换，保留未投影部件；完整对象编辑尚未完成。
- 默认测试使用项目自制样本；6 个外部模板、本机字体或历史观察测试显式忽略。外部准备方式见测试说明。
- 旧 Qt/egui 前端、WPS 提取资源和逆向输出没有迁入。公开基线不需要安装 WPS 或 Qt。

## 项目记录

- [引擎优先开发计划](docs/ENGINE_DEVELOPMENT_PLAN_2026-10-18.md)与[当前待办](docs/ENGINE_BACKLOG.md)
- [额度报警与任务停止规则](tools/quota_guard/README.md)

- [开发完成度审核](docs/DEVELOPMENT_AUDIT_2026-10-18.md)
- [开发方向与阶段验收](docs/DEVELOPMENT_DIRECTION_2026-10-18.md)
- [测试与外部模板](docs/TESTING.md)
- [来源与公开范围](docs/PROVENANCE.md)
- [迁入前源码哈希与迁移修改](docs/BASELINE_IMPORT.json)

项目源码沿用原工作区声明的 `MIT OR Apache-2.0`，见 LICENSE-MIT、LICENSE-APACHE。第三方依赖保留其各自许可；外部模板不随仓库分发。

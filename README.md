# ZenOffice

本地优先的 Rust 办公文件引擎。当前公开基线包含受限 DOCX/PPTX 文字修改、XLSX 数字修改与重算，以及包保持和原子保存接口。

**阶段：实验性引擎基线，尚未发布桌面 Sheets Alpha。** 首次导入于 2026-10-18；本机缺少 Rust 工具链，尚未对该仓库重新构建或运行 Rust 测试。历史宿主验收只适用于各自冻结版本，不能当作本仓库已经通过的结果。

下一产品目标：打开支持的 XLSX → 修改已有数字 → 重算 → 撤销/重做 → 另存 → 重开核对。Word、Slides 暂保留受控命令行能力。完整论文排版与任意 Office 文件兼容不属于当前承诺。

## 构建与使用

需要支持 Rust 2024 edition 的 Rust 工具链；当前交付验收目标为 Windows。首次构建会依据 Cargo.lock 解析并裁剪旧工作区遗留依赖，生成的新锁文件应随首次构建验证一并提交。

```powershell
cargo build --release -p zen-harness --bin zen-deliver
cargo test --workspace

# 修改项目自制样本中的一个已有数字，并安全另存
.\target\release\zen-deliver.exe sheet-number --input .\RCD\red-team-corpus\office-targets-2026-10\xlsx\zen_spreadsheet_multiscenario.xlsx --output .\target\edited.xlsx --sheet Transactions --cell D4 --value 42
```

其他命令：`word-structure`、`word-text`、`word-paper-a4`、`slide-text`。不支持的编辑会拒绝；可读或能保留的对象不一定能编辑或由 Zen 正确显示。请使用副本试验。

## 当前边界

- Word：保留原包的运行内文字替换；横向预览仍估宽，已知存在与原生 Word 不同的断行。
- Sheets：受支持普通公式的依赖重算、已有数字修改及支持的数值/图表缓存更新；不能推广为任意文本、公式与结构编辑。
- Slides：受控已有文字替换，保留未投影部件；完整对象编辑尚未完成。
- 默认测试使用项目自制样本；3 个依赖外部模板的测试显式忽略，另有历史输入测试显式忽略。外部准备方式见测试说明。
- 旧 Qt/egui 前端、WPS 提取资源和逆向输出没有迁入。公开基线不需要安装 WPS 或 Qt。

## 项目记录

- [开发完成度审核](docs/DEVELOPMENT_AUDIT_2026-10-18.md)
- [开发方向与阶段验收](docs/DEVELOPMENT_DIRECTION_2026-10-18.md)
- [测试与外部模板](docs/TESTING.md)
- [来源与公开范围](docs/PROVENANCE.md)
- [迁入前源码哈希与迁移修改](docs/BASELINE_IMPORT.json)

项目源码沿用原工作区声明的 `MIT OR Apache-2.0`，见 LICENSE-MIT、LICENSE-APACHE。第三方依赖保留其各自许可；外部模板不随仓库分发。

# 测试与证据

2026-10-18 首次导入：没有重新构建或执行 Rust 测试，不能引用原工程的162项通过数作为该仓库结论。

## 默认检查

```powershell
python tools/check_baseline.py
cargo build --release -p zen-harness --bin zen-deliver
cargo test --workspace
```

Python检查仅验证迁入内容和公开范围，不证明产品功能。Rust测试首次成功后记录提交号、rustc -Vv、cargo -V、Cargo.lock、实际通过/忽略数量和原始日志。若构建更新Cargo.lock，提交新锁文件后再记录对应身份。

项目自制DOCX/XLSX/PPTX/OPC与中文论文样本位于RCD/red-team-corpus，原文件哈希写在manifest中。MCE测试已改为相对CARGO_MANIFEST_DIR定位固定XLSX，缺样本明确失败。

## 外部模板测试

SIU模板原manifest明确未观察到单独再分发许可，因此不上传模板。可选从已记录的官方来源下载并核对冻结哈希：

```powershell
python tools/fetch_external_fixture.py
cargo test -p zen-word siu_ -- --ignored
cargo test -p zen-word pinned_documents_export_effective_formats_for_native_audit -- --ignored
```

这三个测试在默认测试输出中明确标为ignored，不代表通过。上游模板改变或链接失效时准备脚本失败，不能悄悄替换冻结基线。历史cycle23输入测试仍忽略；未迁入它依赖的本机Word观察文件，不使用--ignored一次运行所有外部测试。

## 宿主验收

Rust测试不能替代Microsoft Word/Excel/PowerPoint原生对照。原工程的Office自动化、渲染与历史数据未整体公开迁入，当前仓库未取得新的宿主签收。后续在Windows上使用独立实例，对源与输出身份、同等输入修改、计算结果、对象保持和失败保护分别检查，关闭时不保存原件。

## 首次迁移差异

核心src实现逐字节迁入，未修复Word字宽；3个外部模板测试增加ignore原因；1个MCE测试改为必需样本断言；默认PPT主题资源替换为独立组装的ZenOffice主题，新建PPT外观尚未宿主验收。工作区收窄到8个核心crate，Harness只保留zen-deliver。所有修改可在Git首个提交和BASELINE_IMPORT.json中检查。

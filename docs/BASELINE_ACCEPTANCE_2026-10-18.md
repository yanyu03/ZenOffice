# 公开核心基线验收

时间：2026-10-18 21:40（北京时间）。结论：**限定核心基线通过构建与Rust回归；桌面产品与Office原生互通未重新验收。**

## 已验证身份与结果

- 仓库：https://github.com/yanyu03/ZenOffice ，public，默认分支main。
- 实测提交：dffb11aa0c7116d31e2833c0a2e1f48656cef07d。
- [成功的Windows干净环境运行](https://github.com/yanyu03/ZenOffice/actions/runs/39132465604)。
- 工具链：rustc 1.99.1 (58cf8f9c3 2026-10-13)，cargo 1.99.1 (5f94df478 2026-08-27)，x86_64-pc-windows-msvc。
- python tools/check_baseline.py：106项迁入身份、源引用、清单语法与样本ZIP完整性检查通过；本地干净克隆也通过。
- cargo build --locked --release -p zen-harness --bin zen-deliver：PASS。
- cargo test --locked --workspace --all-targets：31个测试二进制，246 passed，0 failed，6 ignored。
- Cargo.lock SHA256：f2aabab57c4af75611034838a57d19d40f03f8a3c85cd53c91537d438ab482ff。本地文件与成功运行上传的锁文件逐字节一致。
- core-tests.log SHA256：20e12f67e5aabb1f0bfb66d355da044c43e84b91dfcee09f9584fa9c9d566127。原始日志与锁文件保存在该运行的core-baseline-evidence附件，链接可能受GitHub保留期限影响。

6个忽略项：3个依赖SIU外部模板；installed_times_variants依赖显式本机字体与身份；frozen_page_bottom_replay依赖本机字体和历史原生观察；frozen_tab_inputs_read_distinct_intervals_without_rewriting依赖cycle23冻结输入。它们均不计通过，准备边界见TESTING.md。

## 保留的首次失败

[首次运行](https://github.com/yanyu03/ZenOffice/actions/runs/39132164910)的Release构建成功，测试编译因迁入遗漏examples/support/fixture.rs而失败。补齐原共享文件、增加Rust静态引用检查、提交核心依赖锁后重新运行通过；没有删除失败断言或缩小测试命令。

## 完成度与下一步

M0的仓库、公开来源边界、干净构建和默认回归基线已建立。当前没有桌面Office编辑器完成的证据；下一项为M1共用应用服务及数字修改、重算、撤销/重做、另存、重开的完整流程。

未测：新的Word/Excel/PowerPoint宿主对照、真实论文全文分页、Word横向字宽修复、替换主题后的新建PPT原生外观、桌面交互、安装包、长时间运行、跨平台与更广泛文件。历史数据保护证据可参考，但不升级为本轮新宿主签收。

后续文档补充提交不改变该实测代码身份。纯文档更新不会重新触发相同全量测试；产品代码、依赖、测试和相关工具变化由工作流重新检查。

原zen-office-v2工程及证据保持，本轮没有迁移或恢复其自动化。新开发目录为ZenOffice，不假设旧自动化会推进新仓库。

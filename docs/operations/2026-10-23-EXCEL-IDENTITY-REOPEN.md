# 用户授权解除Excel身份阻断

日期：2026-10-23。授权原话：“先打开门禁”。设备：Windows本机，具体固件未核验。工具：Windows PowerShell Get-FileHash、Get-AuthenticodeSignature及VersionInfo；Python3.12.14参数预检。样本数：同一明确指定EXCEL.EXE，1次签名/版本采集及1次最终参数复核。是否实测：只读身份检查。结论强度：可作为新的固定宿主参照；不证明Office行为或具体更新来源。

旧记录版本16.0.17932.21000，SHAc71c7ca0faf896fd52e66924f7ec43ab8b7b110fa3b179097c1f0e3c6c036369。当前16.0.17932.21056，SHAc689d889d65140b5d3fe549f2d6bfae22f7084202f749418f1739b5cb6e31ba5，69756656字节，Microsoft Corporation数字签名Valid。文件版本和SHA变化成立；是否Office自动更新造成未单独取证。

独立新参照research/EXCEL-HOST-2026-10-23.json已批准用于后续隔离类型对照；原res-002-v1 manifest、历史失败、冻结任务书/锁不回改。后续observer显式传入此文件已核实path和SHA，启动前/实际自有COM进程均复查；仍禁止自动刷新或放宽身份检查。原生启动/修改0，本次不重试业务。

新profile的首次预检以ProgramFiles为假定root环境变量，在受控子进程KeyError。改用本次已经明确取证的路径参数，最终路径/完整SHA/大小复核PASS；未改系统环境、未选择其他可执行文件、未运行原生任务。首次配置错误保留target/office_research/ENG-005-identity-041。此为新profile代码错误修正，未发生DSHA工具自检拒绝或原native门禁绕过。

本轮仅提交身份参照、运维/实况与前次阻断文档；observe_typed_excel.ps1、verify_typed_excel.py、test_verify_typed_excel.py继续保留本地待验证，不报告工作区全部同步。旧040本地PENDING_SYNC为当时原始快照；本轮新交接覆盖下一步但不抹去它。ENG005仍PARTIAL，C1–C4本轮NOT_RUN。

复现：读取独立profile的path显式参数，通过Get-FileHash/Get-AuthenticodeSignature及VersionInfo核实；private preflight.py只检查完整SHA/大小，不启动应用。回滚：撤销本轮新profile批准并恢复身份阻断，不改旧manifest或冻结锁。三个最易推翻点：有效签名不等于行为等价；版本差异不证明自动更新过程；身份预检不代替COM运行验收。

# openspec-verify-report: desktop-embedded-packaging

日期：2026-09-15。归档前验证（门禁 `openspec-verify-change`）。

## Completeness（完整性）

- `openspec status --change desktop-embedded-packaging --json`：proposal、design、tasks 工件 done；specs 为空（`.openspec.yaml` skip_specs: true，打包是工程设施非业务能力规格，无 delta spec）
- `openspec validate --all`：37 passed, 0 failed
- tasks.md：16 项任务全部勾选，0 项未完成；每项有对应运行证据或代码落点（图标资源、打包配置、本地验证脚本、流水线、证据文件、文档回填）

## Correctness（正确性）

- 本地绿路径：`.deb` 全链路 9/9（`evidence/verify-deb.sh`，control 字段、Depends、图标树、解包启动「主窗口已创建」）
- CI 绿路径：三平台全绿，最终确认运行 `34954258947`（`evidence/ci-final-green.txt`），release 资产 3 平台产物 + 3 组 SHA256SUMS
- CI 红路径：`desktop-gate` 负例运行 `34925224697`，注入 `dead_code` 被 `-D warnings` 拦截（`evidence/gate-negative-log.txt`）
- 双门禁零告警：桌面 `cargo check --release --all-targets --locked` + `-D warnings`；根仓同口径
- 签名缺席降级路径：每次运行即证据（release 正文动态标注未签名）

## Coherence（一致性）

- design.md 与实现一致：D1-D6 决策均有源码/流水线落点；实测偏差已回填（camelCase 键名、Utility 类目、显式 binaries、fontconfig 补清单、@2x 图标命名、staging 按格式收窄、去掉 tag 触发 + concurrency）
- `desktop/README.md` 打包章节与流水线实际命令、secrets 映射一致
- `docs/desktop-gpui-embedded-design.md` §10 与 §12 已按实测修订
- 提交信息正文列出格式裁剪与签名门控的 why（发布路径相关，符合提交纪律）

## 三维结论：PASS，可归档

## 剩余风险（沿袭验证报告）

- dmg/nsis 未经安装器实测（无对应系统环境）
- 签名在场路径待真实证书接入时验证

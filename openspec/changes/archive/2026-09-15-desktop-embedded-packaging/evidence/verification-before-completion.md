# verification-before-completion: desktop-embedded-packaging

日期：2026-09-15。只列本会话真实运行过的命令与结果。

## Verification 列表

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cd desktop && RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked`（桌面门禁口径） | 0 告警，退出码 0 |
| 2 | 根仓 `RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked` | 0 告警，退出码 0（根仓零改动确认） |
| 3 | `cargo build --release --locked`（desktop workspace） | 成功，11m12s |
| 4 | `cargo packager --release --formats deb` | 产出 `kiro-desktop_0.1.0_amd64.deb`（23.7 MB） |
| 5 | `bash evidence/verify-deb.sh <deb>`（dpkg-deb -I/-c + 解包 + Xvfb 启动） | 9/9 PASS：control 字段、Depends 含 8 个运行库、二进制与图标树、解包启动日志「主窗口已创建」 |
| 6 | `openspec validate --all` | 37 passed, 0 failed |
| 7 | `gh run view 34927426607`（三平台矩阵首次全绿运行） | desktop-gate ✓、macos dmg ✓、windows nsis ✓、ubuntu deb ✓、release ✓ |
| 8 | `gh run view 34925224697`（desktop-gate 负例） | 门禁在注入的 `dead_code` 告警处失败退出（证据 `gate-negative-log.txt`），分支已删 |
| 9 | `gh release view desktop-dev-latest` | prerelease 在场，三平台产物 + 三组 SHA256SUMS |
| 10 | YAML 校验（python yaml 解析 desktop-release.yaml） | 3 jobs、矩阵三腿、apt 清单两处均含 `libfontconfig-dev` |

SKIPPED：macOS dmg 与 Windows nsis 的安装器实测（无对应系统环境）；
签名在场路径验证（无证书，只能验证缺席降级路径，该路径每次运行都覆盖）。

## Documentation Sync 表

| 入口 | 是否同步 | 说明 |
| --- | --- | --- |
| `desktop/README.md` | 是 | 打包命令、deb 依赖、secrets 映射表、后续清单 |
| `docs/desktop-gpui-embedded-design.md` | 是 | §10.1/10.2 修正 + §10.4 实测结论回填 |
| 根 `README.md` / `AGENTS.md` | 否 | 打包不改根侧启动/构建/部署/验证命令 |
| `spec/` / `openspec/specs` | 否 | .openspec.yaml skip_specs：打包是工程设施非业务能力规格 |
| `docs/tooling-sources.md` | 否 | 打包工具随 change 工件与 README 记录，不进长期工具清单 |

## Residual Risk

- 三平台产物均未经安装器端到端实测（deb 有解包启动验证；dmg/nsis 仅构建 + 资产完整）
- 签名在场路径未验证：证书接入后首次运行才是该路径的绿路径证据
- Windows 开机自启（winreg）仅编译验证，沿用 change 6 遗留
- release 资产由滚动预发布覆盖，不做长期版本留存（版本策略随首个公开版本另立）

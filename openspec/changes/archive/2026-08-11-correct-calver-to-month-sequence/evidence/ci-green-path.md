# CI 绿路径验证（任务 6.2 - 6.4）

Change：`correct-calver-to-month-sequence`
日期：2026-08-11
状态：**PASS**

## 任务 6.2：Cargo 版本升至 2026.8.11

| 项 | 值 |
| --- | --- |
| PR | https://github.com/wtfdelphia/kiro.rs/pull/16 |
| 合并方式 | merge commit，分支保留，无强推 |
| 合并后 main | `b52cfce` |
| `Cargo.toml` / `Cargo.lock` | `2026.8.10` → `2026.8.11` |
| `cargo update --workspace --offline` | 仅 `kiro-rs` 自身变更，24 个依赖版本未变 |
| `cargo check --release --all-targets` | 零告警 |
| `cargo run --quiet -- --version` | `kiro-rs 2026.8.11` |

版本号语义为「2026 年 8 月第 11 次发布」，非 8 月 11 日。序号接在 `v2026.8.10` 之后保持单调递增。

**跨天验证序号方案的稳定性**：本次改版发生在 2026-08-10，实际发布在 2026-08-11。序号语义下版本号
不受日期变化影响；若仍为日历日语义，跨天会使已确定的版本号失效并需重新改 Cargo 与重跑验证。

## 任务 6.3：绿路径产物链

正式附注 tag `v2026.8.11` 指向 `b52cfce`（`origin/main` HEAD）。推送前本地演练：

```
python scripts/check_release_version.py validate --tag v2026.8.11 --commit b52cfce... --main-ref origin/main
release identity valid: v2026.8.11 (Cargo 2026.8.11)   exit=0
```

| Workflow | Run | 结论 |
| --- | --- | --- |
| Build Artifacts | https://github.com/wtfdelphia/kiro.rs/actions/runs/31450853797 | **全 success**：`pre-check`、`version-gate`、`warning-gate`、7 条产物腿、`release` |
| Build and Push Docker Images | https://github.com/wtfdelphia/kiro.rs/actions/runs/31450853794 | **全 success**：`pre-check`、`version-gate`、`warning-gate`、amd64/arm64 两腿、`manifest` |

两条 workflow 的 `version-gate` 均为 **success**，与同一门禁在红路径（`v2026.8.900`）的 failure 形成
对照：新的序号判定既能放行合法版本，也能拦截失配版本。

### Release 资产

```
name: v2026.8.11 | prerelease: False | draft: False
target: b52cfce3347438ef8fb398ddde07422b02773496
assets:
  kiro-rs-v2026.8.11-Linux-arm64
  kiro-rs-v2026.8.11-Linux-musl-arm64
  kiro-rs-v2026.8.11-Linux-musl-x64
  kiro-rs-v2026.8.11-Linux-x64
  kiro-rs-v2026.8.11-macOS-arm64
  kiro-rs-v2026.8.11-macOS-x64
  kiro-rs-v2026.8.11-Windows-x64.exe
```

7 个资产名均含正式 tag，非 prerelease、非 draft。

### GHCR 镜像

新增 tag：`v2026.8.11`、`latest`、`v2026.8.11-amd64`、`v2026.8.11-arm64`。

## 任务 6.4：该版本包含 Claude Opus 5 支持

| 检查 | 结果 |
| --- | --- |
| `git merge-base --is-ancestor 9d4bdba refs/tags/v2026.8.11^{commit}` | exit 0（Opus 5 提交可从正式 tag 到达） |
| tag 快照中 `converter.rs` 的 `opus-5` 引用数 | 9 处 |

此前发布的 `v2026.8.10` 指向 `674c2cc`，早于 Opus 5 提交 `9d4bdba`，因此不含该能力。`v2026.8.11`
是首个包含 `claude-opus-5` 支持的正式版本。

## 与红路径证据的关系

| 路径 | tag | version-gate | 产物 |
| --- | --- | --- | --- |
| 红 | `v2026.8.900`（Cargo 失配，三位序号） | failure | 全 skipped，无 Release 无镜像 |
| 绿 | `v2026.8.11`（身份一致） | success | 7 腿产物 + Release + 双架构镜像 + manifest |

两条路径使用同一份门禁代码，构成完整的「能拦也能放」证据。`.github/workflows/` 在本 change 中零
改动，故上一轮 `release-version-governance` 的 gate 接线证据继续有效。

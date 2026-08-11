# Verification Before Completion

Change：`correct-calver-to-month-sequence`
日期：2026-08-11
分支：`dev`（`1aa5b53`）；`origin/main` = `b52cfce`
范围：任务 1.1-7.3 完成；7.4 归档待用户确认，7.5 归档时同步 Purpose 段。

## Verification

| 命令 / 检查 | 结果 | 结论 |
| --- | --- | --- |
| `cargo check --release --all-targets` | exit 0，**告警 0** | 与基线 0 一致，零新增 |
| `cargo test --release --locked` | **728 passed**, 0 failed | 与上一 change 后基线持平，无回归（本 change 不改 Rust） |
| `python -m unittest` 门禁三模块 | **48 passed** | 基线 40，新增 8 例（同月多版本、三位序号、月份与序号边界） |
| `cargo metadata --locked --no-deps` | `version 2026.8.11`、`rust_version 1.97.1` | manifest 与锁文件一致，MSRV 未动 |
| `cargo update --workspace --offline` | `kiro-rs v2026.8.10 -> v2026.8.11` | 仅自身变更，24 个依赖未变 |
| `cargo run --quiet -- --version` | `kiro-rs 2026.8.11` | 二进制自报版本等于 Cargo 版本 |
| `check_release_version.py validate --tag v2026.8.11` | `release identity valid: v2026.8.11 (Cargo 2026.8.11)` | 推送前本地演练通过 |
| 红路径 build run `31379254956` | `version-gate` failure、`warning-gate` success、`build`/`release` **skipped** | 门禁可拦 |
| 红路径 docker run `31379254920` | 同上，`build`/`manifest` **skipped** | 两条流水线一致 |
| 红路径 annotation | `Cargo.toml version '2026.8.10' does not match release tag 'v2026.8.900'` | **报失配而非格式错误**，证明三位序号通过新格式判定 |
| 绿路径 build run `31450853797` | 全 success：两 gate + 7 腿产物 + `release` | 门禁可放 |
| 绿路径 docker run `31450853794` | 全 success：两 gate + amd64/arm64 + `manifest` | 多架构发布完成 |
| Release `v2026.8.11` | 7 个 `kiro-rs-v2026.8.11-*` 资产，非 prerelease、非 draft | 资产名与正式 tag 一致 |
| GHCR tag | 新增 `v2026.8.11`、`latest`、`v2026.8.11-amd64`、`v2026.8.11-arm64` | 镜像 tag 与正式 tag 一致 |
| `git merge-base --is-ancestor 9d4bdba v2026.8.11^{commit}` | exit 0；tag 快照 `converter.rs` 含 9 处 `opus-5` | 该正式版本首次包含 Claude Opus 5 支持 |
| `git ls-remote --tags origin \| rg '2026\.8\.(11\|900)'` | 仅 `v2026.8.11` 及其 `^{}` 行 | 临时 tag 已清除；peeled 行同时印证附注 tag 判定机制 |
| `git diff --stat 8d6eeca..HEAD -- .github/` | 空 | workflow 接线零改动 |
| `openspec validate --all` | 22 passed, 0 failed | schema 通过 |
| `git status --short --untracked-files=all` | 仅本 change 的 tasks 与 evidence | 无 `config.json`、`credentials.*`、`.codegraph/` |
| `codegraph sync` | 未执行 | 索引有 pending changes；本 change 不改 Rust，改动点由 rg 精确定位，不 sync 以免混入提交 |
| `actionlint` | 命令不存在 | **SKIPPED**；但本 change `.github/` 零改动，风险不新增 |

### 实现前后行为对照

| tag | 旧门禁 | 新门禁 |
| --- | --- | --- |
| `v2026.8.11`（Cargo 一致） | 通过（但被解读为 8 月 11 日） | 通过（解读为 8 月第 11 次发布） |
| `v2026.8.12` 紧随同月 | **拒绝**（同一自然日限制） | 通过 |
| `v2026.2.30` | 拒绝（非法日历日） | 通过（合法序号） |
| `v2026.8.100` | 拒绝（正则两位上界） | 通过 |
| `v2026.13.1` | 拒绝（`dt.date` 副作用） | 拒绝（显式月份校验） |
| `v2026.8.0` / `v2026.08.11` / `v2026.8.011` | 拒绝 | 拒绝（未放宽） |

## Documentation Sync

| 文档 | 状态 |
| --- | --- |
| `README.md` | 已更新：格式 `vYYYY.MM.MICRO`、明确第三段为当月序号而非日期、移除同日单版本限制、人工发布表述、发版清单增补序号查法 |
| `docs/version-governance-optimization-design.md` | 已更新：顶部增补 2026-08-10 格式纠正记录（含 29 tag 对照、误判成因、calver.org 与 Twisted 依据）；正文 5 处过期结论加「已推翻/已纠正」标注；风险表「同日紧急修复需等待日期变化」标注为已消解 |
| `docs/release-version-governance-remaining-verification.md` | 已更新：加格式纠正说明；红路径示例 tag 由 `v2026.8.11` 换为 `v2026.8.900`（前者已成为本次绿路径目标版本，沿用会造成指引冲突）；`v2026.2.30` 反例改为 `v2026.13.1` |
| `AGENTS.md` | 无需修改。rg 确认不含版本格式约定；本 change 不改变 AI 纪律、告警门禁口径或高风险矩阵 |
| 顶层 `spec/` | 无需修改。rg 确认三份文档均不含版本发布约定 |
| `openspec/project.md` | 无需修改。同上 |
| `openspec/specs/release-version-governance/` | **待归档同步**。delta 已就绪；另需手动修正 `## Purpose` 段的 `vYYYY.M.D`（delta 只含 Requirements 段，不覆盖 Purpose）——task 7.5 |

## Residual Risk

- 历史区段序号语义空洞：`v2026.7.27` 起若干 tag 的第三段源自日期巧合，7 月并未真的发布 31 次。按 D3
  不追溯改写，换取已发布镜像与二进制引用稳定。
- 序号跳号不被门禁拦截（D2 有意设计）。维护者误填只会造成号段不连续，不破坏单调性与唯一性。
- 本机无 actionlint；本 change `.github/` 零改动，风险不新增。
- 主规格 `## Purpose` 段仍为旧格式，须在归档同步时一并修正，否则长期规格内部表述不一致。
- 已发布 GHCR 镜像的 OCI label 未直接 inspect（package 私有，本机 token 无 `read:packages`）；该限制
  沿自上一 change，本轮未改动 Docker 相关代码。

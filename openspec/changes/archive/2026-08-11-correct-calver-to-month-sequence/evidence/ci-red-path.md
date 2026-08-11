# CI 红路径验证（任务 6.1）

Change：`correct-calver-to-month-sequence`
日期：2026-08-10
状态：**PASS**

## 前置：门禁修正必须先在 main

红路径必须在 `main` 已是新门禁的前提下执行。执行前核实 `origin/main` 上仍是旧门禁：

```
git show origin/main:scripts/check_release_version.py | rg "CALVER_TAG|dt.date"
13:CALVER_TAG = re.compile(r"^v(\d{4})\.([1-9]\d?)\.([1-9]\d?)$")
97:        dt.date(year, month, day)
```

若在此状态下推送三位序号的临时 tag，会被**旧正则**以「格式错误」拒掉，从而掩盖真正要验证的
Cargo 一致性判定。故先经 PR #15 将修正合入 main。

| 项 | 值 |
| --- | --- |
| PR | https://github.com/wtfdelphia/kiro.rs/pull/15 |
| 合并方式 | merge commit，分支保留，无强推 |
| 合并后 main | `a5fcdd5` |
| 合并后核实 | `dt.date` 已消失；出现 `vYYYY.MM.MICRO` 文案与 `invalid month ...; month must be 1-12` |

## 临时 tag 设计

`v2026.8.900` 指向 `a5fcdd5`，Cargo 版本为 `2026.8.10`。

该 tag 刻意做到「除 Cargo 一致性外全部合法」：附注 tag、月份 8 在 1-12 内、序号 900 无前导零且
非 0、可从 `origin/main` 到达。序号取三位是关键设计——**旧门禁的 `[1-9]\d?` 会因两位上界直接拒绝
它**，因此若失败原因是格式错误，即证明门禁未生效；只有报 Cargo 失配，才同时证明「新格式放行三位
序号」与「一致性判定仍然生效」两件事。

## Run 结果

| Workflow | Run | 结论 |
| --- | --- | --- |
| Build Artifacts | https://github.com/wtfdelphia/kiro.rs/actions/runs/31379254956 | `pre-check` success、`warning-gate` success、`version-gate` **failure**、`build` **skipped**、`release` **skipped** |
| Build and Push Docker Images | https://github.com/wtfdelphia/kiro.rs/actions/runs/31379254920 | `pre-check` success、`warning-gate` success、`version-gate` **failure**、`build` **skipped**、`manifest` **skipped** |

两条 workflow 的 annotation 原文一致：

```
::error::Cargo.toml version '2026.8.10' does not match release tag 'v2026.8.900';
set [package].version to '2026.8.900', update Cargo.lock, commit, and recreate the tag
```

**关键判定**：失败原因是 Cargo 失配，**不是**格式错误。三位序号 `900` 通过了新的格式判定，证明
`[1-9]\d*` 已解除两位上界，且 `dt.date()` 移除后未引入误判。

## 无发布副作用确认

| 检查 | 结果 |
| --- | --- |
| GitHub Releases | 无 `v2026.8.900`；现存最新正式 Release 仍为 `v2026.8.10` |
| GHCR tag 列表 | 无任何 `v2026.8.900*` |

## 临时 tag 清理

```
git push origin :refs/tags/v2026.8.900   ->  - [deleted]  v2026.8.900
git tag -d v2026.8.900                   ->  Deleted tag (was ea6b219)
git ls-remote --tags origin | rg 2026.8.900  ->  无输出
```

失败的 Actions run 记录保留在历史中，它们即红路径证据本身。

## 与上一轮红路径证据的关系

上一轮 `release-version-governance` 的红路径证明了 gate **接线**正确（needs 依赖使产物 job
skipped）。本轮 `.github/workflows/` 零改动，故接线证据仍然有效；本轮新增的是**判定规则**在新格式
下仍能正确拦截的证据。两者互补，不重复。

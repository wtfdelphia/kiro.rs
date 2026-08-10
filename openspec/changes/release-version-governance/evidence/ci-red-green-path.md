# CI 红/绿路径验证（任务 7.5 / 7.6）

Change：`release-version-governance`
日期：2026-08-10
仓库：`wtfdelphia/kiro.rs`

## 前置：实现合入 main

7.5/7.6 都要求正式 tag 从 `origin/main` 可达，且 `version-gate.yaml` 必须已存在于 `main`。
实现此前只在 `dev-test`，因此先以普通 merge（`--no-ff`）经 PR 合入，无强推、无 reset、无分支删除。

| 项 | 值 |
| --- | --- |
| PR | https://github.com/wtfdelphia/kiro.rs/pull/6 |
| 合并方式 | merge commit（`gh pr merge --merge`），分支保留 |
| 合并后 main | `bf1f0b3` |
| main 上 workflow | `build.yaml`、`docker-build.yaml`、`version-gate.yaml`、`warning-gate.yaml`、`build-dev-release.yaml` |

PR 合并本身触发了 `main` push 的两条正式 workflow，实测证实稳定分支触发器已从 `master` 迁至 `main`。

## 第一轮红路径：暴露门禁缺陷（v2026.8.11）

临时附注 tag `v2026.8.11` 指向 `bf1f0b3`，Cargo 版本为 `2026.8.10`，唯一违规点应是 Cargo 一致性。

| Workflow | Run | 结论 |
| --- | --- | --- |
| Build Artifacts | https://github.com/wtfdelphia/kiro.rs/actions/runs/31362558798 | `pre-check` success、`version-gate` **failure**、`warning-gate` success、`build` **skipped**、`release` **skipped** |
| Build and Push Docker Images | https://github.com/wtfdelphia/kiro.rs/actions/runs/31362558781 | `pre-check` success、`version-gate` **failure**、`warning-gate` success、`build` **skipped**、`manifest` **skipped** |

拦截行为与 skipped 传播完全符合规格，但**失败原因不符合预期**：

```
::error::release tag 'v2026.8.11' must be an annotated tag
```

而本地对同一 tag 执行同一命令报的是预期的 Cargo 失配：

```
::error::Cargo.toml version '2026.8.10' does not match release tag 'v2026.8.11';
set [package].version to '2026.8.11', update Cargo.lock, commit, and recreate the tag
```

远端 tag 对象类型经 API 确认为 `tag`（附注）：
`gh api repos/wtfdelphia/kiro.rs/git/ref/tags/v2026.8.11 --jq '.object'` →
`{"sha":"60e3171...","type":"tag"}`。

### 根因

`version-gate` job 日志显示 `actions/checkout@v5` 在 tag 已存在时执行：

```
git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules \
  origin +bf1f0b3a6ffcd803f48330131f4824d24673cb05:refs/tags/v2026.8.11
```

该 refspec 把 **commit SHA** 覆盖写入本地 `refs/tags/v2026.8.11`，于是
`git cat-file -t refs/tags/<tag>` 返回 `commit`，附注 tag 检查对**任何**正式 tag 都会误判。
本地最小复现已确认：fetch tag ref 后类型为 `tag`，按上述 refspec 再 fetch 一次即变为 `commit`。

影响面：这是先于 Cargo 一致性的判定，因此绿路径（7.5）在修复前必定被误拦。第一轮红路径
虽然「拦住了」，但拦截理由错误，不能作为 Cargo 一致性判定的有效证据。

### 修复

门禁改为从远端权威判定 tag 类型，不再依赖被 checkout 改写的本地 ref：

- `scripts/check_release_version.py` 新增 `tag_object_type()`，通过 `git ls-remote` 的
  peeled `^{}` 行区分附注与轻量 tag；新增 `--remote` 参数；缺省仍走本地 `cat-file`，保持向后兼容
- 必须使用 glob refspec（`refs/tags/<tag>*`）：git 在 refspec 精确命名某个 ref 时会抑制
  peeled 行，而该行是唯一的判定信号
- `rev-parse` 由 `^{}` 改为 `^{commit}`，使解引用对两种 ref 形态都稳定
- `version-gate.yaml` 传入 `--remote origin`
- 新增 4+2 个回归用例：checkout 改写后仍接受合法附注 tag、改写后正确报 Cargo 失配、远端轻量
  tag 被拒、tag 不在远端被拒，以及前缀共享（`v2026.8.1*` 也匹配 `v2026.8.10`）的两个方向

`python -m unittest` 三个模块：**40 passed**（原 23 + 新增回归与既有覆盖）。

## 临时 tag 清理

见本文件末尾「清理确认」小节。
